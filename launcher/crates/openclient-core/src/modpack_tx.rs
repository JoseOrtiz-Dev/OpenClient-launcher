//! Actualizar o restaurar el modpack de una instancia como transacción:
//! todo se valida y descarga en staging, se guarda una copia de seguridad y,
//! si cualquier paso falla, se deshace solo. `recover` repara un crash a medias.
//!
//! Flujo: `stage` (descarga, cancelable) -> `plan` (puro, no escribe) -> `apply`
//! (local, no cancelable) -> `restore` / `list_snapshots` / `recover`.

use crate::{
    modpack::{
        file_path, read_manifest, write_manifest, Modpack, PackDependencies, PackFile, PackManifest,
    },
    GameContext,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha512};
use std::{
    collections::{BTreeSet, HashMap, HashSet},
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

const BACKUP_LIMIT: u64 = 4 * 1024 * 1024 * 1024;
const CONFIG_FULL_LIMIT: u64 = 256 * 1024 * 1024;
const META_LIMIT: u64 = 16 * 1024 * 1024;
const RETAIN: usize = 3;
const MARKER: &str = ".applying";
const OVER_LIMIT: &str = "La copia de seguridad superaría 4 GiB.";
const STALE: &str =
    "La instancia cambió mientras se preparaba la actualización; vuelve a intentarlo.";

/// Resultado de comparar el pack nuevo con el disco. No escribe nada.
#[derive(Clone, Debug)]
pub struct UpdatePlan {
    /// Archivos del pack que no existen todavía.
    pub add: Vec<String>,
    /// Archivos que se sobrescriben (el original queda en el snapshot).
    pub replace: Vec<String>,
    /// Archivos del pack anterior, intactos, que el nuevo ya no trae.
    pub remove: Vec<String>,
    /// Archivos del usuario que se conservan tal cual.
    pub kept: Vec<String>,
    pub new_manifest: PackManifest,
    pub dependencies: PackDependencies,
    /// Subconjunto de `replace` que el usuario había modificado o añadido.
    pub modified: Vec<String>,
    /// Estimación de la copia de seguridad.
    pub backup_bytes: u64,
    /// Hash en disco (None = ausente) de cada ruta tocada; `apply` lo vuelve a comprobar.
    expect: HashMap<String, Option<String>>,
}

#[derive(Clone, Debug, Serialize)]
pub struct SnapshotInfo {
    pub id: String,
    pub created: u64,
    pub from: String,
    pub to: String,
    pub bytes: u64,
    /// Solo `restore`: archivos añadidos por el usuario que no se tocaron.
    pub kept: Vec<String>,
}

#[derive(Clone, Serialize, Deserialize)]
struct Added {
    path: String,
    sha512: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Meta {
    format: u32,
    created: u64,
    from: String,
    to: String,
    entry: Value,
    manifest: Option<PackManifest>,
    added: Vec<Added>,
    saved: Vec<String>,
    #[serde(default)]
    new_dirs: Vec<String>,
    state: String,
    copied: bool,
    bytes: u64,
    #[serde(default)]
    config_partial: bool,
}

// ---------------------------------------------------------------- utilidades

fn is_link(meta: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if meta.file_attributes() & 0x400 != 0 {
            return true;
        }
    }
    meta.file_type().is_symlink()
}

/// `Ok(None)` si no existe; error si es un enlace.
fn plain_meta(path: &Path) -> Result<Option<fs::Metadata>, String> {
    match fs::symlink_metadata(path) {
        Ok(meta) if is_link(&meta) => Err(format!(
            "No se admiten enlaces dentro de la instancia: {}",
            path.display()
        )),
        Ok(meta) => Ok(Some(meta)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.to_string()),
    }
}

fn valid_rel(rel: &str) -> Result<(), String> {
    file_path(Path::new(""), rel).map(|_| ())
}

fn sha_valid(hash: &str) -> bool {
    hash.len() == 128 && hash.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn hash_file(path: &Path) -> Result<String, String> {
    let mut file = fs::File::open(path).map_err(|error| error.to_string())?;
    let mut hasher = Sha512::new();
    let mut buffer = vec![0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(|error| error.to_string())?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

/// Las carpetas intermedias de `rel` no pueden ser enlaces ni archivos.
fn ensure_parents_plain(game: &Path, rel: &str) -> Result<(), String> {
    let mut current = game.to_path_buf();
    let parts: Vec<&str> = rel.split('/').collect();
    for part in &parts[..parts.len().saturating_sub(1)] {
        current.push(part);
        if let Some(meta) = plain_meta(&current)? {
            if !meta.is_dir() {
                return Err(format!("Un archivo ocupa la carpeta de {rel}."));
            }
        }
    }
    Ok(())
}

/// Hash y tamaño de `game/rel`; `None` si no existe.
fn disk_state(game: &Path, rel: &str) -> Result<Option<(String, u64)>, String> {
    ensure_parents_plain(game, rel)?;
    let path = game.join(rel);
    match plain_meta(&path)? {
        None => Ok(None),
        Some(meta) if !meta.is_file() => Err(format!(
            "Una carpeta ocupa la ruta de un archivo del modpack: {rel}"
        )),
        Some(meta) => Ok(Some((hash_file(&path)?, meta.len()))),
    }
}

fn file_exists(game: &Path, rel: &str) -> Result<bool, String> {
    ensure_parents_plain(game, rel)?;
    match plain_meta(&game.join(rel))? {
        None => Ok(false),
        Some(meta) if meta.is_file() => Ok(true),
        Some(_) => Err(format!(
            "Una carpeta ocupa la ruta de un archivo del modpack: {rel}"
        )),
    }
}

/// Archivos normales bajo `base/rel` como rutas relativas con `/`.
/// `strict`: un enlace o nombre inválido es un error; si no, se omite.
fn walk(base: &Path, rel: &str, strict: bool, out: &mut Vec<(String, u64)>) -> Result<(), String> {
    let dir = if rel.is_empty() {
        base.to_path_buf()
    } else {
        base.join(rel)
    };
    let entries = match fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.to_string()),
    };
    for entry in entries {
        let entry = entry.map_err(|error| error.to_string())?;
        let Ok(name) = entry.file_name().into_string() else {
            if strict {
                return Err("Nombre de archivo no válido en la copia de seguridad.".into());
            }
            continue;
        };
        let child = if rel.is_empty() {
            name
        } else {
            format!("{rel}/{name}")
        };
        let meta = fs::symlink_metadata(entry.path()).map_err(|error| error.to_string())?;
        if is_link(&meta) {
            if strict {
                return Err("La copia de seguridad contiene un enlace.".into());
            }
            continue;
        }
        if meta.is_dir() {
            walk(base, &child, strict, out)?;
        } else if meta.is_file() {
            if valid_rel(&child).is_err() {
                if strict {
                    return Err("Ruta no válida en la copia de seguridad.".into());
                }
                continue;
            }
            out.push((child, meta.len()));
        }
        if out.len() > 100_000 {
            return Err("Demasiados archivos en la carpeta.".into());
        }
    }
    Ok(())
}

/// Archivos de `config/` y su tamaño total; `true` si pasa de 256 MiB.
fn config_files(game: &Path) -> Result<(Vec<(String, u64)>, bool), String> {
    let mut files = Vec::new();
    walk(game, "config", false, &mut files)?;
    let total: u64 = files.iter().map(|(_, size)| size).sum();
    Ok((files, total > CONFIG_FULL_LIMIT))
}

fn options_file(game: &Path) -> Result<Option<u64>, String> {
    Ok(plain_meta(&game.join("options.txt"))?
        .filter(fs::Metadata::is_file)
        .map(|meta| meta.len()))
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let temporary = path.with_extension("tmp");
    let result = fs::File::create(&temporary)
        .and_then(|mut file| file.write_all(bytes).and_then(|_| file.sync_all()))
        .and_then(|_| fs::rename(&temporary, path));
    if let Err(error) = result {
        let _ = fs::remove_file(&temporary);
        return Err(format!("No se pudo guardar {}: {error}", path.display()));
    }
    Ok(())
}

fn write_meta(dir: &Path, meta: &Meta) -> Result<(), String> {
    write_atomic(
        &dir.join("meta.json"),
        &serde_json::to_vec(meta).map_err(|error| error.to_string())?,
    )
}

fn read_meta(dir: &Path) -> Result<Option<Meta>, String> {
    let path = dir.join("meta.json");
    let Some(meta) = plain_meta(&path)? else {
        return Ok(None);
    };
    if !meta.is_file() || meta.len() > META_LIMIT {
        return Err("La copia de seguridad está dañada.".into());
    }
    let bytes = fs::read(&path).map_err(|error| error.to_string())?;
    let meta: Meta = serde_json::from_slice(&bytes)
        .map_err(|_| "La copia de seguridad está dañada.".to_string())?;
    if meta.format != 1 {
        return Err("Formato de copia de seguridad incompatible.".into());
    }
    Ok(Some(meta))
}

/// `^[0-9]{1,20}-[0-9a-f]{8}$`
fn valid_snapshot_name(name: &str) -> bool {
    let Some((time, suffix)) = name.split_once('-') else {
        return false;
    };
    (1..=20).contains(&time.len())
        && time.bytes().all(|byte| byte.is_ascii_digit())
        && suffix.len() == 8
        && suffix
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn name_time(name: &str) -> u128 {
    name.split_once('-')
        .and_then(|(time, _)| time.parse().ok())
        .unwrap_or(0)
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_millis())
}

fn label(entry: &Value) -> String {
    entry["modpack"]["versionNumber"]
        .as_str()
        .or_else(|| entry["modpackVersion"].as_str())
        .unwrap_or_default()
        .chars()
        .filter(|c| !c.is_control())
        .take(100)
        .collect()
}

fn find_entry(state_json: &str, id: &str) -> Option<Value> {
    let state: Value = serde_json::from_str(state_json).ok()?;
    state["instances"]
        .as_array()?
        .iter()
        .find(|entry| entry["id"].as_str() == Some(id))
        .cloned()
}

/// Copia a `saved` los campos del vínculo de modpack de la instancia
/// (`modpack`, `modpackVersion`, `mcVersion`, `category`, `fabricVersion`,
/// `loaderVersion`); los demás campos de `current` no se tocan.
pub fn restore_entry_fields(current: &mut Value, saved: &Value) {
    const FIELDS: [&str; 6] = [
        "modpack",
        "modpackVersion",
        "mcVersion",
        "category",
        "fabricVersion",
        "loaderVersion",
    ];
    let Some(target) = current.as_object_mut() else {
        return;
    };
    for field in FIELDS {
        match saved.get(field) {
            Some(value) => {
                target.insert(field.to_owned(), value.clone());
            }
            None => {
                target.remove(field);
            }
        }
    }
}

/// Devuelve la entrada de la instancia a su valor previo en `launcher_state.json`.
fn revert_state(root: &Path, id: &str, entry: &Value) -> Result<(), String> {
    let path = root.join("launcher_state.json");
    let Some(text) = crate::library_state::load_state_at(&path)? else {
        return Ok(());
    };
    let mut state: Value =
        serde_json::from_str(&text).map_err(|_| "La biblioteca guardada es inválida.")?;
    let Some(slot) = state["instances"].as_array_mut().and_then(|items| {
        items
            .iter_mut()
            .find(|item| item["id"].as_str() == Some(id))
    }) else {
        return Ok(());
    };
    if *slot != *entry {
        *slot = entry.clone();
        crate::library_state::write_state_locked(
            &path,
            &serde_json::to_string(&state).map_err(|error| error.to_string())?,
        )?;
    }
    Ok(())
}

struct Loc {
    root: PathBuf,
    game: PathBuf,
    snaps: PathBuf,
}

fn locate(root: &Path, id: &str) -> Result<Loc, String> {
    if !crate::instance_storage::valid_id(id) {
        return Err("Identificador de instancia inválido.".into());
    }
    let root = root.canonicalize().map_err(|error| error.to_string())?;
    let instances = root.join("instances");
    if !plain_meta(&instances)?.is_some_and(|meta| meta.is_dir()) {
        return Err("La biblioteca no tiene instancias.".into());
    }
    let game = instances.join(id);
    if !plain_meta(&game)?.is_some_and(|meta| meta.is_dir()) {
        return Err("La instancia no existe.".into());
    }
    Ok(Loc {
        snaps: root.join("snapshots").join(id),
        root,
        game,
    })
}

fn ensure_snaps(loc: &Loc) -> Result<(), String> {
    let parent = loc.root.join("snapshots");
    for dir in [&parent, &loc.snaps] {
        fs::create_dir_all(dir).map_err(|error| error.to_string())?;
        if !plain_meta(dir)?.is_some_and(|meta| meta.is_dir()) {
            return Err("La carpeta de copias de seguridad no es válida.".into());
        }
    }
    Ok(())
}

fn snapshot_names(snaps: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(snaps) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| valid_snapshot_name(name))
        .collect()
}

fn info_of(name: &str, meta: &Meta) -> SnapshotInfo {
    SnapshotInfo {
        id: name.to_owned(),
        created: meta.created,
        from: meta.from.clone(),
        to: meta.to.clone(),
        bytes: meta.bytes,
        kept: Vec::new(),
    }
}

// --------------------------------------------------------------------- stage

fn absorb(files: &mut Vec<PackFile>, new: Vec<PackFile>) {
    let mut index: HashMap<String, usize> = files
        .iter()
        .enumerate()
        .map(|(position, file)| (file.path.to_lowercase(), position))
        .collect();
    for file in new {
        match index.get(&file.path.to_lowercase()) {
            Some(&position) => files[position] = file,
            None => {
                index.insert(file.path.to_lowercase(), files.len());
                files.push(file);
            }
        }
    }
}

/// Descarga el pack completo (archivos y overrides, con los límites de `modpack.rs`)
/// en `staging`, sin tocar la instancia. Cancelable; el llamador conserva el
/// directorio (`PreparedDirectory`) y su `Drop` lo limpia.
/// Devuelve los archivos que quedan en `staging` (el último gana en rutas repetidas).
pub async fn stage(
    pack: &mut Modpack,
    staging: &Path,
    client: &reqwest::Client,
    ctx: &GameContext,
) -> Result<Vec<PackFile>, String> {
    ctx.cancel.check()?;
    let mut files = pack.install_files(staging, client, ctx).await?;
    let overrides = pack.extract_overrides(staging, ctx)?;
    absorb(&mut files, overrides);
    ctx.cancel.check()?;
    ctx.progress("verifying", 100, "Verificando los archivos del modpack");
    for file in &files {
        valid_rel(&file.path)?;
        let path = file_path(staging, &file.path)?;
        match plain_meta(&path)? {
            Some(meta) if meta.is_file() && meta.len() == file.size => {}
            _ => return Err(format!("Falta un archivo preparado: {}", file.path)),
        }
    }
    Ok(files)
}

// ---------------------------------------------------------------------- plan

fn probe<'a>(
    game: &Path,
    cache: &'a mut HashMap<String, Option<(String, u64)>>,
    path: &str,
) -> Result<&'a Option<(String, u64)>, String> {
    let key = path.to_lowercase();
    if !cache.contains_key(&key) {
        let state = disk_state(game, path)?;
        cache.insert(key.clone(), state);
    }
    Ok(&cache[&key])
}

