//! `content_list` y `content_resolve`: lista unificada de mods, resourcepacks y shaderpacks de una
//! instancia.
//!
//! `content_list` es síncrona y sin red: solo disco, índice y estado. `content_resolve` hace el
//! trabajo caro (hash, lectura del jar, Modrinth) dentro de un presupuesto y devuelve únicamente
//! las filas que cambiaron; la interfaz la repite mientras `pending > 0`.

use crate::{catalog, game::selection};
use openclient_core::{
    content_index::{self, Entry, Index},
    content_meta::{self, clean_text, is_link, valid_id, verified_file, ProjectInfo},
    jar_meta::{self, Kind},
};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fs,
    io::Read,
    path::{Path, PathBuf},
    sync::{Mutex, PoisonError},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

#[cfg(test)]
mod tests;

/// Archivos de disco por categoría; más allá se marca `truncated`.
const DISK_LIMIT: usize = 5000;
/// Por encima de este tamaño no se calcula hash ni se abre el zip.
const HASH_LIMIT: u64 = 512 * 1024 * 1024;
/// Tras un fallo de red no se vuelve a pedir `pending` por red durante este tiempo (segundos).
const OFFLINE_TTL: u64 = 300;
const MANIFEST_LIMIT: u64 = 1024 * 1024;
const DEFAULT_BUDGET_MS: u64 = 1500;

/// Un solo `content_resolve` a la vez por proceso: así no se hashea dos veces lo mismo.
static RESOLVE_LOCK: Mutex<()> = Mutex::new(());

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Category {
    Mods,
    Resourcepacks,
    Shaderpacks,
}

impl Category {
    fn parse(value: &Value) -> Result<Self, String> {
        match value.as_str() {
            Some("mods") => Ok(Self::Mods),
            Some("resourcepacks") => Ok(Self::Resourcepacks),
            Some("shaderpacks") => Ok(Self::Shaderpacks),
            _ => Err("Categoría de contenido inválida.".into()),
        }
    }

    fn dir(self) -> &'static str {
        match self {
            Self::Mods => "mods",
            Self::Resourcepacks => "resourcepacks",
            Self::Shaderpacks => "shaderpacks",
        }
    }

    /// Valor de `type` en `installedContent` y de `kind` en las filas.
    fn kind(self) -> &'static str {
        match self {
            Self::Mods => "mod",
            Self::Resourcepacks => "resourcepack",
            Self::Shaderpacks => "shader",
        }
    }

    fn extension(self) -> &'static str {
        if self == Self::Mods {
            ".jar"
        } else {
            ".zip"
        }
    }
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |elapsed| elapsed.as_secs())
}

fn mtime_ms(metadata: &fs::Metadata) -> u64 {
    metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |elapsed| elapsed.as_millis() as u64)
}

/// `instances/<id>` solo si `id` es un UUID canónico y ni `instances` ni la instancia son enlaces.
fn instance_dir(root: &Path, id: &str) -> Result<PathBuf, String> {
    let canonical = id.len() == 36 && id.bytes().all(|byte| byte.is_ascii_hexdigit() || byte == b'-');
    if !canonical || uuid::Uuid::parse_str(id).is_err() {
        return Err("Identificador de instancia inválido.".into());
    }
    let instances = root.join("instances");
    let game = instances.join(id);
    for path in [&instances, &game] {
        match fs::symlink_metadata(path) {
            Ok(metadata) if is_link(&metadata) || !metadata.is_dir() => {
                return Err("La carpeta de la instancia no es válida.".into())
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.to_string()),
        }
    }
    Ok(game)
}

/// Archivo normal de la carpeta de contenido.
#[derive(Debug)]
struct Disk {
    name: String,
    /// Nombre sin el sufijo `.disabled`.
    base: String,
    enabled: bool,
    size: u64,
    /// Milisegundos UNIX.
    mtime: u64,
    /// Nombre bajo el que se indexa: `base`, salvo un `.disabled` que convive con su `base` activo.
    slot: String,
}

impl Disk {
    fn key(&self, category: Category) -> String {
        content_index::key(category.dir(), &self.slot)
    }
}

