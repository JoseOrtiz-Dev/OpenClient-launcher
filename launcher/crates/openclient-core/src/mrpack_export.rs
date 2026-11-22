//! Exportación de una instancia a `.mrpack` (formato Modrinth).
//!
//! Flujo: `collect` (sin red, lista blanca) -> `resolve_modrinth` (identifica mods por hash)
//! -> `write` (índice + overrides, escritura atómica). Lo que Modrinth no identifica con
//! sha1, sha512, tamaño y CDN coincidentes va a `overrides/`.

use crate::{
    cancel::guard,
    content_meta::{self, hex_valid, is_link, verified_file, API},
    modpack::{download_url, file_path, loader_version_valid, Loader},
    GameContext,
};
use serde_json::{json, Map, Value};
use std::{
    collections::{HashMap, HashSet},
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};

const FILE_LIMIT: u64 = 128 * 1024 * 1024;
const DOWNLOAD_LIMIT: u64 = 256 * 1024 * 1024;
const OVERRIDES_LIMIT: u64 = 512 * 1024 * 1024;
const FILES_LIMIT: usize = 2500;
/// El importador rechaza archivos con más de 8000 entradas, índice incluido.
const OVERRIDES_ENTRIES: usize = 7999;
const DOWNLOADS_TOTAL_LIMIT: u64 = 4 * 1024 * 1024 * 1024;
const NETWORK_ERROR: &str =
    "No se pudo consultar Modrinth para identificar los mods; no se generó el archivo.";
/// Carpetas que se exportan; `saves` solo con `include_saves`.
const DIRECTORIES: [&str; 6] = [
    "mods",
    "config",
    "defaultconfigs",
    "kubejs",
    "resourcepacks",
    "shaderpacks",
];

pub struct ExportOptions {
    pub include_saves: bool,
    pub version_number: String,
    pub name: String,
}

#[derive(Debug)]
pub struct ExportResult {
    pub file: PathBuf,
    pub files: usize,
    pub overrides: usize,
    /// Rutas (`mods/x.jar`) de jars que no se identificaron y viajan dentro del paquete.
    pub embedded_jars: Vec<String>,
    pub bytes: u64,
}

/// Soporte de cliente/servidor declarado por el proyecto en Modrinth.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnvSupport {
    pub client: &'static str,
    pub server: &'static str,
}

/// Archivo identificado en Modrinth, listo para `files[]`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Resolved {
    pub url: String,
    pub project_id: String,
    pub env: Option<EnvSupport>,
}

/// Archivo de la instancia que puede entrar en el paquete.
#[derive(Clone, Debug)]
pub struct Candidate {
    /// Ruta relativa con `/`.
    pub path: String,
    pub file: PathBuf,
    pub size: u64,
    /// Solo para `mods/*.jar`, `resourcepacks/*.zip` y `shaderpacks/*.zip`.
    pub sha1: Option<String>,
    pub sha512: Option<String>,
    pub resolved: Option<Resolved>,
    /// Lo regenera el launcher (`resourcepacks/OpenClient-*.zip`): se omite si no se resuelve.
    pub generated: bool,
}

fn clean(text: &str) -> String {
    text.chars()
        .filter(|character| !character.is_control())
        .take(80)
        .collect()
}

fn hex_lower(text: &str) -> String {
    text.to_ascii_lowercase()
}

/// Se identifica por hash solo lo que Modrinth distribuye como archivo suelto.
fn resolvable(path: &str) -> bool {
    let Some((folder, name)) = path.split_once('/') else {
        return false;
    };
    let lower = name.to_ascii_lowercase();
    !name.contains('/')
        && match folder {
            "mods" => lower.ends_with(".jar"),
            "resourcepacks" | "shaderpacks" => lower.ends_with(".zip"),
            _ => false,
        }
}

struct Walk<'a> {
    candidates: &'a mut Vec<Candidate>,
}