/// Compara el pack anterior (`old`), el disco y el pack nuevo (`staged`, lo que
/// devolvió `stage`). No escribe nada.
pub fn plan(
    game: &Path,
    old: &PackManifest,
    pack: &Modpack,
    staged: &[PackFile],
    managed: &HashSet<String>,
) -> Result<UpdatePlan, String> {
    let dependencies = pack.pack_dependencies()?;
    let mut new: Vec<PackFile> = Vec::new();
    let mut index: HashMap<String, usize> = HashMap::new();
    for file in staged {
        valid_rel(&file.path)?;
        if !sha_valid(&file.sha512) {
            return Err("Un archivo del modpack no tiene SHA-512 válido.".into());
        }
        match index.get(&file.path.to_lowercase()) {
            Some(&position) => new[position] = file.clone(),
            None => {
                index.insert(file.path.to_lowercase(), new.len());
                new.push(file.clone());
            }
        }
    }
    let managed: HashSet<String> = managed.iter().map(|name| name.to_lowercase()).collect();
    for file in &new {
        let lower = file.path.to_lowercase();
        let name = lower
            .strip_prefix("mods/")
            .filter(|name| !name.contains('/'))
            .filter(|name| managed.contains(*name));
        if name.is_some() || lower.starts_with("resourcepacks/openclient-") {
            let shown = file.path.rsplit('/').next().unwrap_or(&file.path);
            return Err(format!(
                "El modpack choca con contenido añadido: {shown}. Quítalo de la instancia y reintenta."
            ));
        }
    }
    let mut previous: HashMap<String, &PackFile> = HashMap::new();
    for file in &old.files {
        valid_rel(&file.path)?;
        previous.insert(file.path.to_lowercase(), file);
    }

    let mut cache = HashMap::new();
    let (mut add, mut replace, mut remove, mut kept, mut modified) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let mut files = Vec::new();
    for file in &new {
        let key = file.path.to_lowercase();
        let saves = key.starts_with("saves/");
        let origin = previous.get(&key).copied();
        let Some((hash, _)) = probe(game, &mut cache, &file.path)?.clone() else {
            add.push(file.path.clone());
            files.push(file.clone());
            continue;
        };
        if hash.eq_ignore_ascii_case(&file.sha512) {
            if !saves {
                files.push(file.clone());
            }
            continue;
        }
        if saves {
            // Nunca se sobrescribe un mundo.
            kept.push(file.path.clone());
            continue;
        }
        let untouched = origin.is_some_and(|o| o.sha512.eq_ignore_ascii_case(&hash));
        let jar = key.starts_with("mods/") && key.ends_with(".jar");
        let overwrite = if origin.is_some() {
            untouched || jar || key.starts_with("resourcepacks/") || key.starts_with("shaderpacks/")
        } else {
            jar
        };
        if overwrite {
            replace.push(file.path.clone());
            files.push(file.clone());
            if !untouched {
                modified.push(file.path.clone());
            }
        } else {
            kept.push(file.path.clone());
        }
    }
    let mut backup = 0u64;
    for old_file in &old.files {
        let key = old_file.path.to_lowercase();
        if index.contains_key(&key) {
            continue;
        }
        let Some((hash, _)) = probe(game, &mut cache, &old_file.path)?.clone() else {
            continue;
        };
        if hash.eq_ignore_ascii_case(&old_file.sha512) && !key.starts_with("saves/") {
            remove.push(old_file.path.clone());
        } else {
            kept.push(old_file.path.clone());
        }
    }
    let mut expect = HashMap::new();
    for path in &add {
        expect.insert(path.to_lowercase(), None);
    }
    for path in replace.iter().chain(&remove) {
        let (hash, size) = probe(game, &mut cache, path)?
            .clone()
            .ok_or("Un archivo cambió mientras se preparaba la actualización.")?;
        let declared = previous
            .get(&path.to_lowercase())
            .filter(|o| o.sha512.eq_ignore_ascii_case(&hash))
            .map_or(0, |o| o.size);
        backup = backup.saturating_add(size.max(declared));
        expect.insert(path.to_lowercase(), Some(hash));
    }
    let (config, partial) = config_files(game)?;
    if !partial {
        backup = backup.saturating_add(config.iter().map(|(_, size)| size).sum());
    }
    backup = backup.saturating_add(options_file(game)?.unwrap_or(0));
    if backup > BACKUP_LIMIT {
        return Err(OVER_LIMIT.into());
    }
    for list in [
        &mut add,
        &mut replace,
        &mut remove,
        &mut kept,
        &mut modified,
    ] {
        list.sort();
        list.dedup();
    }
    Ok(UpdatePlan {
        add,
        replace,
        remove,
        kept,
        new_manifest: PackManifest { files },
        dependencies,
        modified,
        backup_bytes: backup,
        expect,
    })
}

