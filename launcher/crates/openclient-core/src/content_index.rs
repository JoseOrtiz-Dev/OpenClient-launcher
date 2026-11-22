//! Índice persistente de metadatos de contenido: `instances/<id>/.openclient_content_index.json`.
//!
//! Vive en la raíz de la instancia (no dentro de `mods/`) para que la sincronización, que copia y
//! reemplaza esa carpeta, no lo toque. La lectura es tolerante (cualquier problema devuelve un
//! índice vacío que se reconstruye) y la escritura es atómica y serializada.

use crate::content_meta::{cdn_icon, clean_text, hex_valid, is_link, valid_id};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{Mutex, PoisonError},
};

pub const FILE: &str = ".openclient_content_index.json";
pub const CATEGORIES: [&str; 3] = ["mods", "resourcepacks", "shaderpacks"];
pub const MAX_ENTRIES: usize = 5000;
pub const MAX_BYTES: u64 = 4 * 1024 * 1024;
/// Un `notFound` caduca a los 7 días (segundos).
pub const NOT_FOUND_TTL: u64 = 7 * 24 * 60 * 60;
const VERSION: u64 = 1;
const ICON_LIMIT: usize = 256 * 1024;

static LOCK: Mutex<()> = Mutex::new(());

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Entry {
    pub size: u64,
    /// Milisegundos UNIX.
    pub mtime: u64,
    pub sha1: String,
    pub sha512: String,
    pub project_id: String,
    pub version_id: String,
    pub title: String,
    pub author: String,
    pub version: String,
    /// `""`, URL del CDN de Modrinth o `cache:<sha1>`; nunca una ruta.
    pub icon: String,
    pub description: String,
    /// `modrinth`, `jar` o `file`.
    pub origin: String,
    /// Segundos UNIX de la última consulta a Modrinth (0 = aún sin consultar).
    pub resolved_at: u64,
    pub not_found: bool,
    pub checked_at: u64,
    pub update: Value,
}

impl Entry {
    pub fn fresh(&self, size: u64, mtime: u64) -> bool {
        self.size == size && self.mtime == mtime
    }

    /// ¿Falta consultar a Modrinth? Un `notFound` solo se repite al caducar su TTL.
    pub fn needs_network(&self, now: u64) -> bool {
        if self.not_found {
            now.saturating_sub(self.resolved_at) > NOT_FOUND_TTL
        } else {
            self.resolved_at == 0
        }
    }

    /// Descarta lo que no cumpla el formato: el archivo es editable por cualquiera.
    fn sanitized(mut self) -> Option<Self> {
        let hashes = (self.sha1.is_empty() && self.sha512.is_empty())
            || (hex_valid(&self.sha1, 40) && hex_valid(&self.sha512, 128));
        let ids = [&self.project_id, &self.version_id].iter().all(|id| id.is_empty() || valid_id(id));
        let icon = self.icon.is_empty()
            || cdn_icon(&self.icon).is_some()
            || self.icon.strip_prefix("cache:").is_some_and(|sha1| hex_valid(sha1, 40));
        if !hashes || !ids || !icon || !matches!(self.origin.as_str(), "modrinth" | "jar" | "file") {
            return None;
        }
        for text in [&mut self.title, &mut self.author, &mut self.version, &mut self.description] {
            *text = clean_text(text);
        }
        if !self.update.is_object() {
            self.update = Value::Null;
        }
        Some(self)
    }
}

#[derive(Debug, Default)]
pub struct Index {
    /// Segundos UNIX del último fallo de red (0 = sin fallo).
    pub offline_at: u64,
    pub entries: BTreeMap<String, Entry>,
}

/// Nombre de archivo de una sola pieza: sin separadores, `:`, `..`, punto inicial ni controles.
pub fn valid_file_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 255
        && !name.starts_with('.')
        && !name.contains("..")
        && !name.contains(['/', '\\', ':'])
        && !name.chars().any(char::is_control)
}

pub fn key(category: &str, name: &str) -> String {
    format!("{category}/{name}")
}

pub fn valid_key(key: &str) -> bool {
    key.split_once('/')
        .is_some_and(|(category, name)| CATEGORIES.contains(&category) && valid_file_name(name))
}

/// ¿Existe el archivo de la clave, activado o como `.disabled`? (`symlink_metadata`: no sigue enlaces.)
pub fn on_disk(game: &Path, key: &str) -> bool {
    let path = game.join(key);
    let mut disabled = path.clone().into_os_string();
    disabled.push(".disabled");
    fs::symlink_metadata(path).is_ok() || fs::symlink_metadata(disabled).is_ok()
}