/// Lista la carpeta sin seguir enlaces ni entrar en subcarpetas. Un enlace como carpeta es un error.
fn scan(game: &Path, category: Category) -> Result<(Vec<Disk>, bool), String> {
    let folder = game.join(category.dir());
    match fs::symlink_metadata(&folder) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok((Vec::new(), false)),
        Err(error) => return Err(error.to_string()),
        Ok(metadata) if is_link(&metadata) || !metadata.is_dir() => {
            return Err("La carpeta de contenido no es una carpeta normal.".into())
        }
        Ok(_) => {}
    }
    let mut files = Vec::new();
    let mut truncated = false;
    for entry in fs::read_dir(&folder).map_err(|error| error.to_string())?.flatten() {
        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };
        let (base, enabled) = match name.strip_suffix(".disabled") {
            Some(base) => (base.to_owned(), false),
            None => (name.clone(), true),
        };
        if !content_index::valid_file_name(&name)
            || !base.to_ascii_lowercase().ends_with(category.extension())
        {
            continue;
        }
        // `DirEntry::metadata` no sigue enlaces.
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        if is_link(&metadata) || !metadata.is_file() {
            continue;
        }
        if files.len() >= DISK_LIMIT {
            truncated = true;
            break;
        }
        files.push(Disk {
            slot: base.clone(),
            name,
            base,
            enabled,
            size: metadata.len(),
            mtime: mtime_ms(&metadata),
        });
    }
    files.sort_by_cached_key(|file| file.name.to_lowercase());
    let active: HashSet<String> =
        files.iter().filter(|file| file.enabled).map(|file| file.base.to_lowercase()).collect();
    for file in files.iter_mut().filter(|file| !file.enabled) {
        if active.contains(&file.base.to_lowercase()) {
            file.slot = file.name.clone();
        }
    }
    Ok((files, truncated))
}

/// Elemento de `installedContent` del tipo de la categoría.
struct Installed {
    id: String,
    version_id: String,
    title: String,
    version: String,
    enabled: bool,
    source: String,
    file_name: Option<String>,
    raw: Value,
}

fn installed(instance: &Value, category: Category) -> Vec<Installed> {
    instance["installedContent"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|item| item["type"].as_str().unwrap_or("mod") == category.kind())
        .filter_map(|item| {
            let id = item["id"].as_str().filter(|id| valid_id(id))?.to_owned();
            let text = |key: &str| clean_text(item[key].as_str().unwrap_or_default());
            Some(Installed {
                version_id: item["versionId"].as_str().filter(|v| valid_id(v) && *v != "auto").unwrap_or_default().to_owned(),
                title: text("title"),
                version: text("version"),
                enabled: item["enabled"].as_bool().unwrap_or(false),
                source: item["source"].as_str().unwrap_or("modrinth").to_ascii_lowercase(),
                file_name: item["fileName"].as_str().and_then(last_component),
                raw: item.clone(),
                id,
            })
        })
        .take(256)
        .collect()
}

/// Último componente de un nombre de archivo declarado por una fuente externa (como hace sync).
fn last_component(raw: &str) -> Option<String> {
    let name = Path::new(raw).file_name()?.to_str()?;
    content_index::valid_file_name(name).then(|| name.to_owned())
}

fn primary_file(version: &Value) -> Option<&Value> {
    let files = version["files"].as_array()?;
    files.iter().find(|file| file["primary"].as_bool() == Some(true)).or_else(|| files.first())
}

fn pack_name(project: &str, version: &str) -> Option<String> {
    (valid_id(project) && valid_id(version)).then(|| format!("OpenClient-{project}-{version}.zip"))
}

/// Archivo que el sync de la instancia escribiría para un elemento gestionado.
struct Expected {
    name: String,
    sha1: Option<String>,
}