// -------------------------------------------------------------------- motor

enum Kind {
    Remove,
    Place {
        src: PathBuf,
        /// `true`: se mueve (staging); `false`: se copia (restaurar).
        mv: bool,
        sha: String,
    },
}

struct Op {
    path: String,
    kind: Kind,
}

struct Txn<'a> {
    loc: Loc,
    id: &'a str,
    ops: Vec<Op>,
    manifest: Option<PackManifest>,
    entry_before: &'a Value,
    state_after: &'a str,
    from: String,
    to: String,
}

struct Hooks<'a> {
    ctx: &'a GameContext,
    /// Pruebas: falla antes del paso N.
    fail_after: Option<usize>,
    /// Pruebas: simula la muerte del proceso (sin rollback).
    crash: bool,
}

fn temp_name(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let tag = uuid::Uuid::new_v4().simple().to_string();
    path.with_file_name(format!(".{name}.openclient-{}.tmp", &tag[..8]))
}

/// Copia `src` a `dst` calculando el SHA-512 de lo escrito.
fn copy_hashed(src: &Path, dst: &Path) -> Result<(String, u64), String> {
    let mut input = fs::File::open(src).map_err(|error| error.to_string())?;
    let mut output = fs::File::create(dst).map_err(|error| error.to_string())?;
    let mut hasher = Sha512::new();
    let mut buffer = vec![0u8; 64 * 1024];
    let mut total = 0u64;
    loop {
        let read = input.read(&mut buffer).map_err(|error| error.to_string())?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        output
            .write_all(&buffer[..read])
            .map_err(|error| error.to_string())?;
        total += read as u64;
    }
    output.flush().map_err(|error| error.to_string())?;
    Ok((format!("{:x}", hasher.finalize()), total))
}