impl Walk<'_> {
    fn add(&mut self, relative: String, file: PathBuf, size: u64) -> Result<(), String> {
        if self.candidates.len() >= FILES_LIMIT + OVERRIDES_ENTRIES {
            return Err("La instancia tiene demasiados archivos para exportar.".into());
        }
        file_path(Path::new(""), &relative)
            .map_err(|_| format!("No se puede exportar la ruta «{}».", clean(&relative)))?;
        let generated = relative.starts_with("resourcepacks/OpenClient-")
            && !relative["resourcepacks/".len()..].contains('/')
            && relative.to_ascii_lowercase().ends_with(".zip");
        let (sha1, sha512, size) = if resolvable(&relative) && size <= DOWNLOAD_LIMIT {
            let (sha1, sha512, read) = content_meta::hash_file(&file)?;
            (Some(sha1), Some(sha512), read)
        } else {
            (None, None, size)
        };
        self.candidates.push(Candidate {
            path: relative,
            file,
            size,
            sha1,
            sha512,
            resolved: None,
            generated,
        });
        Ok(())
    }

    fn visit(&mut self, relative: &str, absolute: &Path) -> Result<(), String> {
        let metadata = fs::symlink_metadata(absolute).map_err(|error| error.to_string())?;
        if is_link(&metadata) {
            return Err(format!(
                "La instancia contiene un enlace («{}»); no se puede exportar.",
                clean(relative)
            ));
        }
        if metadata.is_file() {
            return self.add(relative.to_owned(), absolute.to_owned(), metadata.len());
        }
        if !metadata.is_dir() {
            return Ok(());
        }
        let mut entries = fs::read_dir(absolute)
            .map_err(|error| error.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?;
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let name = entry.file_name();
            let name = name.to_str().ok_or_else(|| {
                format!("Nombre de archivo no exportable en «{}».", clean(relative))
            })?;
            // Marcadores del launcher, `.git`, etc.; `servers.dat` es privado.
            if name.starts_with('.') || name.eq_ignore_ascii_case("servers.dat") {
                continue;
            }
            self.visit(&format!("{relative}/{name}"), &entry.path())?;
        }
        Ok(())
    }
}

/// Recorre la lista blanca de la instancia (`instances/<id>`) sin tocar la red.
pub fn collect(game: &Path, opts: &ExportOptions) -> Result<Vec<Candidate>, String> {
    let metadata = fs::symlink_metadata(game).map_err(|error| error.to_string())?;
    if is_link(&metadata) || !metadata.is_dir() {
        return Err("La instancia no tiene una carpeta válida.".into());
    }
    let mut candidates = Vec::new();
    let mut walk = Walk {
        candidates: &mut candidates,
    };
    for name in DIRECTORIES
        .into_iter()
        .chain(opts.include_saves.then_some("saves"))
    {
        if fs::symlink_metadata(game.join(name)).is_ok() {
            walk.visit(name, &game.join(name))?;
        }
    }
    if fs::symlink_metadata(game.join("options.txt")).is_ok() {
        walk.visit("options.txt", &game.join("options.txt"))?;
    }
    Ok(candidates)
}

/// Aplica la respuesta de `POST /version_files` (hash -> versión). Solo se acepta un
/// archivo si coinciden sha512, sha1 y tamaño y la URL es del CDN de Modrinth.
pub fn resolve_from_response(cands: &mut [Candidate], response: &Value) {
    let Some(versions) = response.as_object() else {
        return;
    };
    for candidate in cands.iter_mut().filter(|c| c.resolved.is_none()) {
        let (Some(sha1), Some(sha512)) = (&candidate.sha1, &candidate.sha512) else {
            continue;
        };
        let Some(found) = verified_file(versions, sha1, sha512, candidate.size) else {
            continue;
        };
        if let Some(url) = download_url(&found.file["url"]) {
            candidate.resolved = Some(Resolved {
                url: url.to_string(),
                project_id: found.project.to_owned(),
                env: None,
            });
        }
    }
}

fn side(value: &Value) -> Option<&'static str> {
    match value.as_str()? {
        "required" => Some("required"),
        "optional" => Some("optional"),
        "unsupported" => Some("unsupported"),
        _ => None,
    }
}

/// Completa `env` con `GET /projects`. Un proyecto con soporte desconocido queda sin `env`.
///
/// El mod está instalado en el cliente de quien exporta, así que `client: optional` se
/// exporta como `required`; de lo contrario OpenClient lo omitiría al reimportar.
pub fn apply_projects(cands: &mut [Candidate], projects: &Value) {
    let mut env: HashMap<&str, EnvSupport> = HashMap::new();
    for project in projects.as_array().into_iter().flatten() {
        let (Some(id), Some(client), Some(server)) = (
            project["id"].as_str(),
            side(&project["client_side"]),
            side(&project["server_side"]),
        ) else {
            continue;
        };
        let client = if client == "optional" { "required" } else { client };
        env.insert(id, EnvSupport { client, server });
    }
    for resolved in cands.iter_mut().filter_map(|c| c.resolved.as_mut()) {
        resolved.env = env.get(resolved.project_id.as_str()).cloned();
    }
}