/// Todo lo que no es el listado de la carpeta: estado, caché de versiones y manifiestos de sync.
struct Inputs {
    category: Category,
    installed: Vec<Installed>,
    /// `.openclient_versions.json` (solo mods; `Null` si falta o no es fiable).
    cache: Value,
    /// Nombres (en minúsculas) que escribió el sync.
    manifest: HashSet<String>,
    /// Nombre en minúsculas → sha1 registrado por el sync de packs.
    pack_sha1: HashMap<String, String>,
    /// Nombre en minúsculas → (proyecto, versión, número) según la caché de versiones.
    cached_files: HashMap<String, (String, String, String)>,
}

fn read_small(path: &Path) -> Option<Vec<u8>> {
    let metadata = fs::symlink_metadata(path).ok()?;
    if is_link(&metadata) || !metadata.is_file() || metadata.len() > MANIFEST_LIMIT {
        return None;
    }
    let mut bytes = Vec::new();
    fs::File::open(path).ok()?.take(MANIFEST_LIMIT + 1).read_to_end(&mut bytes).ok()?;
    (bytes.len() as u64 <= MANIFEST_LIMIT).then_some(bytes)
}

impl Inputs {
    fn load(root: &Path, game: &Path, instance: &Value, category: Category) -> Self {
        let folder = game.join(category.dir());
        let mut inputs = Self {
            category,
            installed: installed(instance, category),
            cache: Value::Null,
            manifest: HashSet::new(),
            pack_sha1: HashMap::new(),
            cached_files: HashMap::new(),
        };
        if category == Category::Mods {
            inputs.cache = instance["id"]
                .as_str()
                .and_then(|id| catalog::read_cache(root, id))
                .unwrap_or(Value::Null);
            for version in inputs.cache.as_object().into_iter().flat_map(|cache| cache.values()) {
                let found = primary_file(version)
                    .and_then(|file| last_component(file["filename"].as_str()?))
                    .zip(version["project_id"].as_str().filter(|id| valid_id(id)))
                    .zip(version["id"].as_str().filter(|id| valid_id(id)));
                if let Some(((name, project), id)) = found {
                    let number = clean_text(version["version_number"].as_str().unwrap_or_default());
                    inputs.cached_files.insert(name.to_lowercase(), (project.to_owned(), id.to_owned(), number));
                }
            }
            // Igual que el sync: el nombre antiguo tiene prioridad si existe.
            let legacy = folder.join(".alba_managed.json");
            let path = if legacy.exists() { legacy } else { folder.join(".openclient_managed.json") };
            let names: Vec<String> =
                read_small(&path).and_then(|bytes| serde_json::from_slice(&bytes).ok()).unwrap_or_default();
            inputs.manifest = names.iter().map(|name| name.to_lowercase()).collect();
        } else if let Some(Value::Object(packs)) =
            read_small(&folder.join(".openclient-packs.json")).and_then(|bytes| serde_json::from_slice(&bytes).ok())
        {
            for (name, file) in packs {
                let name = name.to_lowercase();
                if let Some(sha1) = file["sha1"].as_str() {
                    inputs.pack_sha1.insert(name.clone(), sha1.to_owned());
                }
                inputs.manifest.insert(name);
            }
        }
        inputs
    }

    fn expected(&self, item: &Installed) -> Option<Expected> {
        if self.category != Category::Mods {
            let name = pack_name(&item.id, &item.version_id)?;
            return Some(Expected { sha1: self.pack_sha1.get(&name.to_lowercase()).cloned(), name });
        }
        let pinned = catalog::cached_version(&self.cache, &item.raw);
        let auto = (item.version_id.is_empty() || item.raw["versionId"] == "auto").then(|| self.cache.get(item.id.as_str()));
        let file = pinned.or(auto.flatten()).and_then(primary_file);
        let from_cache = file.and_then(|file| {
            Some(Expected {
                name: last_component(file["filename"].as_str()?)?,
                sha1: file["hashes"]["sha1"].as_str().map(str::to_owned),
            })
        });
        from_cache.or_else(|| item.file_name.clone().map(|name| Expected { name, sha1: None }))
    }
}