/// Coloca `src` en `game/rel`: copia atómica (temporal + rename) o movimiento.
fn place(game: &Path, rel: &str, src: &Path, mv: bool, sha: &str) -> Result<(), String> {
    let dest = file_path(game, rel)?;
    ensure_parents_plain(game, rel)?;
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    if mv {
        return fs::rename(src, &dest)
            .map_err(|error| format!("No se pudo escribir {rel}; cierra Minecraft: {error}"));
    }
    let temporary = temp_name(&dest);
    let result = copy_hashed(src, &temporary).and_then(|(hash, _)| {
        if hash.eq_ignore_ascii_case(sha) {
            fs::rename(&temporary, &dest)
                .map_err(|error| format!("No se pudo escribir {rel}; cierra Minecraft: {error}"))
        } else {
            Err(format!("La copia de {rel} no coincide con su huella."))
        }
    });
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn remove_marker(snaps: &Path) {
    let _ = fs::remove_file(snaps.join(MARKER));
}

fn discard(snaps: &Path, dir: &Path) {
    let _ = fs::remove_dir_all(dir);
    remove_marker(snaps);
}

/// Deshace una transacción a medias. Idempotente. En error, conserva el snapshot.
fn rollback(loc: &Loc, id: &str, dir: &Path, meta: &Meta) -> Result<(), String> {
    let game = &loc.game;
    let mut problems: Vec<String> = Vec::new();
    if meta.copied {
        for added in &meta.added {
            let outcome = valid_rel(&added.path)
                .and_then(|_| disk_state(game, &added.path))
                .and_then(|state| match state {
                    // Solo se borra lo que escribimos nosotros (mismo hash).
                    Some((hash, _)) if hash.eq_ignore_ascii_case(&added.sha512) => {
                        fs::remove_file(game.join(&added.path)).map_err(|e| e.to_string())
                    }
                    _ => Ok(()),
                });
            if let Err(error) = outcome {
                problems.push(error);
            }
        }
        let files = dir.join("files");
        for path in &meta.saved {
            let outcome = valid_rel(path).and_then(|_| {
                let src = file_path(&files, path)?;
                if !plain_meta(&src)?.is_some_and(|m| m.is_file()) {
                    return Err(format!("Falta {path} en la copia de seguridad."));
                }
                let sha = hash_file(&src)?;
                match disk_state(game, path)? {
                    Some((hash, _)) if hash == sha => Ok(()),
                    _ => place(game, path, &src, false, &sha),
                }
            });
            if let Err(error) = outcome {
                problems.push(error);
            }
        }
        let mut dirs: Vec<&String> = meta.new_dirs.iter().collect();
        dirs.sort_by_key(|dir| std::cmp::Reverse(dir.len()));
        for relative in dirs {
            if valid_rel(relative).is_ok() {
                let _ = fs::remove_dir(game.join(relative));
            }
        }
        let manifest = match &meta.manifest {
            Some(previous) => write_manifest(game, previous),
            None => match fs::remove_file(game.join(".openclient").join("modpack.json")) {
                Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
                    Err(error.to_string())
                }
                _ => Ok(()),
            },
        };
        if let Err(error) = manifest {
            problems.push(error);
        }
        if let Err(error) = revert_state(&loc.root, id, &meta.entry) {
            problems.push(error);
        }
    }
    if !problems.is_empty() {
        problems.dedup();
        return Err(format!(
            "{}; tus archivos están en {}",
            problems.join("; "),
            dir.display()
        ));
    }
    let mut done = meta.clone();
    done.state = "rolled-back".into();
    let _ = write_meta(dir, &done);
    remove_marker(&loc.snaps);
    Ok(())
}

fn prune(snaps: &Path) {
    let mut done: Vec<(u128, String)> = Vec::new();
    for name in snapshot_names(snaps) {
        let dir = snaps.join(&name);
        match read_meta(&dir) {
            Ok(Some(meta)) if meta.state == "rolled-back" || meta.state == "applying" => {
                let _ = fs::remove_dir_all(&dir);
            }
            _ => done.push((name_time(&name), name)),
        }
    }
    done.sort_by(|a, b| b.cmp(a));
    for (_, name) in done.into_iter().skip(RETAIN) {
        let _ = fs::remove_dir_all(snaps.join(name));
    }
}

fn next_name(snaps: &Path) -> String {
    let latest = snapshot_names(snaps)
        .iter()
        .map(|name| name_time(name))
        .max()
        .map_or(0, |time| time + 1);
    let tag = uuid::Uuid::new_v4().simple().to_string();
    format!("{}-{}", now_ms().max(latest), &tag[..8])
}

/// Crea el snapshot, aplica las operaciones y escribe manifiesto y estado.
/// Cualquier error posterior al snapshot deshace todo.
fn run(txn: Txn, hooks: &Hooks) -> Result<SnapshotInfo, String> {
    let ctx = hooks.ctx;
    let Txn {
        loc,
        id,
        ops,
        manifest,
        entry_before,
        state_after,
        from,
        to,
    } = txn;
    let game = loc.game.clone();
    let manifest_before = read_manifest(&game)?;

    // Validación previa: nada de esto toca el disco.
    let mut ops_live: Vec<Op> = Vec::new();
    let mut added = Vec::new();
    let mut saved: BTreeSet<String> = BTreeSet::new();
    let mut new_dirs: BTreeSet<String> = BTreeSet::new();
    for op in ops {
        valid_rel(&op.path)?;
        let exists = file_exists(&game, &op.path)?;
        match &op.kind {
            Kind::Remove => {
                if !exists {
                    continue;
                }
                saved.insert(op.path.clone());
            }
            Kind::Place { sha, src, .. } => {
                if !plain_meta(src)?.is_some_and(|meta| meta.is_file()) {
                    return Err(format!("Falta un archivo preparado: {}", op.path));
                }
                if exists {
                    saved.insert(op.path.clone());
                } else {
                    added.push(Added {
                        path: op.path.clone(),
                        sha512: sha.clone(),
                    });
                }
                let parts: Vec<&str> = op.path.split('/').collect();
                for end in 1..parts.len() {
                    let dir = parts[..end].join("/");
                    if fs::symlink_metadata(game.join(&dir)).is_err() {
                        new_dirs.insert(dir);
                    }
                }
            }
        }
        ops_live.push(op);
    }
    let (config, partial) = config_files(&game)?;
    if !partial {
        saved.extend(config.into_iter().map(|(path, _)| path));
    }
    if options_file(&game)?.is_some() {
        saved.insert("options.txt".into());
    }

    // Snapshot (el juego sigue intacto hasta que `copied` sea true).
    ctx.progress("snapshot", 0, "Creando copia de seguridad");
    ensure_snaps(&loc)?;
    let name = next_name(&loc.snaps);
    let dir = loc.snaps.join(&name);
    let marker = loc.snaps.join(MARKER);
    fs::File::create(&marker)
        .and_then(|mut file| {
            file.write_all(name.as_bytes())
                .and_then(|_| file.sync_all())
        })
        .map_err(|error| format!("No se pudo preparar la copia de seguridad: {error}"))?;
    let mut meta = Meta {
        format: 1,
        created: (now_ms() / 1000) as u64,
        from,
        to,
        entry: entry_before.clone(),
        manifest: manifest_before,
        added,
        saved: Vec::new(),
        new_dirs: new_dirs.into_iter().collect(),
        state: "applying".into(),
        copied: false,
        bytes: 0,
        config_partial: partial,
    };
    let snapshot = (|| -> Result<(), String> {
        fs::create_dir(&dir).map_err(|error| error.to_string())?;
        write_meta(&dir, &meta)?;
        let files = dir.join("files");
        let mut bytes = 0u64;
        let total = saved.len().max(1);
        for (position, path) in saved.iter().enumerate() {
            let dest = file_path(&files, path)?;
            if let Some(parent) = dest.parent() {
                fs::create_dir_all(parent).map_err(|error| error.to_string())?;
            }
            ensure_parents_plain(&game, path)?;
            bytes = bytes.saturating_add(
                fs::copy(game.join(path), &dest)
                    .map_err(|error| format!("No se pudo copiar {path}: {error}"))?,
            );
            if bytes > BACKUP_LIMIT {
                return Err(OVER_LIMIT.into());
            }
            if position % 200 == 199 {
                ctx.progress(
                    "snapshot",
                    (position * 100 / total) as u8,
                    "Creando copia de seguridad",
                );
            }
        }
        meta.saved = saved.iter().cloned().collect();
        meta.bytes = bytes;
        meta.copied = true;
        write_meta(&dir, &meta)
    })();
    if let Err(error) = snapshot {
        discard(&loc.snaps, &dir);
        return Err(error);
    }

    // Cambios reales: a partir de aquí cualquier fallo vuelve atrás.
    let mut step = 0usize;
    let mut tick = || -> Result<(), String> {
        if hooks.fail_after == Some(step) {
            return Err("Fallo inyectado por la prueba.".into());
        }
        step += 1;
        Ok(())
    };
    let count = ops_live.len().max(1);
    let result = (|| -> Result<(), String> {
        let ordered = ops_live
            .iter()
            .filter(|op| matches!(op.kind, Kind::Remove))
            .chain(
                ops_live
                    .iter()
                    .filter(|op| !matches!(op.kind, Kind::Remove)),
            );
        for (position, op) in ordered.enumerate() {
            tick()?;
            match &op.kind {
                Kind::Remove => {
                    ensure_parents_plain(&game, &op.path)?;
                    match fs::remove_file(game.join(&op.path)) {
                        Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
                            return Err(format!(
                                "No se pudo quitar {}; cierra Minecraft: {error}",
                                op.path
                            ))
                        }
                        _ => {}
                    }
                }
                Kind::Place { src, mv, sha } => place(&game, &op.path, src, *mv, sha)?,
            }
            if position % 50 == 49 {
                ctx.progress(
                    "applying",
                    (position * 100 / count) as u8,
                    &format!("Aplicando cambios ({}/{count})", position + 1),
                );
            }
        }
        tick()?;
        ctx.progress("writing", 100, "Guardando el modpack de la instancia");
        match &manifest {
            Some(next) => write_manifest(&game, next)?,
            None => match fs::remove_file(game.join(".openclient").join("modpack.json")) {
                Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
                    return Err(error.to_string())
                }
                _ => {}
            },
        }
        tick()?;
        crate::library_state::write_state_locked(
            &loc.root.join("launcher_state.json"),
            state_after,
        )?;
        tick()?;
        meta.state = "done".into();
        write_meta(&dir, &meta)
    })();
    if let Err(error) = result {
        if hooks.crash {
            return Err(error);
        }
        meta.state = "applying".into();
        return match rollback(&loc, id, &dir, &meta) {
            Ok(()) => Err(error),
            Err(problem) => Err(format!("{error}; {problem}")),
        };
    }
    remove_marker(&loc.snaps);
    prune(&loc.snaps);
    Ok(info_of(&name, &meta))
}

