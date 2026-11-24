//! Copias de seguridad de mundos: cada `saves/<mundo>` va a un zip en
//! `snapshots/<id>/worlds/<ms>-<manual|auto>.zip`, junto a las copias del modpack, así que
//! viajan con la instancia a la papelera.
//!
//! El zip lleva `openclient-backup.json` y las entradas `saves/<mundo>/<ruta>`. Restaurar
//! valida todo antes de tocar nada, guarda antes una copia automática de lo que va a
//! reemplazar y deja el mundo anterior en su sitio si el cambio falla a medias.

use crate::{content_meta::is_link, modpack::file_path, GameContext};
use serde_json::{json, Value};
use std::{
    collections::HashSet,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

const TOTAL_LIMIT: u64 = 8 * 1024 * 1024 * 1024;
/// Tope de las copias automáticas: más grandes se omiten y se avisa.
pub const AUTO_LIMIT: u64 = 2 * 1024 * 1024 * 1024;
const FILE_LIMIT: u64 = 4 * 1024 * 1024 * 1024;
const ENTRIES_LIMIT: usize = 200_000;
const KEEP_AUTO: usize = 5;
const MANIFEST: &str = "openclient-backup.json";
const MANIFEST_LIMIT: u64 = 64 * 1024;
const LOCK: &str = "session.lock";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Manual,
    Auto,
}