/// Fila de la lista, antes de serializarla.
#[derive(Default)]
struct Row {
    key: String,
    file_name: String,
    enabled: bool,
    present: bool,
    managed: bool,
    dependency: bool,
    loose: bool,
    title: String,
    author: String,
    version: String,
    project_id: String,
    version_id: String,
    icon: String,
    source: &'static str,
    description: String,
    size: u64,
    modified: u64,
    flags: Vec<&'static str>,
}

impl Row {
    fn flag(&mut self, flag: &'static str) {
        if !self.flags.contains(&flag) {
            self.flags.push(flag);
        }
    }

    fn json(&self, category: Category) -> Value {
        json!({
            "key": self.key, "fileName": self.file_name, "enabled": self.enabled, "present": self.present,
            "managed": self.managed, "dependency": self.dependency, "kind": category.kind(),
            "title": self.title, "author": self.author, "version": self.version,
            "projectId": self.project_id, "versionId": self.version_id, "icon": self.icon,
            "source": self.source, "description": self.description, "size": self.size,
            "modified": self.modified / 1000, "flags": self.flags, "updateAvailable": Value::Null,
        })
    }
}

/// Nombre sin `.disabled`, sin extensión ni códigos `§`.
fn stem(category: Category, name: &str) -> String {
    let name = name.strip_suffix(".disabled").unwrap_or(name);
    let extension = category.extension();
    let name = if name.to_ascii_lowercase().ends_with(extension) { &name[..name.len() - extension.len()] } else { name };
    clean_text(&jar_meta::strip_format(name))
}

/// Prioridad por campo: Modrinth por hash > installedContent > metadatos del jar > nombre de archivo.
fn make_row(category: Category, file: Option<&Disk>, entry: Option<&Entry>, item: Option<&Installed>, name: &str) -> Row {
    let modrinth = entry.is_some_and(|entry| entry.origin == "modrinth");
    let from_entry = |field: fn(&Entry) -> &String| entry.map_or("", |entry| field(entry).as_str());
    let first = |options: [&str; 5]| options.into_iter().find(|text| !text.is_empty()).unwrap_or_default().to_owned();
    let (entry_title, entry_version) = (from_entry(|e| &e.title), from_entry(|e| &e.version));
    let file_stem = stem(category, name);
    let item_title = item.map_or("", |item| item.title.as_str());
    let item_version = item.map_or("", |item| item.version.as_str());
    let source = match entry.map(|entry| entry.origin.as_str()) {
        _ if modrinth || item.is_some_and(|item| item.source == "modrinth") => "modrinth",
        Some("jar") => "jar",
        _ => "file",
    };
    let modrinth_item = item.filter(|item| item.source == "modrinth");
    Row {
        file_name: file.map_or(name, |file| file.name.as_str()).to_owned(),
        title: first([
            if modrinth { entry_title } else { "" },
            item_title,
            entry_title,
            &file_stem,
            item.map_or("", |item| item.id.as_str()),
        ]),
        version: first([if modrinth { entry_version } else { "" }, item_version, entry_version, "", ""]),
        author: from_entry(|e| &e.author).to_owned(),
        project_id: modrinth_item.map_or(from_entry(|e| &e.project_id), |item| item.id.as_str()).to_owned(),
        version_id: first([
            modrinth_item.map_or("", |item| item.version_id.as_str()),
            from_entry(|e| &e.version_id),
            "",
            "",
            "",
        ]),
        icon: from_entry(|e| &e.icon).to_owned(),
        description: from_entry(|e| &e.description).to_owned(),
        source,
        size: file.map_or(0, |file| file.size),
        modified: file.map_or(0, |file| file.mtime),
        ..Row::default()
    }
}