// ------------------------------------------------------------------- apply

/// Aplica el plan sobre la instancia. Síncrono, local y no cancelable.
/// `state_before` es la entrada actual de la instancia; `state_after_json`, la
/// biblioteca completa que se guarda al final (el estado se escribe el último).
pub fn apply(
    root: &Path,
    id: &str,
    plan: &UpdatePlan,
    staging: &Path,
    state_before: &Value,
    state_after_json: &str,
) -> Result<SnapshotInfo, String> {
    let ctx = GameContext::new(root.to_path_buf(), env!("CARGO_PKG_VERSION"), |_, _| Ok(()));
    apply_with(
        root,
        id,
        plan,
        staging,
        state_before,
        state_after_json,
        &ctx,
    )
}

/// Como `apply`, notificando las fases (`snapshot`, `applying`, `writing`) por `ctx`.
pub fn apply_with(
    root: &Path,
    id: &str,
    plan: &UpdatePlan,
    staging: &Path,
    state_before: &Value,
    state_after_json: &str,
    ctx: &GameContext,
) -> Result<SnapshotInfo, String> {
    let hooks = Hooks {
        ctx,
        fail_after: None,
        crash: false,
    };
    apply_inner(
        root,
        id,
        plan,
        staging,
        state_before,
        state_after_json,
        &hooks,
    )
}

fn apply_inner(
    root: &Path,
    id: &str,
    plan: &UpdatePlan,
    staging: &Path,
    state_before: &Value,
    state_after_json: &str,
    hooks: &Hooks,
) -> Result<SnapshotInfo, String> {
    let loc = locate(root, id)?;
    recover_at(&loc, id)?;
    if find_entry(state_after_json, id).is_none() {
        return Err("El estado nuevo no incluye la instancia.".into());
    }
    let by_key: HashMap<String, &PackFile> = plan
        .new_manifest
        .files
        .iter()
        .map(|file| (file.path.to_lowercase(), file))
        .collect();
    let mut ops = Vec::new();
    let mut seen = HashSet::new();
    for path in &plan.remove {
        valid_rel(path)?;
        let key = path.to_lowercase();
        let hash = plan
            .expect
            .get(&key)
            .and_then(Option::as_ref)
            .ok_or("Plan de actualización inválido.")?;
        if by_key.contains_key(&key) || !seen.insert(key) {
            return Err("Plan de actualización inválido.".into());
        }
        match disk_state(&loc.game, path)? {
            Some((current, _)) if current.eq_ignore_ascii_case(hash) => {}
            _ => return Err(STALE.into()),
        }
        ops.push(Op {
            path: path.clone(),
            kind: Kind::Remove,
        });
    }
    for path in plan.replace.iter().chain(&plan.add) {
        valid_rel(path)?;
        let key = path.to_lowercase();
        let file = by_key.get(&key).ok_or("Plan de actualización inválido.")?;
        let expected = plan
            .expect
            .get(&key)
            .ok_or("Plan de actualización inválido.")?;
        if !seen.insert(key) {
            return Err("Plan de actualización inválido.".into());
        }
        let current = disk_state(&loc.game, path)?;
        let unchanged = match (&current, expected) {
            (None, None) => true,
            (Some((hash, _)), Some(expected)) => hash.eq_ignore_ascii_case(expected),
            _ => false,
        };
        if !unchanged {
            return Err(STALE.into());
        }
        let src = file_path(staging, path)?;
        if !plain_meta(&src)?.is_some_and(|meta| meta.is_file() && meta.len() == file.size) {
            return Err(format!("Falta un archivo preparado: {path}"));
        }
        ops.push(Op {
            path: (*path).clone(),
            kind: Kind::Place {
                src,
                mv: true,
                sha: file.sha512.clone(),
            },
        });
    }
    run(
        Txn {
            loc,
            id,
            ops,
            manifest: Some(plan.new_manifest.clone()),
            entry_before: state_before,
            state_after: state_after_json,
            from: label(state_before),
            to: find_entry(state_after_json, id)
                .map(|entry| label(&entry))
                .unwrap_or_default(),
        },
        hooks,
    )
}

// ------------------------------------------------------- snapshots y recover

/// Copias de seguridad completadas, la más reciente primero.
pub fn list_snapshots(root: &Path, id: &str) -> Result<Vec<SnapshotInfo>, String> {
    let loc = locate(root, id)?;
    if plain_meta(&loc.snaps)?.is_none() {
        return Ok(Vec::new());
    }
    let mut found = Vec::new();
    for name in snapshot_names(&loc.snaps) {
        let dir = loc.snaps.join(&name);
        if !plain_meta(&dir)?.is_some_and(|meta| meta.is_dir()) {
            continue;
        }
        if let Ok(Some(meta)) = read_meta(&dir) {
            if meta.state == "done" {
                found.push((name_time(&name), info_of(&name, &meta)));
            }
        }
    }
    found.sort_by(|a, b| (b.0, &b.1.id).cmp(&(a.0, &a.1.id)));
    Ok(found.into_iter().map(|(_, info)| info).collect())
}

/// Si una transacción murió a medias (marcador `.applying`), la deshace.
/// `Ok(true)` si hubo algo que reparar. Idempotente y barato cuando no hay marcador.
pub fn recover(root: &Path, id: &str) -> Result<bool, String> {
    if !crate::instance_storage::valid_id(id) {
        return Err("Identificador de instancia inválido.".into());
    }
    if fs::symlink_metadata(root.join("snapshots").join(id).join(MARKER)).is_err() {
        return Ok(false);
    }
    recover_at(&locate(root, id)?, id)
}