/// Identifica los archivos por hash en Modrinth. Si la red falla no hay degradación
/// silenciosa: se devuelve el error y no se genera ningún archivo.
pub async fn resolve_modrinth(
    cands: &mut [Candidate],
    client: &reqwest::Client,
    ctx: &GameContext,
) -> Result<(), String> {
    let mut hashes: Vec<String> = cands
        .iter()
        .filter(|candidate| candidate.resolved.is_none())
        .filter_map(|candidate| candidate.sha512.clone())
        .collect();
    hashes.sort();
    hashes.dedup();
    let batches = hashes.chunks(500).count();
    for (index, batch) in hashes.chunks(500).enumerate() {
        ctx.cancel.check()?;
        let response = guard(&ctx.cancel, async {
            content_meta::post_version_files(client, API, batch)
                .await
                .map_err(|_| NETWORK_ERROR.to_string())
        })
        .await?;
        resolve_from_response(cands, &response);
        ctx.progress(
            "resolving",
            ((index + 1) * 50 / batches).min(100) as u8,
            &format!("Identificando mods en Modrinth ({}/{batches})", index + 1),
        );
    }
    let mut projects: Vec<String> = cands
        .iter()
        .filter_map(|candidate| candidate.resolved.as_ref())
        .map(|resolved| resolved.project_id.clone())
        .collect();
    projects.sort();
    projects.dedup();
    let batches = projects.chunks(100).count();
    for (index, batch) in projects.chunks(100).enumerate() {
        ctx.cancel.check()?;
        let response = guard(&ctx.cancel, async {
            content_meta::get_projects(client, API, batch)
                .await
                .map_err(|_| NETWORK_ERROR.to_string())
        })
        .await?;
        apply_projects(cands, &response);
        ctx.progress(
            "resolving",
            (50 + (index + 1) * 50 / batches).min(100) as u8,
            &format!("Leyendo proyectos de Modrinth ({}/{batches})", index + 1),
        );
    }
    Ok(())
}

fn minecraft_valid(version: &str) -> bool {
    !version.is_empty()
        && version.len() <= 40
        && version.as_bytes()[0].is_ascii_digit()
        && version
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b".-_".contains(&byte))
}

/// Versión del cargador que consta en un perfil de `versions/<mc>/<loader>-<mc>.json`.
pub fn loader_version_from_profile(loader: &str, profile: &Value) -> Option<String> {
    let library = |prefix: &str| {
        profile["libraries"]
            .as_array()?
            .iter()
            .filter_map(|library| library["name"].as_str()?.strip_prefix(prefix))
            .map(|version| version.to_owned())
            .next()
    };
    let game_argument = |flag: &str| {
        let arguments = profile["arguments"]["game"].as_array()?;
        let position = arguments.iter().position(|value| value == flag)?;
        arguments.get(position + 1)?.as_str().map(str::to_owned)
    };
    let id = profile["id"].as_str().unwrap_or_default();
    let found = match loader {
        "fabric" => library("net.fabricmc:fabric-loader:").or_else(|| {
            let rest = id.strip_prefix("fabric-loader-")?;
            let minecraft = profile["inheritsFrom"].as_str()?;
            rest.strip_suffix(&format!("-{minecraft}")).map(str::to_owned)
        }),
        "quilt" => library("org.quiltmc:quilt-loader:").or_else(|| {
            let rest = id.strip_prefix("quilt-loader-")?;
            let minecraft = profile["inheritsFrom"].as_str()?;
            rest.strip_suffix(&format!("-{minecraft}")).map(str::to_owned)
        }),
        "forge" => game_argument("--fml.forgeVersion")
            .or_else(|| id.rsplit_once("-forge-").map(|(_, version)| version.to_owned())),
        "neoforge" => game_argument("--fml.neoForgeVersion")
            .or_else(|| id.rsplit_once("neoforge-").map(|(_, version)| version.to_owned())),
        _ => None,
    };
    found.filter(|version| loader_version_valid(version))
}

fn cached_profile_version(root: &Path, loader: &str, minecraft: &str) -> Option<String> {
    let path = root
        .join("versions")
        .join(minecraft)
        .join(format!("{loader}-{minecraft}.json"));
    let metadata = fs::symlink_metadata(&path).ok()?;
    if is_link(&metadata) || !metadata.is_file() || metadata.len() > 16 * 1024 * 1024 {
        return None;
    }
    let profile: Value = serde_json::from_slice(&fs::read(path).ok()?).ok()?;
    loader_version_from_profile(loader, &profile)
}

/// `dependencies` del índice a partir de la entrada de estado de la instancia.
pub fn loader_dependencies(root: &Path, entry: &Value) -> Result<Map<String, Value>, String> {
    let minecraft = entry["mcVersion"]
        .as_str()
        .filter(|version| minecraft_valid(version))
        .ok_or("La versión de Minecraft de la instancia no es válida.")?;
    let category = entry["category"].as_str().unwrap_or_default();
    let loader = [
        Loader::Vanilla,
        Loader::Fabric,
        Loader::Quilt,
        Loader::Forge,
        Loader::NeoForge,
    ]
    .into_iter()
    .find(|loader| loader.category() == category)
    .ok_or("El cargador de la instancia no es exportable.")?;
    let mut dependencies = Map::new();
    dependencies.insert("minecraft".into(), json!(minecraft));
    let Some(key) = loader.index_key() else {
        return Ok(dependencies);
    };
    let declared = ["fabricVersion", "loaderVersion"]
        .into_iter()
        .map(|field| &entry[field])
        .chain(std::iter::once(&entry["modpack"]["loaderVersion"]))
        .filter_map(Value::as_str)
        .find(|version| !version.is_empty());
    let version = match declared {
        Some(version) if loader_version_valid(version) => Some(version.to_owned()),
        Some(_) => return Err("La versión del cargador de la instancia no es válida.".into()),
        None => cached_profile_version(root, category, minecraft),
    }
    .ok_or("Inicia la instancia una vez para fijar la versión del cargador antes de exportar.")?;
    dependencies.insert(key.into(), json!(version));
    Ok(dependencies)
}