/// Fusiona instalado, manifiestos y disco.
fn rows(inputs: &Inputs, files: &[Disk], index: &Index) -> Vec<Row> {
    let category = inputs.category;
    let entry_of = |file: &Disk| index.entries.get(&file.key(category)).filter(|entry| entry.fresh(file.size, file.mtime));
    // base en minúsculas → (activado, desactivado)
    let mut slots: HashMap<String, (Option<usize>, Option<usize>)> = HashMap::new();
    for (position, file) in files.iter().enumerate() {
        let slot = slots.entry(file.base.to_lowercase()).or_default();
        if file.enabled {
            slot.0 = Some(position);
        } else {
            slot.1 = Some(position);
        }
    }
    let expected: Vec<Option<Expected>> = inputs.installed.iter().map(|item| inputs.expected(item)).collect();
    let claimed: HashSet<String> = expected.iter().flatten().map(|expected| expected.name.to_lowercase()).collect();
    let mut used = vec![false; files.len()];
    let mut keys = HashSet::new();
    let mut out = Vec::new();

    for (item, expected) in inputs.installed.iter().zip(&expected) {
        let key = format!("p:{}", item.id);
        if !keys.insert(key.clone()) {
            continue;
        }
        let slot = expected.as_ref().and_then(|expected| slots.get(&expected.name.to_lowercase()));
        let file = slot.and_then(|slot| slot.0).map(|position| {
            used[position] = true;
            &files[position]
        });
        let entry = file.and_then(entry_of);
        let name = file.map(|file| file.name.as_str()).or(expected.as_ref().map(|e| e.name.as_str())).unwrap_or_default();
        let mut row = make_row(category, file, entry, Some(item), name);
        (row.key, row.managed, row.enabled, row.present) = (key, true, item.enabled, file.is_some());
        let recorded = expected.as_ref().and_then(|expected| expected.sha1.as_deref());
        if let (Some(entry), Some(recorded)) = (entry, recorded) {
            if !entry.sha1.is_empty() && !entry.sha1.eq_ignore_ascii_case(recorded) {
                row.flag("modifiedLocally");
            }
        }
        if file.is_some() && slot.is_some_and(|slot| slot.1.is_some()) {
            row.flag("duplicate");
        }
        out.push(row);
    }

    for file in files.iter().enumerate().filter(|(position, _)| !used[*position]).map(|(_, file)| file) {
        let lower = file.base.to_lowercase();
        let both = slots.get(&lower).is_some_and(|slot| slot.0.is_some() && slot.1.is_some());
        let entry = entry_of(file);
        let mut row = make_row(category, Some(file), entry, None, &file.name);
        let mut key = format!("f:{}", file.slot.to_lowercase());
        (row.enabled, row.present, row.loose) = (file.enabled, true, true);
        if file.enabled && inputs.manifest.contains(&lower) {
            // Dependencia: la escribió el sync pero el usuario no la eligió.
            (row.managed, row.dependency, row.loose) = (true, true, false);
            let cached = inputs.cached_files.get(&lower);
            if let Some((project, version, number)) = cached {
                (row.project_id, row.version_id) = (project.clone(), version.clone());
                if row.version.is_empty() {
                    row.version = number.clone();
                }
            }
            if !row.project_id.is_empty() {
                key = format!("p:{}", row.project_id);
            }
        }
        if !keys.insert(key.clone()) {
            key = format!("f:{}", file.slot.to_lowercase());
            keys.insert(key.clone());
        }
        row.key = key;
        if both {
            row.flag("duplicate");
        }
        if !file.enabled && (inputs.manifest.contains(&lower) || claimed.contains(&lower)) {
            row.flag("nameClash");
        }
        out.push(row);
    }

    // Mismo proyecto en varios archivos: se avisa en los sueltos.
    let mut per_project: HashMap<String, usize> = HashMap::new();
    for row in out.iter().filter(|row| !row.project_id.is_empty()) {
        *per_project.entry(row.project_id.clone()).or_default() += 1;
    }
    for row in out.iter_mut().filter(|row| row.loose && per_project.get(&row.project_id).is_some_and(|count| *count > 1)) {
        row.flag("duplicate");
    }
    out.sort_by_cached_key(|row| (row.title.to_lowercase(), row.key.clone()));
    out
}

/// Archivos que necesitan `content_resolve`: sin entrada válida o, sin conexión pendiente, sin consultar.
fn pending(category: Category, files: &[Disk], index: &Index, now: u64) -> usize {
    let offline = index.offline_at != 0 && now.saturating_sub(index.offline_at) < OFFLINE_TTL;
    files
        .iter()
        .filter(|file| match index.entries.get(&file.key(category)) {
            Some(entry) if entry.fresh(file.size, file.mtime) => !offline && entry.needs_network(now),
            _ => true,
        })
        .count()
}