fn recover_at(loc: &Loc, id: &str) -> Result<bool, String> {
    let marker = loc.snaps.join(MARKER);
    if plain_meta(&loc.snaps)?.is_none() || plain_meta(&marker)?.is_none() {
        return Ok(false);
    }
    let name = fs::read_to_string(&marker).unwrap_or_default();
    let name = name.trim();
    if !valid_snapshot_name(name) {
        remove_marker(&loc.snaps);
        return Ok(true);
    }
    let dir = loc.snaps.join(name);
    if plain_meta(&dir)?.is_none() {
        remove_marker(&loc.snaps);
        return Ok(true);
    }
    match read_meta(&dir)? {
        // Murió antes de guardar `meta.json`: el juego no se había tocado.
        None => {
            discard(&loc.snaps, &dir);
            Ok(true)
        }
        Some(meta) if meta.state == "done" || meta.state == "rolled-back" => {
            remove_marker(&loc.snaps);
            Ok(false)
        }
        Some(meta) => {
            rollback(loc, id, &dir, &meta)?;
            Ok(true)
        }
    }
}

/// Vuelve a una copia de seguridad. Antes guarda otra del estado actual, así que
/// restaurar también se puede deshacer. `state_json_for` recibe la entrada que
/// tenía la instancia en la copia y devuelve la biblioteca completa a guardar
/// (véase `restore_entry_fields`). `saves/` nunca se toca.
pub fn restore(
    root: &Path,
    id: &str,
    snapshot: &str,
    state_json_for: impl FnOnce(&Value) -> Result<String, String>,
) -> Result<SnapshotInfo, String> {
    let ctx = GameContext::new(root.to_path_buf(), env!("CARGO_PKG_VERSION"), |_, _| Ok(()));
    restore_with(root, id, snapshot, state_json_for, &ctx)
}

/// Como `restore`, notificando las fases por `ctx`.
pub fn restore_with(
    root: &Path,
    id: &str,
    snapshot: &str,
    state_json_for: impl FnOnce(&Value) -> Result<String, String>,
    ctx: &GameContext,
) -> Result<SnapshotInfo, String> {
    let hooks = Hooks {
        ctx,
        fail_after: None,
        crash: false,
    };
    restore_inner(root, id, snapshot, state_json_for, &hooks)
}

fn restore_inner(
    root: &Path,
    id: &str,
    snapshot: &str,
    state_json_for: impl FnOnce(&Value) -> Result<String, String>,
    hooks: &Hooks,
) -> Result<SnapshotInfo, String> {
    if !valid_snapshot_name(snapshot) {
        return Err("Identificador de copia de seguridad inválido.".into());
    }
    let loc = locate(root, id)?;
    recover_at(&loc, id)?;
    let dir = loc.snaps.join(snapshot);
    let canonical = dir
        .canonicalize()
        .map_err(|_| "La copia de seguridad ya no existe.".to_string())?;
    let parent = loc.snaps.canonicalize().map_err(|e| e.to_string())?;
    if canonical.parent() != Some(parent.as_path())
        || !plain_meta(&dir)?.is_some_and(|meta| meta.is_dir())
    {
        return Err("Copia de seguridad inválida.".into());
    }
    let meta = read_meta(&dir)?.ok_or("La copia de seguridad ya no existe.")?;
    if meta.state != "done" {
        return Err("La copia de seguridad no está disponible.".into());
    }
    let state_text = crate::library_state::load_state_at(&loc.root.join("launcher_state.json"))?
        .ok_or("La biblioteca aún no existe.")?;
    let current = find_entry(&state_text, id).ok_or("La instancia no está en la biblioteca.")?;

    let files = dir.join("files");
    let mut listed = Vec::new();
    walk(&files, "", true, &mut listed)?;
    let mut ops = Vec::new();
    for (rel, _) in listed {
        let src = file_path(&files, &rel)?;
        let sha = hash_file(&src)?;
        if matches!(disk_state(&loc.game, &rel)?, Some((hash, _)) if hash == sha) {
            continue;
        }
        ops.push(Op {
            path: rel,
            kind: Kind::Place {
                src,
                mv: false,
                sha,
            },
        });
    }
    let mut kept = Vec::new();
    for added in &meta.added {
        valid_rel(&added.path)?;
        match disk_state(&loc.game, &added.path)? {
            None => {}
            Some((hash, _)) if hash.eq_ignore_ascii_case(&added.sha512) => ops.push(Op {
                path: added.path.clone(),
                kind: Kind::Remove,
            }),
            Some(_) => kept.push(added.path.clone()),
        }
    }
    kept.sort();
    let state_after = state_json_for(&meta.entry)?;
    if find_entry(&state_after, id).is_none() {
        return Err("El estado nuevo no incluye la instancia.".into());
    }
    let mut info = run(
        Txn {
            loc,
            id,
            ops,
            manifest: meta.manifest.clone(),
            entry_before: &current,
            state_after: &state_after,
            from: label(&current),
            to: label(&meta.entry),
        },
        hooks,
    )?;
    info.kept = kept;
    Ok(info)
}

// ------------------------------------------------------------------- pruebas

#[cfg(test)]
mod tests {
    use super::*;
    use crate::directory_install::PreparedDirectory;
    use serde_json::json;
    use std::{collections::BTreeMap, io::Cursor};