fn text_valid(text: &str) -> bool {
    !text.trim().is_empty() && text.chars().count() <= 64 && !text.chars().any(char::is_control)
}

/// Valida el destino sin crear nada y devuelve el padre canónico y el nombre.
fn checked_destination(dest: &Path, root: &Path) -> Result<(PathBuf, String), String> {
    let invalid = || "Elige un archivo .mrpack con una ruta absoluta válida.".to_string();
    if !dest.is_absolute()
        || !dest
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("mrpack"))
    {
        return Err(invalid());
    }
    let name = dest
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| name.chars().count() <= 120 && file_path(Path::new(""), name).is_ok())
        .ok_or_else(invalid)?
        .to_owned();
    let parent = dest
        .parent()
        .filter(|parent| parent.is_dir())
        .ok_or("La carpeta de destino no existe.")?
        .canonicalize()
        .map_err(|error| error.to_string())?;
    let root = root.canonicalize().map_err(|error| error.to_string())?;
    if parent.starts_with(&root) {
        return Err("Elige una carpeta fuera del directorio de datos de OpenClient.".into());
    }
    match fs::symlink_metadata(parent.join(&name)) {
        Ok(metadata) if is_link(&metadata) || !metadata.is_file() => {
            return Err("El destino no es un archivo.".into())
        }
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
            return Err(error.to_string())
        }
        _ => {}
    }
    Ok((parent, name))
}

/// Borra el temporal si la escritura no llega al `rename`.
struct TempFile(PathBuf);

impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

fn index_entry(candidate: &Candidate, resolved: &Resolved) -> Result<Value, String> {
    let (Some(sha1), Some(sha512)) = (&candidate.sha1, &candidate.sha512) else {
        return Err("Falta el hash de un archivo identificado.".into());
    };
    if !hex_valid(sha1, 40)
        || !hex_valid(sha512, 128)
        || !(1..=DOWNLOAD_LIMIT).contains(&candidate.size)
        || download_url(&Value::String(resolved.url.clone())).is_none()
    {
        return Err(format!(
            "El archivo «{}» no se puede listar como descarga.",
            clean(&candidate.path)
        ));
    }
    let mut entry = json!({
        "path": candidate.path,
        "hashes": {"sha1": hex_lower(sha1), "sha512": hex_lower(sha512)},
        "downloads": [resolved.url],
        "fileSize": candidate.size,
    });
    if let Some(env) = &resolved.env {
        entry["env"] = json!({"client": env.client, "server": env.server});
    }
    Ok(entry)
}

fn add_override<W: Write + std::io::Seek>(
    zip: &mut zip::ZipWriter<W>,
    candidate: &Candidate,
    total: &mut u64,
) -> Result<(), String> {
    let metadata = fs::symlink_metadata(&candidate.file).map_err(|error| error.to_string())?;
    if is_link(&metadata) || !metadata.is_file() {
        return Err(format!(
            "«{}» ya no es un archivo normal.",
            clean(&candidate.path)
        ));
    }
    let size = metadata.len();
    if size > FILE_LIMIT || *total + size > OVERRIDES_LIMIT {
        return Err(format!(
            "El archivo «{}» o el total de archivos incluidos supera el límite del modpack.",
            clean(&candidate.path)
        ));
    }
    *total += size;
    let lower = candidate.path.to_ascii_lowercase();
    let method = if lower.ends_with(".jar") || lower.ends_with(".zip") {
        zip::CompressionMethod::Stored
    } else {
        zip::CompressionMethod::Deflated
    };
    let options = zip::write::SimpleFileOptions::default().compression_method(method);
    zip.start_file(format!("overrides/{}", candidate.path), options)
        .map_err(|error| error.to_string())?;
    let mut input = fs::File::open(&candidate.file)
        .map_err(|error| error.to_string())?
        .take(size + 1);
    let copied = std::io::copy(&mut input, zip).map_err(|error| error.to_string())?;
    if copied != size {
        return Err(format!(
            "«{}» cambió mientras se exportaba.",
            clean(&candidate.path)
        ));
    }
    Ok(())
}