fn instance_id(instance: &Value) -> Result<&str, String> {
    instance["id"].as_str().ok_or_else(|| "Identificador de instancia inválido.".into())
}

/// `content_list` `{root, id, category}`: sin red ni escrituras.
pub fn list(request: &Value) -> Result<Value, String> {
    let category = Category::parse(&request["category"])?;
    let (root, instance) = selection(request)?;
    list_at(root, &instance, category, now())
}

fn list_at(root: &Path, instance: &Value, category: Category, now: u64) -> Result<Value, String> {
    let game = instance_dir(root, instance_id(instance)?)?;
    let (files, truncated) = scan(&game, category)?;
    let inputs = Inputs::load(root, &game, instance, category);
    let index = content_index::load(&game);
    Ok(json!({
        "category": category.dir(),
        "pending": pending(category, &files, &index, now),
        "offline": index.offline_at != 0,
        "truncated": truncated,
        "items": rows(&inputs, &files, &index).iter().map(|row| row.json(category)).collect::<Vec<_>>(),
    }))
}

/// Conexión a Modrinth; inyectable para probar sin Internet.
pub(crate) struct Net {
    client: reqwest::Client,
    api: String,
}

impl Net {
    fn modrinth() -> Result<Self, String> {
        let client = reqwest::Client::builder()
            .https_only(true)
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(15))
            .user_agent("OpenClient/0.1 (Minecraft desktop launcher)")
            .build()
            .map_err(|_| "No se pudo iniciar la conexión con Modrinth.".to_owned())?;
        Ok(Self { client, api: content_meta::API.to_owned() })
    }
}

/// Hash, lectura del jar y entrada de índice de un archivo; el PNG embebido va aparte.
fn read_entry(path: &Path, category: Category, now: u64) -> Option<(Entry, Option<Vec<u8>>)> {
    let metadata = fs::symlink_metadata(path).ok().filter(|m| !is_link(m) && m.is_file())?;
    let (size, mtime) = (metadata.len(), mtime_ms(&metadata));
    let hashable = size <= HASH_LIMIT;
    let (sha1, sha512) = if hashable {
        let (sha1, sha512, read) = content_meta::hash_file(path).ok()?;
        if read != size {
            return None;
        }
        (sha1, sha512)
    } else {
        Default::default()
    };
    let kind = match category {
        Category::Mods => Some(Kind::Mod),
        Category::Resourcepacks => Some(Kind::ResourcePack),
        Category::Shaderpacks => None,
    };
    let meta = kind.filter(|_| hashable).and_then(|kind| jar_meta::read(path, kind));
    let mut entry = Entry {
        size,
        mtime,
        sha1,
        sha512,
        origin: "file".into(),
        // Lo que no se hashea no se puede consultar: queda resuelto tal cual.
        resolved_at: if hashable { 0 } else { now },
        ..Entry::default()
    };
    let mut icon = None;
    if let Some(meta) = meta {
        entry.origin = "jar".into();
        entry.title = meta.title.unwrap_or_default();
        entry.version = meta.version.unwrap_or_default();
        entry.author = meta.author.unwrap_or_default();
        entry.description = meta.description.unwrap_or_default();
        icon = meta.icon;
    }
    Some((entry, icon))
}

/// Aplica la respuesta de `POST /version_files` a las entradas cuyo hash iba en el lote. Con el
/// mismo criterio que la exportación: sha512, sha1 y tamaño deben coincidir; si no, o si Modrinth
/// no conoce el hash, la entrada queda `notFound` hasta que caduque.
fn apply_version_files<'a>(entries: impl IntoIterator<Item = &'a mut Entry>, response: &Value, now: u64) {
    let Some(versions) = response.as_object() else {
        return;
    };
    for entry in entries {
        entry.resolved_at = now;
        let found = verified_file(versions, &entry.sha1, &entry.sha512, entry.size);
        entry.not_found = found.is_none();
        if let Some(found) = found {
            entry.project_id = found.project.to_owned();
            entry.version_id = found.version["id"].as_str().filter(|id| valid_id(id)).unwrap_or_default().to_owned();
            entry.origin = "modrinth".into();
            let number = clean_text(found.version["version_number"].as_str().unwrap_or_default());
            if !number.is_empty() {
                entry.version = number;
            }
            // Falta título/icono/autor: se completan con `/projects` y `/teams`.
            entry.resolved_at = 0;
        }
    }
}