fn read(game: &Path) -> Option<Index> {
    let path = game.join(FILE);
    let metadata = fs::symlink_metadata(&path).ok()?;
    if is_link(&metadata) || !metadata.is_file() || metadata.len() > MAX_BYTES {
        return None;
    }
    let mut bytes = Vec::new();
    fs::File::open(path).ok()?.take(MAX_BYTES + 1).read_to_end(&mut bytes).ok()?;
    let raw: Value = serde_json::from_slice(&bytes).ok().filter(|_| bytes.len() as u64 <= MAX_BYTES)?;
    if raw["v"].as_u64() != Some(VERSION) {
        return None;
    }
    let entries = raw["entries"]
        .as_object()?
        .iter()
        .filter(|(key, _)| valid_key(key))
        .filter_map(|(key, value)| {
            let entry = serde_json::from_value::<Entry>(value.clone()).ok()?.sanitized()?;
            Some((key.clone(), entry))
        })
        .take(MAX_ENTRIES)
        .collect();
    Some(Index { offline_at: raw["offlineAt"].as_u64().unwrap_or(0), entries })
}

/// Carga el índice; ausente, enorme, enlace, JSON roto o `v` distinta equivalen a vacío.
pub fn load(game: &Path) -> Index {
    read(game).unwrap_or_default()
}

fn drop_oldest(index: &mut Index, count: usize) {
    let mut order: Vec<(u64, String)> =
        index.entries.iter().map(|(key, entry)| (entry.resolved_at, key.clone())).collect();
    order.sort();
    for (_, key) in order.into_iter().take(count) {
        index.entries.remove(&key);
    }
}

/// Serializa respetando los límites: si no cabe, descarta primero lo más antiguo.
fn encode(index: &mut Index) -> Result<Vec<u8>, String> {
    if index.entries.len() > MAX_ENTRIES {
        drop_oldest(index, index.entries.len() - MAX_ENTRIES);
    }
    loop {
        let bytes = serde_json::to_vec(&json!({"v": VERSION, "offlineAt": index.offline_at, "entries": index.entries}))
            .map_err(|error| error.to_string())?;
        if bytes.len() as u64 <= MAX_BYTES {
            return Ok(bytes);
        }
        if index.entries.is_empty() {
            return Err("El índice de contenido es demasiado grande.".into());
        }
        drop_oldest(index, (index.entries.len() / 10).max(1));
    }
}

/// Temporal hermano + `sync_all` + `rename`; el temporal se borra si algo falla.
fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut name = path.file_name().map(|name| name.to_os_string()).unwrap_or_default();
    name.push(format!(".tmp-{}", uuid::Uuid::new_v4()));
    let temp = path.with_file_name(name);
    let result = (|| -> std::io::Result<()> {
        let mut output = fs::File::create_new(&temp)?;
        output.write_all(bytes)?;
        output.sync_all()?;
        drop(output);
        fs::rename(&temp, path)
    })();
    result.map_err(|error| {
        let _ = fs::remove_file(&temp);
        format!("No se pudo guardar el índice de contenido: {error}")
    })
}

fn update_with(game: &Path, keep: &dyn Fn(&str) -> bool, change: impl FnOnce(&mut Index)) -> Result<(), String> {
    let _lock = LOCK.lock().unwrap_or_else(PoisonError::into_inner);
    // Se relee dentro del cerrojo: otra llamada pudo escribir desde la última lectura.
    let mut index = load(game);
    change(&mut index);
    index.entries.retain(|key, _| valid_key(key) && keep(key));
    let bytes = encode(&mut index)?;
    write_atomic(&game.join(FILE), &bytes)
}

/// Lee, aplica `change`, poda (archivos que ya no existen, límites) y escribe, todo bajo cerrojo.
pub fn update(game: &Path, change: impl FnOnce(&mut Index)) -> Result<(), String> {
    update_with(game, &|key| on_disk(game, key), change)
}

pub fn icon_path(root: &Path, sha1: &str) -> Option<PathBuf> {
    hex_valid(sha1, 40)
        .then(|| root.join("cache").join("content-icons").join(format!("{}.png", sha1.to_ascii_lowercase())))
}