/// Escribe el `.mrpack`: `<padre>/.<nombre>.openclient-<uuid>.tmp` y `rename` al final.
pub fn write(
    dest: &Path,
    root: &Path,
    deps: &Map<String, Value>,
    cands: &[Candidate],
    opts: &ExportOptions,
    ctx: &GameContext,
) -> Result<ExportResult, String> {
    ctx.cancel.check()?;
    if !text_valid(&opts.name) || !text_valid(&opts.version_number) {
        return Err("El nombre o la versión del modpack no son válidos.".into());
    }
    let (parent, name) = checked_destination(dest, root)?;
    let mut downloads = Vec::new();
    let mut embedded = Vec::new();
    let mut seen = HashSet::new();
    for candidate in cands {
        file_path(Path::new(""), &candidate.path)
            .map_err(|_| format!("No se puede exportar la ruta «{}».", clean(&candidate.path)))?;
        if !seen.insert(candidate.path.to_lowercase()) {
            return Err("La instancia contiene rutas duplicadas.".into());
        }
        match &candidate.resolved {
            Some(resolved) => downloads.push((candidate, resolved)),
            None if candidate.generated => {}
            None => embedded.push(candidate),
        }
    }
    if downloads.len() > FILES_LIMIT {
        return Err("La instancia tiene demasiados mods para un .mrpack (máximo 2500).".into());
    }
    if embedded.len() > OVERRIDES_ENTRIES {
        return Err("La instancia tiene demasiados archivos para un .mrpack.".into());
    }
    if downloads.iter().map(|(c, _)| c.size).sum::<u64>() > DOWNLOADS_TOTAL_LIMIT {
        return Err("Los mods descargables superan el límite de 4 GiB.".into());
    }
    let mut files = Vec::with_capacity(downloads.len());
    for (candidate, resolved) in &downloads {
        files.push(index_entry(candidate, resolved)?);
    }
    let index = json!({
        "formatVersion": 1, "game": "minecraft", "versionId": opts.version_number,
        "name": opts.name, "files": files, "dependencies": deps,
    });

    let temp = parent.join(format!(".{name}.openclient-{}.tmp", uuid::Uuid::new_v4()));
    let output = fs::File::create_new(&temp).map_err(|error| error.to_string())?;
    let guard = TempFile(temp.clone());
    let mut zip = zip::ZipWriter::new(output);
    zip.start_file(
        "modrinth.index.json",
        zip::write::SimpleFileOptions::default(),
    )
    .map_err(|error| error.to_string())?;
    zip.write_all(&serde_json::to_vec_pretty(&index).map_err(|error| error.to_string())?)
        .map_err(|error| error.to_string())?;
    let mut total = 0u64;
    for (position, candidate) in embedded.iter().enumerate() {
        ctx.cancel.check()?;
        add_override(&mut zip, candidate, &mut total)?;
        if (position + 1) % 200 == 0 {
            ctx.progress(
                "writing",
                ((position + 1) * 100 / embedded.len()).min(100) as u8,
                &format!("Escribiendo archivos ({}/{})", position + 1, embedded.len()),
            );
        }
    }
    ctx.cancel.check()?;
    let output = zip.finish().map_err(|error| error.to_string())?;
    output.sync_all().map_err(|error| error.to_string())?;
    let bytes = output.metadata().map_err(|error| error.to_string())?.len();
    drop(output);
    let file = parent.join(&name);
    fs::rename(&temp, &file).map_err(|error| format!("No se pudo guardar el archivo: {error}"))?;
    drop(guard);
    let embedded_jars = embedded
        .iter()
        .filter(|candidate| candidate.path.to_ascii_lowercase().ends_with(".jar"))
        .map(|candidate| candidate.path.clone())
        .collect();
    Ok(ExportResult {
        file,
        files: downloads.len(),
        overrides: embedded.len(),
        embedded_jars,
        bytes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        modpack::{Modpack, Origin},
        CancelToken,
    };
    use std::io::Cursor;

    fn ctx() -> GameContext {
        GameContext::new(PathBuf::from("."), "test", |_, _| Ok(()))
    }

    fn temp_dir(label: &str) -> PathBuf {
        let path =
            std::env::temp_dir().join(format!("openclient-export-{label}-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn put(base: &Path, relative: &str, contents: &[u8]) {
        let path = base.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }

    fn opts(include_saves: bool) -> ExportOptions {
        ExportOptions {
            include_saves,
            version_number: "1.0.0".into(),
            name: "Demo".into(),
        }
    }

    fn zip_names(path: &Path) -> Vec<String> {
        let mut archive = zip::ZipArchive::new(fs::File::open(path).unwrap()).unwrap();
        (0..archive.len())
            .map(|index| archive.by_index(index).unwrap().name().unwrap().to_string())
            .collect()
    }

    fn paths(cands: &[Candidate]) -> Vec<String> {
        let mut list: Vec<String> = cands.iter().map(|c| c.path.clone()).collect();
        list.sort();
        list
    }

    fn trap_instance() -> PathBuf {
        let game = temp_dir("trap");
        for (path, data) in [
            ("mods/sodium.jar", "jar"),
            ("config/a.toml", "a"),
            ("config/servers.dat", "secreto"),
            ("config/.oculto", "x"),
            ("options.txt", "fov:70"),
            ("servers.dat", "secreto"),
            ("launcher_state.json", "{}"),
            ("token.txt", "tok"),
            ("logs/latest.log", "log"),
            ("crash-reports/c.txt", "c"),
            ("screenshots/s.png", "s"),
            ("natives/x.dll", "n"),
            (".openclient/modpack.json", "{}"),
            ("mods/.openclient_managed.json", "{}"),
            ("mods/.openclient_versions.json", "{}"),
            ("mods/.alba_x", "x"),
            ("saves/Mundo/level.dat", "w"),
            ("resourcepacks/OpenClient-Base.zip", "z"),
            ("resourcepacks/Mio.zip", "z"),
        ] {
            put(&game, path, data.as_bytes());
        }
        game
    }

    #[test]
    fn collect_never_exports_private_or_regenerable_files() {
        let game = trap_instance();
        let without = paths(&collect(&game, &opts(false)).unwrap());
        assert_eq!(
            without,
            [
                "config/a.toml",
                "mods/sodium.jar",
                "options.txt",
                "resourcepacks/Mio.zip",
                "resourcepacks/OpenClient-Base.zip",
            ]
        );
        let with = paths(&collect(&game, &opts(true)).unwrap());
        assert!(with.contains(&"saves/Mundo/level.dat".to_owned()));
        assert_eq!(with.len(), without.len() + 1);
        // El archivo regenerable solo viaja si Modrinth lo identifica.
        let cands = collect(&game, &opts(false)).unwrap();
        assert!(cands.iter().any(|c| c.generated && c.path.contains("OpenClient-")));
        let deps = json!({"minecraft": "1.21.4"}).as_object().unwrap().clone();
        let out = temp_dir("trap-out");
        let root = temp_dir("trap-root");
        let result = write(&out.join("p.mrpack"), &root, &deps, &cands, &opts(false), &ctx()).unwrap();
        let names = zip_names(&result.file);
        assert!(names.iter().all(|name| name == "modrinth.index.json" || name.starts_with("overrides/")));
        assert!(!names.iter().any(|name| name.contains("OpenClient-") || name.contains("servers.dat")));
        assert_eq!(result.overrides, 4);
        assert_eq!(result.embedded_jars, ["mods/sodium.jar"]);
    }

    #[cfg(unix)]
    #[test]
    fn collect_rejects_symlinks() {
        let game = temp_dir("link");
        let outside = temp_dir("link-out");
        std::os::unix::fs::symlink(&outside, game.join("config")).unwrap();
        assert!(collect(&game, &opts(false)).is_err());
    }

    #[test]
    fn export_roundtrips_through_the_importer() {
        let game = temp_dir("roundtrip");
        put(&game, "mods/local.jar", b"local-jar");
        put(&game, "mods/sodium.jar", b"sodium-jar");
        put(&game, "config/a.toml", b"valor=1");
        put(&game, "options.txt", b"fov:70");
        let mut cands = collect(&game, &opts(false)).unwrap();
        let sodium = cands.iter_mut().find(|c| c.path == "mods/sodium.jar").unwrap();
        sodium.resolved = Some(Resolved {
            url: "https://cdn.modrinth.com/data/AANobbMI/versions/x/sodium.jar".into(),
            project_id: "AANobbMI".into(),
            env: Some(EnvSupport { client: "required", server: "optional" }),
        });
        let sodium_sha512 = sodium.sha512.clone().unwrap();
        let root = temp_dir("roundtrip-root");
        let entry = json!({"mcVersion": "1.21.4", "category": "fabric", "fabricVersion": "0.16.14"});
        let deps = loader_dependencies(&root, &entry).unwrap();
        let out = temp_dir("roundtrip-out");
        let dest = out.join("Demo.mrpack");
        let result = write(&dest, &root, &deps, &cands, &opts(false), &ctx()).unwrap();
        assert_eq!((result.files, result.overrides), (1, 3));
        assert_eq!(result.embedded_jars, ["mods/local.jar"]);
        assert_eq!(result.bytes, fs::metadata(&dest).unwrap().len());
        assert_eq!(fs::read_dir(&out).unwrap().count(), 1, "sin temporales");

        let mut pack = Modpack::open_local(&result.file).unwrap();
        let dependencies = pack.pack_dependencies().unwrap();
        assert_eq!(dependencies.minecraft, "1.21.4");
        assert_eq!(dependencies.loader, Loader::Fabric);
        assert_eq!(dependencies.loader_version.as_deref(), Some("0.16.14"));
        assert_eq!(pack.identity().unwrap(), ("Demo", "1.0.0"));
        let summary = pack.summary().unwrap();
        assert_eq!(summary["mods"], json!(["local", "sodium"]));

        let stage = temp_dir("roundtrip-stage");
        let extracted = pack.extract_overrides(&stage, &ctx()).unwrap();
        assert_eq!(extracted.len(), 3);
        assert!(extracted.iter().all(|file| file.origin == Origin::Override));
        assert_eq!(fs::read(stage.join("config/a.toml")).unwrap(), b"valor=1");
        assert_eq!(fs::read(stage.join("mods/local.jar")).unwrap(), b"local-jar");
        assert!(!stage.join("mods/sodium.jar").exists());

        let mut archive = zip::ZipArchive::new(fs::File::open(&result.file).unwrap()).unwrap();
        let index: Value = serde_json::from_reader(archive.by_name("modrinth.index.json").unwrap()).unwrap();
        let file = &index["files"][0];
        assert_eq!(file["path"], "mods/sodium.jar");
        assert_eq!(file["hashes"]["sha512"], sodium_sha512);
        assert_eq!(file["env"], json!({"client": "required", "server": "optional"}));
        assert_eq!(file["fileSize"], 10);
        assert!(download_url(&file["downloads"][0]).is_some());
    }

    #[test]
    fn mismatched_api_data_sends_the_file_to_overrides() {
        let game = temp_dir("api");
        for name in ["a", "b", "c", "d"] {
            put(&game, &format!("mods/{name}.jar"), name.as_bytes());
        }
        let mut cands = collect(&game, &opts(false)).unwrap();
        let by = |cands: &[Candidate], path: &str| cands.iter().find(|c| c.path == path).unwrap().clone();
        let version = |candidate: &Candidate, sha1: &str, size: u64, url: &str| {
            json!({"project_id": "P1", "files": [{
                "hashes": {"sha1": sha1, "sha512": candidate.sha512}, "size": size, "url": url,
            }]})
        };
        let cdn = "https://cdn.modrinth.com/data/P1/versions/v/x.jar";
        let (a, b, c, d) = (
            by(&cands, "mods/a.jar"),
            by(&cands, "mods/b.jar"),
            by(&cands, "mods/c.jar"),
            by(&cands, "mods/d.jar"),
        );
        let mut response = Map::new();
        response.insert(a.sha512.clone().unwrap(), version(&a, a.sha1.as_ref().unwrap(), a.size, cdn));
        response.insert(b.sha512.clone().unwrap(), version(&b, &"0".repeat(40), b.size, cdn));
        response.insert(c.sha512.clone().unwrap(), version(&c, c.sha1.as_ref().unwrap(), c.size + 1, cdn));
        response.insert(
            d.sha512.clone().unwrap(),
            version(&d, d.sha1.as_ref().unwrap(), d.size, "https://evil.example/d.jar"),
        );
        resolve_from_response(&mut cands, &Value::Object(response));
        let resolved: Vec<&str> = cands
            .iter()
            .filter(|c| c.resolved.is_some())
            .map(|c| c.path.as_str())
            .collect();
        assert_eq!(resolved, ["mods/a.jar"]);
        assert_eq!(cands[0].resolved.as_ref().unwrap().project_id, "P1");
        apply_projects(
            &mut cands,
            &json!([{"id": "P1", "client_side": "optional", "server_side": "unsupported"}]),
        );
        assert_eq!(
            cands[0].resolved.as_ref().unwrap().env,
            Some(EnvSupport { client: "required", server: "unsupported" })
        );
        apply_projects(&mut cands, &json!([{"id": "P1", "client_side": "unknown", "server_side": "required"}]));
        assert_eq!(cands[0].resolved.as_ref().unwrap().env, None);
    }

    #[test]
    fn invalid_destinations_fail_without_creating_files() {
        let game = temp_dir("dest");
        put(&game, "config/a.toml", b"a");
        let cands = collect(&game, &opts(false)).unwrap();
        let root = temp_dir("dest-root");
        let out = temp_dir("dest-out");
        let deps = json!({"minecraft": "1.21.4"}).as_object().unwrap().clone();
        for dest in [
            root.join("p.mrpack"),
            root.join("instances").join("p.mrpack"),
            PathBuf::from("p.mrpack"),
            out.join("p.zip"),
            out.join("p"),
            out.join("faltante").join("p.mrpack"),
            out.join(format!("{}.mrpack", "x".repeat(130))),
            out.join("CON.mrpack"),
            out.clone(),
        ] {
            assert!(write(&dest, &root, &deps, &cands, &opts(false), &ctx()).is_err(), "{dest:?}");
        }
        assert_eq!(fs::read_dir(&out).unwrap().count(), 0);
        assert_eq!(fs::read_dir(&root).unwrap().count(), 0);
        let mut bad = opts(false);
        bad.name = "  ".into();
        assert!(write(&out.join("p.mrpack"), &root, &deps, &cands, &bad, &ctx()).is_err());
        assert_eq!(fs::read_dir(&out).unwrap().count(), 0);
    }

    #[test]
    fn failure_mid_write_leaves_neither_temp_nor_destination() {
        let game = temp_dir("fail");
        put(&game, "config/a.toml", b"a");
        let huge = game.join("config").join("z-grande.bin");
        fs::File::create(&huge).unwrap().set_len(FILE_LIMIT + 1).unwrap();
        let cands = collect(&game, &opts(false)).unwrap();
        assert_eq!(cands.len(), 2);
        let root = temp_dir("fail-root");
        let out = temp_dir("fail-out");
        let deps = json!({"minecraft": "1.21.4"}).as_object().unwrap().clone();
        let dest = out.join("p.mrpack");
        assert!(write(&dest, &root, &deps, &cands, &opts(false), &ctx()).is_err());
        assert_eq!(fs::read_dir(&out).unwrap().count(), 0);

        // Cancelación entre entradas: tampoco queda nada.
        let token = CancelToken::default();
        token.cancel();
        let cancelled = ctx().with_cancel(token);
        let error = write(&dest, &root, &deps, &cands[..1], &opts(false), &cancelled).unwrap_err();
        assert!(error.starts_with("CANCELLED:"));
        assert_eq!(fs::read_dir(&out).unwrap().count(), 0);
    }

    #[test]
    fn loader_dependencies_cover_the_five_loaders() {
        let root = temp_dir("loaders");
        let profile = |loader: &str, value: Value| {
            put(&root, &format!("versions/1.21.4/{loader}-1.21.4.json"), value.to_string().as_bytes());
        };
        profile("fabric", json!({"id": "fabric-loader-0.16.14-1.21.4", "inheritsFrom": "1.21.4"}));
        profile("quilt", json!({"id": "x", "libraries": [{"name": "org.quiltmc:quilt-loader:0.27.1"}]}));
        profile("forge", json!({"id": "1.21.4-forge-54.0.12", "arguments": {"game": []}}));
        profile(
            "neoforge",
            json!({"id": "neoforge-21.4.5", "arguments": {"game": ["--fml.neoForgeVersion", "21.4.5"]}}),
        );
        let entry = |category: &str| json!({"mcVersion": "1.21.4", "category": category});
        let get = |category: &str| loader_dependencies(&root, &entry(category)).unwrap();
        assert_eq!(Value::Object(get("vanilla")), json!({"minecraft": "1.21.4"}));
        assert_eq!(get("fabric")["fabric-loader"], "0.16.14");
        assert_eq!(get("quilt")["quilt-loader"], "0.27.1");
        assert_eq!(get("forge")["forge"], "54.0.12");
        assert_eq!(get("neoforge")["neoforge"], "21.4.5");
        assert_eq!(get("neoforge")["minecraft"], "1.21.4");

        let declared = json!({"mcVersion": "1.21.4", "category": "quilt", "modpack": {"loaderVersion": "0.30.0"}});
        assert_eq!(loader_dependencies(&root, &declared).unwrap()["quilt-loader"], "0.30.0");
        let pinned = json!({"mcVersion": "1.21.4", "category": "fabric", "fabricVersion": "0.17.0"});
        assert_eq!(loader_dependencies(&root, &pinned).unwrap()["fabric-loader"], "0.17.0");

        let empty = temp_dir("loaders-empty");
        let error = loader_dependencies(&empty, &entry("forge")).unwrap_err();
        assert!(error.starts_with("Inicia la instancia"));
        for bad in [
            json!({"mcVersion": "../1", "category": "fabric"}),
            json!({"mcVersion": "1.21.4", "category": "rift"}),
            json!({"mcVersion": "1.21.4", "category": "fabric", "fabricVersion": "../x"}),
        ] {
            assert!(loader_dependencies(&root, &bad).is_err(), "{bad}");
        }
        // Un perfil exportado debe pasar el validador del importador.
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        zip.start_file("modrinth.index.json", zip::write::SimpleFileOptions::default()).unwrap();
        let index = json!({"formatVersion": 1, "game": "minecraft", "name": "n", "versionId": "1",
            "files": [], "dependencies": get("neoforge")});
        zip.write_all(index.to_string().as_bytes()).unwrap();
        let pack = Modpack::open(zip.finish().unwrap().into_inner()).unwrap();
        assert_eq!(pack.pack_dependencies().unwrap().loader, Loader::NeoForge);
    }
}