/// Título e icono del proyecto sobre las entradas (el icono del CDN sustituye al embebido).
fn apply_projects<'a>(entries: impl IntoIterator<Item = &'a mut Entry>, infos: &HashMap<String, ProjectInfo>) {
    for entry in entries {
        let Some(info) = infos.get(&entry.project_id) else {
            continue;
        };
        if !info.title.is_empty() {
            entry.title = info.title.clone();
        }
        if let Some(icon) = &info.icon {
            entry.icon = icon.clone();
        }
    }
}

/// Autor del equipo y marca de «consultado» de las entradas cuyo proyecto se pidió.
fn finish_projects<'a>(
    entries: impl IntoIterator<Item = &'a mut Entry>,
    infos: &HashMap<String, ProjectInfo>,
    authors: &HashMap<String, String>,
    now: u64,
) {
    for entry in entries {
        let author = infos.get(&entry.project_id).and_then(|info| authors.get(info.team.as_deref()?));
        if let Some(author) = author {
            entry.author = author.clone();
        }
        entry.resolved_at = now;
    }
}

fn select<'a>(entries: &'a mut BTreeMap<String, Entry>, keys: &'a HashSet<String>) -> impl Iterator<Item = &'a mut Entry> {
    entries.iter_mut().filter(|(key, _)| keys.contains(*key)).map(|(_, entry)| entry)
}

/// Consulta en lote (`/version_files`, `/projects`, `/teams`). Lo aplicado antes de un fallo se
/// conserva: la siguiente llamada solo repite lo que falta.
async fn network(net: &Net, entries: &mut BTreeMap<String, Entry>, keys: &[String], now: u64) -> Result<(), String> {
    let wanted = |entries: &BTreeMap<String, Entry>, test: &dyn Fn(&Entry) -> bool| -> HashSet<String> {
        keys.iter().filter(|key| entries.get(*key).is_some_and(test)).cloned().collect()
    };
    let by_hash = wanted(entries, &|entry| entry.project_id.is_empty() && !entry.sha512.is_empty());
    let mut hashes: Vec<String> = by_hash.iter().map(|key| entries[key].sha512.clone()).collect();
    hashes.sort();
    hashes.dedup();
    for batch in hashes.chunks(500) {
        let response = content_meta::post_version_files(&net.client, &net.api, batch).await?;
        let batch: HashSet<&String> = batch.iter().collect();
        apply_version_files(select(entries, &by_hash).filter(|entry| batch.contains(&entry.sha512)), &response, now);
    }

    let by_project = wanted(entries, &|entry| !entry.project_id.is_empty() && !entry.not_found && entry.resolved_at == 0);
    let mut ids: Vec<String> = by_project.iter().map(|key| entries[key].project_id.clone()).collect();
    ids.sort();
    ids.dedup();
    let mut infos = HashMap::new();
    for batch in ids.chunks(100) {
        infos.extend(content_meta::project_infos(&content_meta::get_projects(&net.client, &net.api, batch).await?));
    }
    apply_projects(select(entries, &by_project), &infos);
    let mut teams: Vec<String> = infos.values().filter_map(|info| info.team.clone()).collect();
    teams.sort();
    teams.dedup();
    let mut authors = HashMap::new();
    for batch in teams.chunks(100) {
        authors.extend(content_meta::team_authors(&content_meta::get_teams(&net.client, &net.api, batch).await?));
    }
    finish_projects(select(entries, &by_project), &infos, &authors, now);
    Ok(())
}