impl Kind {
    fn label(self) -> &'static str {
        match self {
            Kind::Manual => "manual",
            Kind::Auto => "auto",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorldInfo {
    pub name: String,
    pub bytes: u64,
    /// Milisegundos desde la época de `level.dat` (o de la carpeta).
    pub modified: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BackupInfo {
    pub id: String,
    pub kind: Kind,
    pub created: u64,
    pub bytes: u64,
    pub worlds: Vec<String>,
}

#[derive(Debug)]
pub enum Outcome {
    Created(BackupInfo),
    NoWorlds,
    /// El total de los mundos supera el tope pedido; no se escribió nada.
    TooLarge(u64),
}

struct Dirs {
    saves: PathBuf,
    backups: PathBuf,
}

/// `None` si no existe; error si es un enlace.
fn plain(path: &Path) -> Result<Option<fs::Metadata>, String> {
    match fs::symlink_metadata(path) {
        Ok(meta) if is_link(&meta) => Err("Hay un enlace donde se esperaba una carpeta normal.".into()),
        Ok(meta) => Ok(Some(meta)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.to_string()),
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_millis() as u64)
}

fn locate(root: &Path, id: &str) -> Result<Dirs, String> {
    if !crate::instance_storage::valid_id(id) {
        return Err("Identificador de instancia inválido.".into());
    }
    let root = root.canonicalize().map_err(|error| error.to_string())?;
    let game = root.join("instances").join(id);
    if !plain(&game)?.is_some_and(|meta| meta.is_dir()) {
        return Err("La instancia no existe.".into());
    }
    Ok(Dirs {
        saves: game.join("saves"),
        backups: root.join("snapshots").join(id).join("worlds"),
    })
}

fn ensure_backups(dirs: &Dirs) -> Result<(), String> {
    fs::create_dir_all(&dirs.backups).map_err(|error| error.to_string())?;
    for dir in [dirs.backups.parent(), Some(dirs.backups.as_path())].into_iter().flatten() {
        if !plain(dir)?.is_some_and(|meta| meta.is_dir()) {
            return Err("La carpeta de copias de seguridad no es válida.".into());
        }
    }
    Ok(())
}

fn world_name_valid(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 100
        && !name.contains(['/', '\\', ':'])
        && file_path(Path::new(""), name).is_ok()
}

fn clean(text: &str) -> String {
    text.chars().filter(|c| !c.is_control()).take(60).collect()
}

struct FileEntry {
    rel: String,
    path: PathBuf,
    size: u64,
}

/// Archivos de un mundo (sin `session.lock` ni ocultos). Enlaces o rutas raras = error.
fn walk(base: &Path, rel: &str, out: &mut Vec<FileEntry>) -> Result<(), String> {
    let path = base.join(rel);
    let mut entries = fs::read_dir(&path)
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let name = entry.file_name();
        let name = name
            .to_str()
            .ok_or_else(|| format!("Nombre de archivo no copiable en «{}».", clean(rel)))?;
        if name.starts_with('.') || (rel.matches('/').count() == 0 && name == LOCK) {
            continue;
        }
        let child = format!("{rel}/{name}");
        let meta = fs::symlink_metadata(entry.path()).map_err(|error| error.to_string())?;
        if is_link(&meta) {
            return Err(format!("El mundo contiene un enlace («{}»); no se puede copiar.", clean(&child)));
        }
        if meta.is_dir() {
            walk(base, &child, out)?;
        } else if meta.is_file() {
            if out.len() >= ENTRIES_LIMIT {
                return Err("El mundo tiene demasiados archivos para copiarlo.".into());
            }
            file_path(Path::new(""), &child)
                .map_err(|_| format!("No se puede copiar la ruta «{}».", clean(&child)))?;
            out.push(FileEntry { rel: child, path: entry.path(), size: meta.len() });
        }
    }
    Ok(())
}

fn world_files(saves: &Path, world: &str) -> Result<Vec<FileEntry>, String> {
    let mut files = Vec::new();
    walk(saves, world, &mut files)?;
    Ok(files)
}

fn world_names(saves: &Path) -> Result<Vec<String>, String> {
    if plain(saves)?.is_none() {
        return Ok(Vec::new());
    }
    let mut names = Vec::new();
    for entry in fs::read_dir(saves).map_err(|error| error.to_string())?.flatten() {
        let Some(name) = entry.file_name().into_string().ok() else {
            continue;
        };
        let is_dir = fs::symlink_metadata(entry.path())
            .is_ok_and(|meta| !is_link(&meta) && meta.is_dir());
        if is_dir && !name.starts_with('.') && world_name_valid(&name) {
            names.push(name);
        }
    }
    names.sort();
    Ok(names)
}

/// Mundos de `saves/` con su tamaño. Solo lectura.
pub fn worlds(root: &Path, id: &str) -> Result<Vec<WorldInfo>, String> {
    let dirs = locate(root, id)?;
    let mut list = Vec::new();
    for name in world_names(&dirs.saves)? {
        let files = world_files(&dirs.saves, &name)?;
        let modified = fs::metadata(dirs.saves.join(&name).join("level.dat"))
            .or_else(|_| fs::metadata(dirs.saves.join(&name)))
            .ok()
            .and_then(|meta| meta.modified().ok())
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map_or(0, |elapsed| elapsed.as_millis() as u64);
        list.push(WorldInfo { bytes: files.iter().map(|file| file.size).sum(), name, modified });
    }
    Ok(list)
}

/// `^[0-9]{1,20}-(manual|auto)$`
pub fn valid_backup_id(id: &str) -> bool {
    let Some((time, kind)) = id.split_once('-') else {
        return false;
    };
    (1..=20).contains(&time.len())
        && time.bytes().all(|byte| byte.is_ascii_digit())
        && matches!(kind, "manual" | "auto")
}

fn parse_id(id: &str) -> (u64, Kind) {
    let (time, kind) = id.split_once('-').unwrap_or_default();
    (
        time.parse().unwrap_or(0),
        if kind == "auto" { Kind::Auto } else { Kind::Manual },
    )
}

fn zip_path(dirs: &Dirs, id: &str) -> PathBuf {
    dirs.backups.join(format!("{id}.zip"))
}

fn read_manifest<R: Read + std::io::Seek>(archive: &mut zip::ZipArchive<R>) -> Result<Vec<String>, String> {
    let entry = archive
        .by_name(MANIFEST)
        .map_err(|_| "La copia de seguridad no tiene índice.".to_string())?;
    if entry.size() > MANIFEST_LIMIT {
        return Err("El índice de la copia de seguridad es demasiado grande.".into());
    }
    let mut bytes = Vec::new();
    entry
        .take(MANIFEST_LIMIT)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    let value: Value = serde_json::from_slice(&bytes).map_err(|_| "El índice de la copia no es válido.")?;
    let worlds = value["worlds"]
        .as_array()
        .ok_or("El índice de la copia no es válido.")?
        .iter()
        .map(|world| world.as_str().filter(|name| world_name_valid(name)).map(str::to_owned))
        .collect::<Option<Vec<_>>>()
        .ok_or("El índice de la copia no es válido.")?;
    Ok(worlds)
}

fn info_of(dirs: &Dirs, id: &str) -> Option<BackupInfo> {
    let path = zip_path(dirs, id);
    let meta = plain(&path).ok()??;
    if !meta.is_file() {
        return None;
    }
    let worlds = fs::File::open(&path)
        .ok()
        .and_then(|file| zip::ZipArchive::new(file).ok())
        .and_then(|mut archive| read_manifest(&mut archive).ok())
        .unwrap_or_default();
    let (created, kind) = parse_id(id);
    Some(BackupInfo { id: id.to_owned(), kind, created, bytes: meta.len(), worlds })
}

fn backup_ids(dirs: &Dirs) -> Vec<String> {
    let Ok(entries) = fs::read_dir(&dirs.backups) else {
        return Vec::new();
    };
    let mut ids: Vec<String> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter_map(|name| name.strip_suffix(".zip").map(str::to_owned))
        .filter(|id| valid_backup_id(id))
        .collect();
    ids.sort_by_key(|id| std::cmp::Reverse(parse_id(id).0));
    ids
}

/// Copias existentes, la más reciente primero. Solo lectura.
pub fn backups(root: &Path, id: &str) -> Result<Vec<BackupInfo>, String> {
    let dirs = locate(root, id)?;
    Ok(backup_ids(&dirs).iter().filter_map(|name| info_of(&dirs, name)).collect())
}

/// Borra el temporal si no se llega al `rename`.
struct Cleanup(PathBuf);

impl Drop for Cleanup {
    fn drop(&mut self) {
        if self.0.is_dir() {
            let _ = fs::remove_dir_all(&self.0);
        } else {
            let _ = fs::remove_file(&self.0);
        }
    }
}

fn percent(done: u64, total: u64) -> u8 {
    (done * 100).checked_div(total).map_or(100, |value| value.min(100) as u8)
}

fn stored(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    [".mca", ".mcr", ".zip", ".jar", ".png", ".ogg", ".jpg"]
        .iter()
        .any(|suffix| lower.ends_with(suffix))
}

fn prune(dirs: &Dirs, keep: &str) {
    let autos: Vec<String> = backup_ids(dirs)
        .into_iter()
        .filter(|id| parse_id(id).1 == Kind::Auto)
        .collect();
    // La copia protegida ocupa una de las plazas.
    let slots = KEEP_AUTO - usize::from(autos.iter().any(|id| id == keep));
    for id in autos.iter().filter(|id| id.as_str() != keep).skip(slots) {
        let _ = fs::remove_file(zip_path(dirs, id));
    }
}

fn new_id(dirs: &Dirs, kind: Kind) -> String {
    let mut time = now_ms();
    loop {
        let id = format!("{time}-{}", kind.label());
        if plain(&zip_path(dirs, &id)).ok().flatten().is_none() {
            return id;
        }
        time += 1;
    }
}

/// Copia los mundos indicados (todos si `only` es `None`). `max_bytes` es el tope de este
/// llamador; el límite duro es 8 GiB. Las copias automáticas se podan a las 5 más recientes.
pub fn create(
    root: &Path,
    id: &str,
    only: Option<&[String]>,
    kind: Kind,
    max_bytes: u64,
    ctx: &GameContext,
) -> Result<Outcome, String> {
    create_inner(root, id, only, kind, max_bytes, "", ctx)
}

fn create_inner(
    root: &Path,
    id: &str,
    only: Option<&[String]>,
    kind: Kind,
    max_bytes: u64,
    protect: &str,
    ctx: &GameContext,
) -> Result<Outcome, String> {
    ctx.cancel.check()?;
    let dirs = locate(root, id)?;
    let existing = world_names(&dirs.saves)?;
    let selected: Vec<String> = match only {
        None => existing,
        Some(names) => {
            let mut chosen = Vec::new();
            for name in names {
                if !existing.contains(name) {
                    return Err("Ese mundo ya no existe.".into());
                }
                if !chosen.contains(name) {
                    chosen.push(name.clone());
                }
            }
            chosen
        }
    };
    if selected.is_empty() {
        return Ok(Outcome::NoWorlds);
    }
    let mut files = Vec::new();
    for name in &selected {
        walk(&dirs.saves, name, &mut files)?;
    }
    let total: u64 = files.iter().map(|file| file.size).sum();
    if total > TOTAL_LIMIT {
        return Err("Los mundos superan el límite de 8 GiB para una copia de seguridad.".into());
    }
    if total > max_bytes {
        return Ok(Outcome::TooLarge(total));
    }
    ensure_backups(&dirs)?;
    let name = new_id(&dirs, kind);
    let part = dirs.backups.join(format!(".{name}.{}.part", uuid::Uuid::new_v4()));
    let output = fs::File::create_new(&part).map_err(|error| error.to_string())?;
    let _cleanup = Cleanup(part.clone());
    let mut zip = zip::ZipWriter::new(output);
    let plain_options = zip::write::SimpleFileOptions::default();
    zip.start_file(MANIFEST, plain_options).map_err(|error| error.to_string())?;
    zip.write_all(json!({"format": 1, "worlds": selected}).to_string().as_bytes())
        .map_err(|error| error.to_string())?;
    let mut done = 0u64;
    let mut buffer = vec![0u8; 256 * 1024];
    for (position, file) in files.iter().enumerate() {
        ctx.cancel.check()?;
        let method = if stored(&file.rel) {
            zip::CompressionMethod::Stored
        } else {
            zip::CompressionMethod::Deflated
        };
        let options = plain_options
            .compression_method(method)
            .large_file(file.size >= 0x7FFF_FFFF);
        zip.start_file(format!("saves/{}", file.rel), options)
            .map_err(|error| error.to_string())?;
        let mut input = fs::File::open(&file.path)
            .map_err(|error| format!("No se pudo leer «{}»: {error}", clean(&file.rel)))?
            .take(FILE_LIMIT + 1);
        let mut copied = 0u64;
        loop {
            ctx.cancel.check()?;
            let read = input.read(&mut buffer).map_err(|error| error.to_string())?;
            if read == 0 {
                break;
            }
            zip.write_all(&buffer[..read]).map_err(|error| error.to_string())?;
            copied += read as u64;
        }
        if copied > FILE_LIMIT {
            return Err(format!("«{}» es demasiado grande para copiarlo.", clean(&file.rel)));
        }
        done += file.size;
        if position % 50 == 0 || position + 1 == files.len() {
            let percent = percent(done, total);
            ctx.progress(
                "backup",
                percent,
                &format!("Copiando mundos ({}/{})", position + 1, files.len()),
            );
        }
    }
    ctx.cancel.check()?;
    let output = zip.finish().map_err(|error| error.to_string())?;
    output.sync_all().map_err(|error| error.to_string())?;
    drop(output);
    fs::rename(&part, zip_path(&dirs, &name))
        .map_err(|error| format!("No se pudo guardar la copia de seguridad: {error}"))?;
    prune(&dirs, protect);
    info_of(&dirs, &name)
        .map(Outcome::Created)
        .ok_or_else(|| "No se pudo leer la copia recién creada.".into())
}

/// Entradas del zip ya validadas: `(índice, mundo, ruta relativa al mundo, tamaño)`.
struct Plan {
    worlds: Vec<String>,
    files: Vec<(usize, String, String, u64)>,
}

fn plan_restore(archive: &mut zip::ZipArchive<fs::File>) -> Result<Plan, String> {
    let invalid = || "La copia de seguridad no es válida.".to_string();
    let worlds = read_manifest(archive)?;
    if archive.len() > ENTRIES_LIMIT + 1 {
        return Err("La copia de seguridad tiene demasiados archivos.".into());
    }
    let mut files = Vec::new();
    let mut seen = HashSet::new();
    let mut total = 0u64;
    for index in 0..archive.len() {
        let entry = archive.by_index(index).map_err(|_| invalid())?;
        let name = entry.name().map_err(|_| invalid())?.into_owned();
        if name == MANIFEST || entry.is_dir() {
            continue;
        }
        if entry.unix_mode().is_some_and(|mode| mode & 0o170000 == 0o120000) {
            return Err(invalid());
        }
        let rest = name.strip_prefix("saves/").ok_or_else(invalid)?;
        let (world, inner) = rest.split_once('/').ok_or_else(invalid)?;
        if !worlds.iter().any(|known| known == world) {
            return Err(invalid());
        }
        file_path(Path::new(""), rest).map_err(|_| invalid())?;
        if !seen.insert(rest.to_lowercase()) {
            return Err(invalid());
        }
        total += entry.size();
        if total > TOTAL_LIMIT {
            return Err("La copia de seguridad supera el límite de 8 GiB.".into());
        }
        files.push((index, world.to_owned(), inner.to_owned(), entry.size()));
    }
    Ok(Plan { worlds, files })
}

fn extract(
    archive: &mut zip::ZipArchive<fs::File>,
    plan: &Plan,
    stage: &Path,
    ctx: &GameContext,
) -> Result<(), String> {
    for world in &plan.worlds {
        fs::create_dir_all(stage.join(world)).map_err(|error| error.to_string())?;
    }
    let total: u64 = plan.files.iter().map(|file| file.3).sum();
    let mut done = 0u64;
    let mut buffer = vec![0u8; 256 * 1024];
    for (position, (index, world, inner, size)) in plan.files.iter().enumerate() {
        ctx.cancel.check()?;
        let destination = file_path(&stage.join(world), inner)?;
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let entry = archive.by_index(*index).map_err(|error| error.to_string())?;
        let mut input = entry.take(size + 1);
        let mut output = fs::File::create_new(&destination).map_err(|error| error.to_string())?;
        let mut copied = 0u64;
        loop {
            ctx.cancel.check()?;
            let read = input
                .read(&mut buffer)
                .map_err(|_| "La copia de seguridad está dañada.".to_string())?;
            if read == 0 {
                break;
            }
            output.write_all(&buffer[..read]).map_err(|error| error.to_string())?;
            copied += read as u64;
        }
        if copied != *size {
            return Err("La copia de seguridad está dañada.".into());
        }
        done += size;
        if position % 50 == 0 || position + 1 == plan.files.len() {
            let percent = percent(done, total);
            ctx.progress("restore", percent, &format!("Extrayendo ({}/{})", position + 1, plan.files.len()));
        }
    }
    Ok(())
}

/// Resultado de `restore`: la copia usada y la que guarda lo que había antes.
#[derive(Debug)]
pub struct Restored {
    pub restored: BackupInfo,
    pub previous: Option<BackupInfo>,
}

/// Reemplaza los mundos de la copia. Antes guarda el estado actual de esos mundos como copia
/// automática. Si el reemplazo falla a medias, vuelve a dejar los mundos anteriores.
pub fn restore(
    root: &Path,
    id: &str,
    backup: &str,
    ctx: &GameContext,
) -> Result<Restored, String> {
    if !valid_backup_id(backup) {
        return Err("Identificador de copia de seguridad inválido.".into());
    }
    let dirs = locate(root, id)?;
    let path = zip_path(&dirs, backup);
    if !plain(&path)?.is_some_and(|meta| meta.is_file()) {
        return Err("La copia de seguridad ya no existe.".into());
    }
    let mut archive = zip::ZipArchive::new(fs::File::open(&path).map_err(|error| error.to_string())?)
        .map_err(|_| "La copia de seguridad no es válida.".to_string())?;
    let plan = plan_restore(&mut archive)?;
    if plan.worlds.is_empty() {
        return Err("La copia de seguridad no contiene mundos.".into());
    }
    ensure_backups(&dirs)?;
    let stage = dirs.backups.join(format!(".restore-{}", uuid::Uuid::new_v4()));
    fs::create_dir(&stage).map_err(|error| error.to_string())?;
    let _stage_cleanup = Cleanup(stage.clone());
    extract(&mut archive, &plan, &stage, ctx)?;

    ctx.cancel.check()?;
    let present: Vec<String> = world_names(&dirs.saves)?
        .into_iter()
        .filter(|name| plan.worlds.contains(name))
        .collect();
    let previous = if present.is_empty() {
        None
    } else {
        match create_inner(root, id, Some(&present), Kind::Auto, TOTAL_LIMIT, backup, ctx)? {
            Outcome::Created(info) => Some(info),
            _ => return Err("No se pudo guardar el estado actual antes de restaurar.".into()),
        }
    };

    ctx.cancel.check()?;
    // Desde aquí cambian los mundos del disco: ya no se puede cancelar.
    ctx.progress("applying", 100, "Reemplazando los mundos");
    fs::create_dir_all(&dirs.saves).map_err(|error| error.to_string())?;
    let old = dirs.backups.join(format!(".old-{}", uuid::Uuid::new_v4()));
    fs::create_dir(&old).map_err(|error| error.to_string())?;
    let _old_cleanup = Cleanup(old.clone());
    let mut swapped: Vec<(&String, bool)> = Vec::new();
    for world in &plan.worlds {
        let target = dirs.saves.join(world);
        let had = plain(&target)?.is_some();
        let result = (|| -> std::io::Result<()> {
            if had {
                fs::rename(&target, old.join(world))?;
            }
            fs::rename(stage.join(world), &target).inspect_err(|_| {
                if had {
                    let _ = fs::rename(old.join(world), &target);
                }
            })
        })()
        .map_err(|error| format!("No se pudo reemplazar el mundo «{}»; cierra Minecraft ({error}).", clean(world)));
        if let Err(error) = result {
            for (done, had) in swapped.iter().rev() {
                let target = dirs.saves.join(done);
                let _ = fs::rename(&target, stage.join(done));
                if *had {
                    let _ = fs::rename(old.join(done), &target);
                }
            }
            return Err(error);
        }
        swapped.push((world, had));
    }
    let restored = info_of(&dirs, backup).ok_or("La copia de seguridad ya no existe.")?;
    Ok(Restored { restored, previous })
}

/// Borra una copia de seguridad.
pub fn delete(root: &Path, id: &str, backup: &str) -> Result<(), String> {
    if !valid_backup_id(backup) {
        return Err("Identificador de copia de seguridad inválido.".into());
    }
    let dirs = locate(root, id)?;
    let path = zip_path(&dirs, backup);
    if !plain(&path)?.is_some_and(|meta| meta.is_file()) {
        return Err("La copia de seguridad ya no existe.".into());
    }
    fs::remove_file(path).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CancelToken;

    fn ctx() -> GameContext {
        GameContext::new(PathBuf::from("."), "test", |_, _| Ok(()))
    }

    fn library() -> PathBuf {
        let root = std::env::temp_dir().join(format!("openclient-wb-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(root.join("instances/demo")).unwrap();
        root
    }

    fn put(root: &Path, relative: &str, bytes: &[u8]) {
        let path = root.join("instances/demo/saves").join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }

    fn read(root: &Path, relative: &str) -> Vec<u8> {
        fs::read(root.join("instances/demo/saves").join(relative)).unwrap()
    }

    fn manual(root: &Path, only: Option<&[String]>) -> BackupInfo {
        match create(root, "demo", only, Kind::Manual, TOTAL_LIMIT, &ctx()).unwrap() {
            Outcome::Created(info) => info,
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn roundtrip_replaces_the_world_and_keeps_the_previous_state() {
        let root = library();
        put(&root, "Mundo/level.dat", b"v1");
        put(&root, "Mundo/region/r.0.0.mca", b"region1");
        put(&root, "Mundo/session.lock", b"lock");
        put(&root, "Otro/level.dat", b"otro");
        let info = manual(&root, Some(&["Mundo".to_string()]));
        assert_eq!(info.worlds, ["Mundo"]);
        assert_eq!(info.kind, Kind::Manual);
        assert!(valid_backup_id(&info.id));

        put(&root, "Mundo/level.dat", b"v2");
        put(&root, "Mundo/nuevo.txt", b"extra");
        let done = restore(&root, "demo", &info.id, &ctx()).unwrap();
        assert_eq!(read(&root, "Mundo/level.dat"), b"v1");
        assert_eq!(read(&root, "Mundo/region/r.0.0.mca"), b"region1");
        assert!(!root.join("instances/demo/saves/Mundo/nuevo.txt").exists());
        assert!(!root.join("instances/demo/saves/Mundo/session.lock").exists());
        assert_eq!(read(&root, "Otro/level.dat"), b"otro");
        let previous = done.previous.unwrap();
        assert_eq!(previous.kind, Kind::Auto);

        // Volver atrás con la copia automática que dejó restore.
        restore(&root, "demo", &previous.id, &ctx()).unwrap();
        assert_eq!(read(&root, "Mundo/level.dat"), b"v2");
        assert_eq!(read(&root, "Mundo/nuevo.txt"), b"extra");

        let listed = backups(&root, "demo").unwrap();
        assert!(listed.len() >= 2);
        assert!(listed.windows(2).all(|pair| pair[0].created >= pair[1].created));
        let leftovers = fs::read_dir(root.join("snapshots/demo/worlds"))
            .unwrap()
            .flatten()
            .filter(|entry| entry.file_name().to_string_lossy().starts_with('.'))
            .count();
        assert_eq!(leftovers, 0, "sin temporales");
    }

    #[test]
    fn restore_recreates_a_deleted_world() {
        let root = library();
        put(&root, "Mundo/level.dat", b"v1");
        let info = manual(&root, None);
        fs::remove_dir_all(root.join("instances/demo/saves/Mundo")).unwrap();
        let done = restore(&root, "demo", &info.id, &ctx()).unwrap();
        assert!(done.previous.is_none());
        assert_eq!(read(&root, "Mundo/level.dat"), b"v1");
    }

    #[test]
    fn worlds_lists_sizes_and_skips_hidden_and_files() {
        let root = library();
        put(&root, "A/level.dat", b"1234");
        put(&root, "A/data/x.dat", b"12");
        put(&root, ".oculto/level.dat", b"x");
        put(&root, "suelto.txt", b"x");
        let list = worlds(&root, "demo").unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!((list[0].name.as_str(), list[0].bytes), ("A", 6));
        assert!(create(&root, "demo", Some(&["Falta".into()]), Kind::Manual, TOTAL_LIMIT, &ctx()).is_err());
        let empty = library();
        assert!(matches!(
            create(&empty, "demo", None, Kind::Manual, TOTAL_LIMIT, &ctx()).unwrap(),
            Outcome::NoWorlds
        ));
        assert!(backups(&empty, "demo").unwrap().is_empty());
    }

    #[test]
    fn auto_copies_over_the_cap_are_skipped_and_old_autos_pruned() {
        let root = library();
        put(&root, "M/level.dat", b"datos");
        assert!(matches!(
            create(&root, "demo", None, Kind::Auto, 3, &ctx()).unwrap(),
            Outcome::TooLarge(5)
        ));
        assert!(backups(&root, "demo").unwrap().is_empty());
        let keep = manual(&root, None);
        for _ in 0..8 {
            create(&root, "demo", None, Kind::Auto, AUTO_LIMIT, &ctx()).unwrap();
        }
        let listed = backups(&root, "demo").unwrap();
        assert_eq!(listed.iter().filter(|b| b.kind == Kind::Auto).count(), KEEP_AUTO);
        assert!(listed.iter().any(|b| b.id == keep.id), "las manuales no se podan");
    }

    #[test]
    fn delete_and_ids_are_validated() {
        let root = library();
        put(&root, "M/level.dat", b"d");
        let info = manual(&root, None);
        for bad in ["../x", "1-otro", "x-manual", "", "1-manual.zip"] {
            assert!(delete(&root, "demo", bad).is_err(), "{bad}");
            assert!(restore(&root, "demo", bad, &ctx()).is_err(), "{bad}");
        }
        assert!(restore(&root, "../x", &info.id, &ctx()).is_err());
        delete(&root, "demo", &info.id).unwrap();
        assert!(delete(&root, "demo", &info.id).is_err());
        assert!(backups(&root, "demo").unwrap().is_empty());
    }

    fn forged(root: &Path, entries: &[(&str, &[u8])], worlds: &[&str]) -> String {
        let dir = root.join("snapshots/demo/worlds");
        fs::create_dir_all(&dir).unwrap();
        let id = "5-manual".to_string();
        let mut zip = zip::ZipWriter::new(fs::File::create(dir.join(format!("{id}.zip"))).unwrap());
        let options = zip::write::SimpleFileOptions::default();
        zip.start_file(MANIFEST, options).unwrap();
        zip.write_all(json!({"format": 1, "worlds": worlds}).to_string().as_bytes()).unwrap();
        for (name, data) in entries {
            zip.start_file(*name, options).unwrap();
            zip.write_all(data).unwrap();
        }
        zip.finish().unwrap();
        id
    }

    #[test]
    fn forged_archives_are_rejected_without_touching_the_world() {
        let root = library();
        put(&root, "M/level.dat", b"original");
        for entries in [
            vec![("saves/M/../../../fuera.txt", b"x".as_slice())],
            vec![("saves/M/a.txt", b"x".as_slice()), ("saves/M/A.TXT", b"y".as_slice())],
            vec![("saves/Otro/a.txt", b"x".as_slice())],
            vec![("fuera/a.txt", b"x".as_slice())],
            vec![("saves/M/.oculto", b"x".as_slice())],
            vec![("saves/M/CON", b"x".as_slice())],
        ] {
            let id = forged(&root, &entries, &["M"]);
            assert!(restore(&root, "demo", &id, &ctx()).is_err(), "{entries:?}");
            assert_eq!(read(&root, "M/level.dat"), b"original");
            assert!(!root.join("instances/demo/fuera.txt").exists());
        }
        let id = forged(&root, &[("saves/M/a.txt", b"x".as_slice())], &["../M"]);
        assert!(restore(&root, "demo", &id, &ctx()).is_err());
        assert_eq!(read(&root, "M/level.dat"), b"original");
        // Un índice válido con un archivo normal sí se restaura.
        let id = forged(&root, &[("saves/M/a.txt", b"x".as_slice())], &["M"]);
        restore(&root, "demo", &id, &ctx()).unwrap();
        assert_eq!(read(&root, "M/a.txt"), b"x");
        assert!(!root.join("instances/demo/saves/M/level.dat").exists());
    }

    #[test]
    fn cancelled_backup_leaves_no_files() {
        let root = library();
        put(&root, "M/level.dat", b"d");
        let token = CancelToken::default();
        token.cancel();
        let cancelled = ctx().with_cancel(token);
        let error = create(&root, "demo", None, Kind::Manual, TOTAL_LIMIT, &cancelled).unwrap_err();
        assert!(error.starts_with("CANCELLED:"));
        assert!(!root.join("snapshots").exists());

        let info = manual(&root, None);
        let token = CancelToken::default();
        token.cancel();
        let cancelled = ctx().with_cancel(token);
        assert!(restore(&root, "demo", &info.id, &cancelled).is_err());
        let hidden = fs::read_dir(root.join("snapshots/demo/worlds"))
            .unwrap()
            .flatten()
            .filter(|entry| entry.file_name().to_string_lossy().starts_with('.'))
            .count();
        assert_eq!(hidden, 0);
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_in_worlds_are_rejected() {
        let root = library();
        put(&root, "M/level.dat", b"d");
        let outside = library();
        std::os::unix::fs::symlink(&outside, root.join("instances/demo/saves/M/enlace")).unwrap();
        assert!(create(&root, "demo", None, Kind::Manual, TOTAL_LIMIT, &ctx()).is_err());
    }
}