    fn temp_root() -> PathBuf {
        let path = std::env::temp_dir().join(format!("openclient-tx-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn ctx() -> GameContext {
        GameContext::new(PathBuf::from("."), "test", |_, _| Ok(()))
    }

    fn pack(version: &str, entries: &[(&str, &str)]) -> Modpack {
        let index = format!(
            r#"{{"formatVersion":1,"game":"minecraft","name":"Demo","versionId":"{version}","dependencies":{{"minecraft":"1.21.4"}},"files":[]}}"#
        );
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default();
        zip.start_file("modrinth.index.json", options).unwrap();
        zip.write_all(index.as_bytes()).unwrap();
        for (name, contents) in entries {
            zip.start_file(format!("overrides/{name}"), options)
                .unwrap();
            zip.write_all(contents.as_bytes()).unwrap();
        }
        Modpack::open(zip.finish().unwrap().into_inner()).unwrap()
    }

    fn state_json(id: &str, version: &str) -> String {
        json!({"instances": [{
            "id": id, "name": "Demo", "ram": 4096,
            "modpackVersion": version, "mcVersion": "1.21.4", "category": "vanilla",
            "modpack": {"source": "file", "versionNumber": version, "name": "Demo"}
        }]})
        .to_string()
    }

    struct Fixture {
        root: PathBuf,
        id: String,
        game: PathBuf,
    }

    impl Fixture {
        /// Instancia con la v1 instalada por `publish`.
        fn new(entries: &[(&str, &str)]) -> Self {
            let root = temp_root();
            let id = uuid::Uuid::new_v4();
            let runtime = tokio::runtime::Builder::new_current_thread()
                .build()
                .unwrap();
            pack("1", entries)
                .publish(
                    &root,
                    id,
                    &state_json(&id.to_string(), "1"),
                    &runtime,
                    &crate::modpack::client().unwrap(),
                )
                .unwrap();
            let root = root.canonicalize().unwrap();
            let game = root.join("instances").join(id.to_string());
            Self {
                root,
                id: id.to_string(),
                game,
            }
        }

        fn entry(&self) -> Value {
            find_entry(&self.state(), &self.id).unwrap()
        }

        fn state(&self) -> String {
            fs::read_to_string(self.root.join("launcher_state.json")).unwrap()
        }

        fn write(&self, rel: &str, contents: &str) {
            let path = self.game.join(rel);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, contents).unwrap();
        }

        fn read(&self, rel: &str) -> String {
            fs::read_to_string(self.game.join(rel)).unwrap()
        }

        /// stage + plan; devuelve el plan y el staging vivo.
        fn prepare(
            &self,
            mut next: Modpack,
            managed: &HashSet<String>,
        ) -> Result<(UpdatePlan, PreparedDirectory), String> {
            let staging =
                PreparedDirectory::new(&self.root.join("instances").join("stage-target"), false)?;
            let runtime = tokio::runtime::Builder::new_current_thread()
                .build()
                .unwrap();
            let staged = runtime.block_on(stage(
                &mut next,
                &staging.path,
                &crate::modpack::client().unwrap(),
                &ctx(),
            ))?;
            let old = read_manifest(&self.game)?.unwrap_or_default();
            let plan = plan(&self.game, &old, &next, &staged, managed)?;
            Ok((plan, staging))
        }

        fn update(
            &self,
            next: Modpack,
            version: &str,
            hooks: (Option<usize>, bool),
        ) -> Result<SnapshotInfo, String> {
            let (plan, staging) = self.prepare(next, &HashSet::new())?;
            let ctx = ctx();
            apply_inner(
                &self.root,
                &self.id,
                &plan,
                &staging.path,
                &self.entry(),
                &state_json(&self.id, version),
                &Hooks {
                    ctx: &ctx,
                    fail_after: hooks.0,
                    crash: hooks.1,
                },
            )
        }

        fn tree(&self) -> BTreeMap<String, String> {
            let mut files = Vec::new();
            walk_all(&self.game, "", &mut files);
            files
                .into_iter()
                .map(|rel| {
                    let hash = hash_file(&self.game.join(&rel)).unwrap();
                    (rel, hash)
                })
                .collect()
        }
    }

    /// Como `walk` pero incluye nombres con punto (`.openclient`).
    fn walk_all(base: &Path, rel: &str, out: &mut Vec<String>) {
        let dir = if rel.is_empty() {
            base.to_path_buf()
        } else {
            base.join(rel)
        };
        for entry in fs::read_dir(dir).unwrap() {
            let entry = entry.unwrap();
            let name = entry.file_name().into_string().unwrap();
            let child = if rel.is_empty() {
                name
            } else {
                format!("{rel}/{name}")
            };
            if entry.file_type().unwrap().is_dir() {
                walk_all(base, &child, out);
            } else {
                out.push(child);
            }
        }
    }

    const V1: &[(&str, &str)] = &[
        ("mods/old.jar", "old1"),
        ("mods/same.jar", "same"),
        ("mods/b.jar", "b1"),
        ("config/user.json", "cfg1"),
        ("saves/W/level.dat", "w1"),
    ];

    fn v2() -> Modpack {
        pack(
            "2",
            &[
                ("mods/same.jar", "same"),
                ("mods/b.jar", "b2"),
                ("mods/new.jar", "new"),
                ("config/user.json", "cfg2"),
                ("saves/W/level.dat", "w2"),
            ],
        )
    }

    fn user_edits(fx: &Fixture) {
        fx.write("config/user.json", "mine");
        fx.write("mods/b.jar", "mine-b");
        fx.write("mods/user-added.jar", "extra");
    }

    #[test]
    fn full_diff_keeps_user_files_and_never_touches_worlds() {
        let fx = Fixture::new(V1);
        user_edits(&fx);
        let before = fx.tree();
        let (plan, staging) = fx.prepare(v2(), &HashSet::new()).unwrap();
        assert_eq!(
            fx.tree(),
            before,
            "plan y stage no escriben en la instancia"
        );
        assert_eq!(plan.remove, ["mods/old.jar"]);
        assert_eq!(plan.replace, ["mods/b.jar"]);
        assert_eq!(plan.modified, ["mods/b.jar"]);
        assert_eq!(plan.add, ["mods/new.jar"]);
        assert_eq!(plan.kept, ["config/user.json", "saves/W/level.dat"]);
        assert!(plan
            .new_manifest
            .files
            .iter()
            .all(|f| { f.path != "config/user.json" && f.path != "saves/W/level.dat" }));
        assert_eq!(plan.dependencies.minecraft, "1.21.4");

        let info = apply(
            &fx.root,
            &fx.id,
            &plan,
            &staging.path,
            &fx.entry(),
            &state_json(&fx.id, "2"),
        )
        .unwrap();
        assert_eq!((info.from.as_str(), info.to.as_str()), ("1", "2"));
        assert!(!fx.game.join("mods/old.jar").exists());
        assert_eq!(fx.read("mods/b.jar"), "b2");
        assert_eq!(fx.read("mods/new.jar"), "new");
        assert_eq!(fx.read("mods/same.jar"), "same");
        assert_eq!(fx.read("mods/user-added.jar"), "extra");
        assert_eq!(fx.read("config/user.json"), "mine");
        assert_eq!(fx.read("saves/W/level.dat"), "w1");
        let snapshot = fx.root.join("snapshots").join(&fx.id).join(&info.id);
        assert_eq!(
            fs::read_to_string(snapshot.join("files/mods/b.jar")).unwrap(),
            "mine-b"
        );
        assert_eq!(
            fs::read_to_string(snapshot.join("files/mods/old.jar")).unwrap(),
            "old1"
        );
        assert_eq!(
            fs::read_to_string(snapshot.join("files/config/user.json")).unwrap(),
            "mine"
        );
        assert!(!snapshot.join("files/saves").exists());
        assert!(!fx.root.join("snapshots").join(&fx.id).join(MARKER).exists());
        let manifest = read_manifest(&fx.game).unwrap().unwrap();
        assert_eq!(manifest, plan.new_manifest);
        assert_eq!(fx.entry()["modpackVersion"], "2");
        assert_eq!(list_snapshots(&fx.root, &fx.id).unwrap().len(), 1);
    }

    #[test]
    fn injected_failure_at_every_step_leaves_disk_and_state_untouched() {
        let fx = Fixture::new(V1);
        user_edits(&fx);
        let tree = fx.tree();
        let state: Value = serde_json::from_str(&fx.state()).unwrap();
        let mut steps = 0;
        while let Err(error) = fx.update(v2(), "2", (Some(steps), false)) {
            assert!(error.contains("inyectado"), "{error}");
            assert_eq!(fx.tree(), tree, "paso {steps}");
            assert_eq!(
                serde_json::from_str::<Value>(&fx.state()).unwrap(),
                state,
                "estado en el paso {steps}"
            );
            assert!(list_snapshots(&fx.root, &fx.id).unwrap().is_empty());
            assert!(!fx.root.join("snapshots").join(&fx.id).join(MARKER).exists());
            steps += 1;
            assert!(steps < 20);
        }
        // 1 borrado + 2 colocaciones + manifiesto + estado + cierre.
        assert_eq!(steps, 6);
        assert_eq!(fx.read("mods/b.jar"), "b2");
    }

    #[test]
    fn recover_rolls_back_a_crash_in_the_middle() {
        for crash_at in [3usize, 5] {
            let fx = Fixture::new(V1);
            user_edits(&fx);
            let tree = fx.tree();
            let state: Value = serde_json::from_str(&fx.state()).unwrap();
            let error = fx.update(v2(), "2", (Some(crash_at), true)).unwrap_err();
            assert!(error.contains("inyectado"));
            assert_ne!(fx.tree(), tree, "el crash dejó el disco a medias");
            let marker = fx.root.join("snapshots").join(&fx.id).join(MARKER);
            assert!(marker.exists());
            assert!(recover(&fx.root, &fx.id).unwrap());
            assert_eq!(fx.tree(), tree);
            assert_eq!(
                serde_json::from_str::<Value>(&fx.state()).unwrap(),
                state,
                "crash en el paso {crash_at}"
            );
            assert!(!marker.exists());
            assert!(!recover(&fx.root, &fx.id).unwrap(), "idempotente");
            assert_eq!(fx.tree(), tree);
        }
    }

    #[test]
    fn recover_discards_a_marker_without_journal() {
        let fx = Fixture::new(V1);
        let snaps = fx.root.join("snapshots").join(&fx.id);
        fs::create_dir_all(snaps.join("5-aaaaaaaa")).unwrap();
        fs::write(snaps.join(MARKER), "5-aaaaaaaa").unwrap();
        let tree = fx.tree();
        assert!(recover(&fx.root, &fx.id).unwrap());
        assert!(!snaps.join("5-aaaaaaaa").exists() && !snaps.join(MARKER).exists());
        assert_eq!(fx.tree(), tree);
        assert!(!recover(&fx.root, &fx.id).unwrap());
    }

    #[test]
    fn restore_round_trip_restores_hashes_and_snapshots_the_undo() {
        let fx = Fixture::new(V1);
        user_edits(&fx);
        let tree = fx.tree();
        let first = fx.update(v2(), "2", (None, false)).unwrap();
        assert_ne!(fx.tree(), tree);
        let restored = restore(&fx.root, &fx.id, &first.id, |entry| {
            let mut state: Value = serde_json::from_str(&fx.state()).unwrap();
            let slot = &mut state["instances"][0];
            restore_entry_fields(slot, entry);
            Ok(state.to_string())
        })
        .unwrap();
        assert_eq!(fx.tree(), tree);
        assert_eq!(fx.entry()["modpackVersion"], "1");
        assert_eq!(fx.entry()["ram"], 4096);
        assert_eq!((restored.from.as_str(), restored.to.as_str()), ("2", "1"));
        assert!(restored.kept.is_empty());
        let listed = list_snapshots(&fx.root, &fx.id).unwrap();
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[0].id, restored.id);
        // Y se puede volver a la v2.
        restore(&fx.root, &fx.id, &restored.id, |entry| {
            let mut state: Value = serde_json::from_str(&fx.state()).unwrap();
            restore_entry_fields(&mut state["instances"][0], entry);
            Ok(state.to_string())
        })
        .unwrap();
        assert_eq!(fx.read("mods/b.jar"), "b2");
        assert_eq!(fx.read("mods/new.jar"), "new");
        assert_eq!(fx.read("saves/W/level.dat"), "w1");
    }