/// `content_resolve` `{root, id, category, budgetMs?}`.
pub fn resolve(request: &Value) -> Result<Value, String> {
    let category = Category::parse(&request["category"])?;
    let (root, instance) = selection(request)?;
    let budget = Duration::from_millis(request["budgetMs"].as_u64().unwrap_or(DEFAULT_BUDGET_MS).clamp(1, 10_000));
    resolve_at(root, &instance, category, budget, now(), &Net::modrinth()?)
}

fn resolve_at(
    root: &Path,
    instance: &Value,
    category: Category,
    budget: Duration,
    now: u64,
    net: &Net,
) -> Result<Value, String> {
    let _guard = RESOLVE_LOCK.lock().unwrap_or_else(PoisonError::into_inner);
    let game = instance_dir(root, instance_id(instance)?)?;
    let (files, _) = scan(&game, category)?;
    let inputs = Inputs::load(root, &game, instance, category);
    let stored = content_index::load(&game);
    let before: HashMap<String, Value> = rows(&inputs, &files, &stored)
        .iter()
        .map(|row| (row.key.clone(), row.json(category)))
        .collect();
    let mut entries = stored.entries.clone();

    // 1. Hash y lectura del jar, hasta agotar el presupuesto (como mínimo un archivo por llamada).
    let started = Instant::now();
    let folder = game.join(category.dir());
    let mut embedded: HashMap<String, Vec<u8>> = HashMap::new();
    let mut done = 0;
    for file in &files {
        let key = file.key(category);
        if entries.get(&key).is_some_and(|entry| entry.fresh(file.size, file.mtime)) {
            continue;
        }
        if done > 0 && started.elapsed() >= budget {
            break;
        }
        done += 1;
        // Si no se puede leer queda una entrada «sin datos» para no reintentarlo sin fin.
        let (entry, icon) = read_entry(&folder.join(&file.name), category, now).unwrap_or_else(|| {
            let blank = Entry { size: file.size, mtime: file.mtime, origin: "file".into(), resolved_at: now, ..Entry::default() };
            (blank, None)
        });
        entries.insert(key.clone(), entry);
        embedded.extend(icon.map(|png| (key, png)));
    }

    // 2. Red en lote, con un límite de tiempo total; un fallo no aborta, deja `offline`.
    let targets: Vec<String> = files
        .iter()
        .filter(|file| {
            entries.get(&file.key(category)).is_some_and(|entry| entry.fresh(file.size, file.mtime) && entry.needs_network(now))
        })
        .map(|file| file.key(category))
        .collect();
    let mut offline = false;
    if !targets.is_empty() {
        let limit = budget.max(Duration::from_millis(DEFAULT_BUDGET_MS)) * 2;
        let outcome = catalog::run(async {
            Ok::<_, String>(tokio::time::timeout(limit, network(net, &mut entries, &targets, now)).await)
        })?;
        offline = !matches!(outcome, Ok(Ok(())));
    }

    // 3. Iconos embebidos de lo que Modrinth no ilustró.
    for (key, png) in embedded {
        if let Some(entry) = entries.get_mut(&key).filter(|entry| entry.icon.is_empty()) {
            if content_index::save_icon(root, &entry.sha1, &png).is_ok() {
                entry.icon = format!("cache:{}", entry.sha1.to_ascii_lowercase());
            }
        }
    }

    let offline_at = if offline { now } else { 0 };
    let changed: Vec<(String, Entry)> = entries
        .iter()
        .filter(|(key, entry)| stored.entries.get(*key) != Some(*entry))
        .map(|(key, entry)| (key.clone(), entry.clone()))
        .collect();
    if !changed.is_empty() || offline_at != stored.offline_at {
        content_index::update(&game, |index| {
            index.offline_at = offline_at;
            index.entries.extend(changed);
        })?;
    }
    let index = Index { offline_at, entries };
    let items: Vec<Value> = rows(&inputs, &files, &index)
        .iter()
        .map(|row| row.json(category))
        .filter(|row| before.get(row["key"].as_str().unwrap_or_default()) != Some(row))
        .collect();
    Ok(json!({"items": items, "pending": pending(category, &files, &index, now), "offline": offline}))
}