/// Guarda un PNG ya validado en `<root>/cache/content-icons/<sha1>.png` (sin sobrescribir).
pub fn save_icon(root: &Path, sha1: &str, png: &[u8]) -> Result<(), String> {
    let invalid = || "No se pudo guardar el icono.".to_string();
    let path = icon_path(root, sha1).ok_or_else(invalid)?;
    if png.len() > ICON_LIMIT {
        return Err(invalid());
    }
    let folder = path.parent().ok_or_else(invalid)?;
    for directory in [root.join("cache"), folder.to_path_buf()] {
        match fs::symlink_metadata(&directory) {
            Ok(metadata) if is_link(&metadata) || !metadata.is_dir() => return Err(invalid()),
            Ok(_) => {}
            Err(_) => fs::create_dir(&directory).map_err(|_| invalid())?,
        }
    }
    match fs::symlink_metadata(&path) {
        Ok(metadata) if !is_link(&metadata) && metadata.is_file() => Ok(()),
        Ok(_) => Err(invalid()),
        Err(_) => write_atomic(&path, png),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("openclient-index-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn entry(size: u64, mtime: u64) -> Entry {
        Entry { size, mtime, origin: "file".into(), ..Entry::default() }
    }

    fn put(game: &Path, relative: &str) {
        let path = game.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, b"x").unwrap();
    }

    #[test]
    fn unreadable_indexes_load_as_empty_without_errors() {
        let game = temp_dir();
        assert!(load(&game).entries.is_empty());
        let good = json!({"v": 1, "offlineAt": 5, "entries": {"mods/a.jar": entry(1, 2)}});
        for (text, empty) in [
            ("{ no es json".to_owned(), true),
            (json!({"v": 2, "entries": {}}).to_string(), true),
            (json!({"entries": {}}).to_string(), true),
            (json!({"v": 1, "entries": []}).to_string(), true),
            (good.to_string(), false),
        ] {
            fs::write(game.join(FILE), text).unwrap();
            assert_eq!(load(&game).entries.is_empty(), empty);
        }
        assert_eq!(load(&game).offline_at, 5);
        let big = fs::File::create(game.join(FILE)).unwrap();
        big.set_len(MAX_BYTES + 1).unwrap();
        drop(big);
        assert!(load(&game).entries.is_empty());
        // Un directorio con ese nombre tampoco es un índice.
        fs::remove_file(game.join(FILE)).unwrap();
        fs::create_dir(game.join(FILE)).unwrap();
        assert!(load(&game).entries.is_empty());
        fs::remove_dir_all(game).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn a_symlinked_index_is_ignored() {
        let game = temp_dir();
        let outside = temp_dir().join("fuera.json");
        fs::write(&outside, json!({"v": 1, "entries": {"mods/a.jar": entry(1, 2)}}).to_string()).unwrap();
        std::os::unix::fs::symlink(&outside, game.join(FILE)).unwrap();
        assert!(load(&game).entries.is_empty());
    }

    #[test]
    fn hostile_keys_and_fields_are_dropped_on_load() {
        let game = temp_dir();
        let sha1 = "a".repeat(40);
        let mut good = entry(1, 2);
        good.sha1 = sha1.clone();
        good.sha512 = "b".repeat(128);
        good.title = "Titulo\nroto".into();
        good.icon = format!("cache:{sha1}");
        let bad_icon = Entry { icon: "C:\\Windows\\x.png".into(), ..entry(1, 2) };
        let bad_origin = Entry { origin: "otro".into(), ..entry(1, 2) };
        let bad_hash = Entry { sha1: "zz".into(), ..entry(1, 2) };
        let bad_project = Entry { project_id: "../x".into(), ..entry(1, 2) };
        let cdn_other = Entry { icon: "https://evil.example/data/x.png".into(), ..entry(1, 2) };
        let entries = json!({
            "mods/bueno.jar": good, "mods/../x.jar": entry(1, 2), "../x.jar": entry(1, 2), "mods/a/b.jar": entry(1, 2),
            "mods/C:x.jar": entry(1, 2), "mods/.oculto.jar": entry(1, 2), "otra/x.jar": entry(1, 2),
            "mods/icono.jar": bad_icon, "mods/origen.jar": bad_origin, "mods/hash.jar": bad_hash,
            "mods/proyecto.jar": bad_project, "mods/cdn.jar": cdn_other, "mods/roto.jar": "texto",
        });
        fs::write(game.join(FILE), json!({"v": 1, "entries": entries}).to_string()).unwrap();
        let index = load(&game);
        assert_eq!(index.entries.keys().collect::<Vec<_>>(), ["mods/bueno.jar"]);
        assert_eq!(index.entries["mods/bueno.jar"].title, "Tituloroto");
        fs::remove_dir_all(game).unwrap();
    }

    #[test]
    fn entries_are_valid_only_for_the_same_size_and_mtime() {
        let stored = entry(10, 1000);
        assert!(stored.fresh(10, 1000));
        assert!(!stored.fresh(11, 1000));
        assert!(!stored.fresh(10, 1001));
    }

    #[test]
    fn not_found_expires_after_seven_days_with_an_injected_clock() {
        let mut stored = entry(1, 1);
        assert!(stored.needs_network(100), "sin consultar");
        stored.resolved_at = 100;
        assert!(!stored.needs_network(100_000));
        stored.not_found = true;
        assert!(!stored.needs_network(100 + NOT_FOUND_TTL));
        assert!(stored.needs_network(101 + NOT_FOUND_TTL));
    }

    #[test]
    fn update_merges_prunes_and_leaves_no_temporaries() {
        let game = temp_dir();
        put(&game, "mods/vivo.jar");
        put(&game, "mods/apagado.jar.disabled");
        put(&game, "resourcepacks/pack.zip");
        update(&game, |index| {
            index.offline_at = 9;
            for name in ["mods/vivo.jar", "mods/apagado.jar", "mods/muerto.jar", "resourcepacks/pack.zip"] {
                index.entries.insert(name.into(), entry(1, 1));
            }
        })
        .unwrap();
        // Otra llamada escribe después: se fusiona con lo ya guardado, no lo pisa.
        update(&game, |index| {
            index.entries.insert("mods/vivo.jar".into(), Entry { title: "Nuevo".into(), ..entry(1, 1) });
        })
        .unwrap();
        let index = load(&game);
        assert_eq!(
            index.entries.keys().collect::<Vec<_>>(),
            ["mods/apagado.jar", "mods/vivo.jar", "resourcepacks/pack.zip"]
        );
        assert_eq!((index.offline_at, index.entries["mods/vivo.jar"].title.as_str()), (9, "Nuevo"));
        let names: Vec<String> = fs::read_dir(&game).unwrap().map(|e| e.unwrap().file_name().into_string().unwrap()).collect();
        assert!(names.iter().all(|name| !name.contains(".tmp-")), "{names:?}");
        fs::remove_dir_all(game).unwrap();
    }

    #[test]
    fn size_limits_drop_the_oldest_entries_first() {
        let game = temp_dir();
        update_with(&game, &|_| true, |index| {
            for number in 0..MAX_ENTRIES + 10 {
                index.entries.insert(format!("mods/m{number:05}.jar"), Entry { resolved_at: number as u64 + 1, ..entry(1, 1) });
            }
        })
        .unwrap();
        let index = load(&game);
        assert_eq!(index.entries.len(), MAX_ENTRIES);
        assert!(!index.entries.contains_key("mods/m00009.jar") && index.entries.contains_key("mods/m00010.jar"));
        // Por tamaño en bytes: textos largos hacen que no quepan 5000 entradas.
        update_with(&game, &|_| true, |index| {
            for (number, entry) in index.entries.values_mut().enumerate() {
                entry.title = "t".repeat(100);
                entry.version = "v".repeat(100);
                entry.sha1 = format!("{number:040x}");
                entry.sha512 = format!("{number:0128x}");
                entry.resolved_at = number as u64 + 1;
            }
        })
        .unwrap();
        assert!(fs::metadata(game.join(FILE)).unwrap().len() <= MAX_BYTES);
        fs::remove_dir_all(game).unwrap();
    }

    #[test]
    fn icons_are_saved_once_under_a_validated_hash() {
        let root = temp_dir();
        let sha1 = "c".repeat(40);
        save_icon(&root, &sha1, b"png").unwrap();
        save_icon(&root, &sha1, b"otro").unwrap();
        assert_eq!(fs::read(icon_path(&root, &sha1).unwrap()).unwrap(), b"png");
        assert!(save_icon(&root, "../../x", b"png").is_err());
        assert!(save_icon(&root, &sha1, &vec![0; ICON_LIMIT + 1]).is_err());
        assert!(icon_path(&root, "no-hex").is_none());
        let names: Vec<_> = fs::read_dir(root.join("cache/content-icons")).unwrap().collect();
        assert_eq!(names.len(), 1);
        fs::remove_dir_all(root).unwrap();
    }
}