    #[test]
    fn restore_keeps_files_the_user_changed_since() {
        let fx = Fixture::new(V1);
        let first = fx.update(v2(), "2", (None, false)).unwrap();
        fx.write("mods/new.jar", "edited-after-update");
        let restored = restore(&fx.root, &fx.id, &first.id, |entry| {
            let mut state: Value = serde_json::from_str(&fx.state()).unwrap();
            restore_entry_fields(&mut state["instances"][0], entry);
            Ok(state.to_string())
        })
        .unwrap();
        assert_eq!(restored.kept, ["mods/new.jar"]);
        assert_eq!(fx.read("mods/new.jar"), "edited-after-update");
        assert_eq!(fx.read("mods/old.jar"), "old1");
    }

    #[test]
    fn retention_keeps_the_three_newest_snapshots() {
        let fx = Fixture::new(V1);
        let mut last = String::new();
        for round in 2..7 {
            let marker = format!("v{round}");
            last = fx
                .update(
                    pack(&marker, &[("mods/x.jar", marker.as_str())]),
                    &marker,
                    (None, false),
                )
                .unwrap()
                .id;
        }
        let listed = list_snapshots(&fx.root, &fx.id).unwrap();
        assert_eq!(listed.len(), 3);
        assert_eq!(listed[0].id, last);
        assert_eq!(listed[0].to, "v6");
        let dirs = fs::read_dir(fx.root.join("snapshots").join(&fx.id))
            .unwrap()
            .filter(|e| e.as_ref().unwrap().file_type().unwrap().is_dir())
            .count();
        assert_eq!(dirs, 3);
    }

    #[test]
    fn managed_content_conflict_fails_before_writing_anything() {
        let fx = Fixture::new(V1);
        let tree = fx.tree();
        let managed: HashSet<String> = ["new.jar".to_owned()].into();
        let error = fx.prepare(v2(), &managed).err().unwrap();
        assert!(
            error.contains("choca con contenido añadido: new.jar"),
            "{error}"
        );
        let error = fx
            .prepare(
                pack("3", &[("resourcepacks/OpenClient-x.zip", "z")]),
                &HashSet::new(),
            )
            .err()
            .unwrap();
        assert!(error.contains("OpenClient-x.zip"), "{error}");
        assert_eq!(fx.tree(), tree);
        assert!(!fx.root.join("snapshots").exists());
    }

    #[test]
    fn hostile_ids_are_rejected() {
        let fx = Fixture::new(V1);
        let first = fx.update(v2(), "2", (None, false)).unwrap();
        for bad in [
            "../x",
            "1-zzzzzzzz",
            "",
            "1-ABCDEF12",
            "1-abcdef1",
            "-aaaaaaaa",
            "1/../2-aaaaaaaa",
        ] {
            assert!(
                restore(&fx.root, &fx.id, bad, |_| Ok(fx.state())).is_err(),
                "{bad}"
            );
        }
        assert!(restore(&fx.root, &fx.id, "1-aaaaaaaa", |_| Ok(fx.state())).is_err());
        for bad in ["../x", "a/b", "", ".hidden"] {
            assert!(list_snapshots(&fx.root, bad).is_err(), "{bad}");
            assert!(recover(&fx.root, bad).is_err(), "{bad}");
            assert!(restore(&fx.root, bad, &first.id, |_| Ok(fx.state())).is_err());
        }
        assert_eq!(fx.read("mods/b.jar"), "b2");
    }

    #[test]
    fn backup_over_four_gib_is_refused_before_touching_disk() {
        let fx = Fixture::new(&[]);
        fx.write("mods/a.jar", "aaa");
        fx.write("mods/b.jar", "bbb");
        let sha = |text: &str| format!("{:x}", Sha512::digest(text.as_bytes()));
        let old = PackManifest {
            files: ["a", "b"]
                .iter()
                .map(|n| PackFile {
                    path: format!("mods/{n}.jar"),
                    sha512: sha(&n.repeat(3)),
                    size: 3 * 1024 * 1024 * 1024,
                    origin: crate::modpack::Origin::Download,
                })
                .collect(),
        };
        let tree = fx.tree();
        let next = pack("2", &[]);
        let error = plan(&fx.game, &old, &next, &[], &HashSet::new()).unwrap_err();
        assert_eq!(error, "La copia de seguridad superaría 4 GiB.");
        assert_eq!(fx.tree(), tree);
        // Con tamaños reales sí cabe.
        let small = PackManifest {
            files: old
                .files
                .iter()
                .map(|f| PackFile {
                    size: 3,
                    ..f.clone()
                })
                .collect(),
        };
        let plan = plan(&fx.game, &small, &next, &[], &HashSet::new()).unwrap();
        assert_eq!(plan.remove, ["mods/a.jar", "mods/b.jar"]);
    }

    #[test]
    fn apply_refuses_a_plan_that_went_stale() {
        let fx = Fixture::new(V1);
        let (plan, staging) = fx.prepare(v2(), &HashSet::new()).unwrap();
        fx.write("mods/old.jar", "user-changed-it");
        let tree = fx.tree();
        let error = apply(
            &fx.root,
            &fx.id,
            &plan,
            &staging.path,
            &fx.entry(),
            &state_json(&fx.id, "2"),
        )
        .unwrap_err();
        assert!(error.contains("cambió"), "{error}");
        assert_eq!(fx.tree(), tree);
        assert!(!fx.root.join("snapshots").join(&fx.id).exists());
    }

    #[test]
    fn cancelled_stage_leaves_the_instance_alone() {
        let fx = Fixture::new(V1);
        let tree = fx.tree();
        let token = crate::CancelToken::default();
        token.cancel();
        let cancelled = ctx().with_cancel(token);
        let staging =
            PreparedDirectory::new(&fx.root.join("instances").join("stage-target"), false).unwrap();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let error = runtime
            .block_on(stage(
                &mut v2(),
                &staging.path,
                &crate::modpack::client().unwrap(),
                &cancelled,
            ))
            .unwrap_err();
        assert!(error.starts_with("CANCELLED:"), "{error}");
        assert_eq!(fx.tree(), tree);
    }
}
