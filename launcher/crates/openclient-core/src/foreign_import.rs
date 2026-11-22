//! Importación de packs de otros launchers: CurseForge (`.zip`) y MultiMC/Prism (carpeta o `.zip`).
//!
//! Todo lo que llega es hostil: las rutas pasan por `modpack::file_path`, los enlaces se rechazan,
//! los tamaños se limitan igual que en `modpack.rs` y nada se publica hasta que la instancia
//! está completa (`instance_storage::publish_instance`).
use crate::{
    instance_storage::publish_instance,
    modpack::{
        file_path, loader_version_valid, write_manifest, Loader, Origin, PackFile, PackManifest,
    },
    portable_instance::{copy_game, isolated_path, DIRECTORIES, FILES},
    GameContext,
};
use serde_json::Value;
use sha2::{Digest, Sha512};
use std::{
    collections::HashSet,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};
use zip::ZipArchive;

// Mismos límites que `modpack.rs` (allí son privados).
const FILE_LIMIT: u64 = 128 * 1024 * 1024;
const PACK_LIMIT: u64 = 256 * 1024 * 1024;
const OVERRIDES_LIMIT: u64 = 512 * 1024 * 1024;
const CURSE_ENTRIES: usize = 8000;
const CURSE_FILES: usize = 2500;
const CURSE_MANIFEST_LIMIT: u64 = 2 * 1024 * 1024;
// Límites propios de MultiMC/Prism (el usuario aporta sus mundos).
const MMC_TOTAL_LIMIT: u64 = 4 * 1024 * 1024 * 1024;
const MMC_ENTRIES: usize = 50_000;
const MMC_PACK_LIMIT: u64 = 1024 * 1024;
const MMC_CFG_LIMIT: u64 = 64 * 1024;
const MMC_COMPONENTS: usize = 64;
const UNRESOLVED_FILE: &str = "curseforge-unresolved.json";
const PACK_DIR: &str = ".openclient";
const INVALID_PATH: &str = "Ruta de modpack inválida.";

/// Claves de `instance.cfg` que se ignoran a propósito: solo se lee `name`.
/// Si aparece alguna, la importación avisa de que no se ha aplicado.
const IGNORED_CFG_KEYS: &[&str] = &[
    "JavaPath",
    "JvmArgs",
    "PreLaunchCommand",
    "PostExitCommand",
    "WrapperCommand",
    "OverrideCommands",
    "OverrideJavaLocation",
    "OverrideJavaArgs",
    "OverrideMemory",
    "JoinServerOnLaunch",
    "JoinServerOnLaunchAddress",
    "MinMemAlloc",
    "MaxMemAlloc",
];

/// Lo que se sabe de un pack ajeno antes de importarlo.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ForeignPack {
    pub name: String,
    pub version_number: String,
    pub minecraft: String,
    pub loader: Loader,
    pub loader_version: Option<String>,
    /// CurseForge: pares `(projectID, fileID)` que no se pueden descargar.
    pub unresolved: Vec<(u64, u64)>,
    /// Archivos que se copiarían tal cual a la instancia.
    pub override_files: usize,
    /// De ellos, los `.jar` bajo `mods/`.
    pub override_jars: usize,
    pub warnings: Vec<String>,
}

impl ForeignPack {
    /// Texto honesto para la confirmación de CurseForge.
    pub fn curseforge_message(&self) -> String {
        let count = self.unresolved.len();
        if count == 0 {
            return "Se importarán los archivos incluidos en el pack.".into();
        }
        let mut message = format!(
            "OpenClient no puede descargar los {count} mods de CurseForge (requiere una clave de API que no tenemos). Se importarán solo los archivos incluidos; tendrás que añadir esos mods a mano."
        );
        if self.override_jars == 0 {
            message.push_str(" La instancia probablemente no arrancará así.");
        }
        message
    }
}

fn plain_context(root: &Path) -> GameContext {
    GameContext::new(root.to_path_buf(), env!("CARGO_PKG_VERSION"), |_, _| Ok(()))
}

fn is_link(metadata: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return true;
        }
    }
    metadata.file_type().is_symlink()
}

fn printable(text: &str, max: usize) -> String {
    text.chars().filter(|c| !c.is_control()).take(max).collect()
}

/// Nombre válido (1-64 caracteres, sin controles) o `None`.
fn clean_name(raw: &str) -> Option<String> {
    let name = raw.trim();
    (!name.is_empty() && name.chars().count() <= 64 && !name.chars().any(char::is_control))
        .then(|| name.to_owned())
}

fn fallback_name(raw: &str) -> String {
    let name = printable(raw, 64);
    let name = name.trim();
    if name.is_empty() {
        "Instancia importada".into()
    } else {
        name.to_owned()
    }
}

/// Misma regla que "create": dígito inicial, hasta 40, `[A-Za-z0-9._-]`.
fn minecraft_version_valid(version: &str) -> bool {
    !version.is_empty()
        && version.len() <= 40
        && version.as_bytes()[0].is_ascii_digit()
        && version
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b".-_".contains(&byte))
}

fn library_root(root: &Path) -> Result<PathBuf, String> {
    let root = root.canonicalize().map_err(|error| error.to_string())?;
    let instances = root.join("instances");
    if let Ok(metadata) = fs::symlink_metadata(&instances) {
        if is_link(&metadata)
            || !metadata.is_dir()
            || instances
                .canonicalize()
                .map_err(|error| error.to_string())?
                != instances
        {
            return Err("La carpeta de instancias apunta a otra ubicación.".into());
        }
    }
    Ok(root)
}

fn open_zip(path: &Path, size_limit: u64, entries: usize) -> Result<ZipArchive<fs::File>, String> {
    if !path.is_absolute()
        || !path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("zip"))
    {
        return Err("Selecciona un archivo .zip.".into());
    }
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("No se pudo leer el archivo: {error}"))?;
    if is_link(&metadata) || !metadata.is_file() || !(1..=size_limit).contains(&metadata.len()) {
        return Err(format!(
            "El archivo debe ser un .zip de hasta {} MiB.",
            size_limit / (1024 * 1024)
        ));
    }
    let file =
        fs::File::open(path).map_err(|error| format!("No se pudo abrir el archivo: {error}"))?;
    let archive = ZipArchive::new(file).map_err(|error| format!("ZIP inválido: {error}"))?;
    if archive.len() > entries {
        return Err("El archivo contiene demasiadas entradas.".into());
    }
    Ok(archive)
}

/// Contenido de una entrada pequeña (`Ok(None)` si no existe).
fn read_entry(
    archive: &mut ZipArchive<fs::File>,
    name: &str,
    limit: u64,
) -> Result<Option<Vec<u8>>, String> {
    let entry = match archive.by_name(name) {
        Ok(entry) => entry,
        Err(zip::result::ZipError::FileNotFound) => return Ok(None),
        Err(error) => return Err(error.to_string()),
    };
    if entry.is_dir() || entry.is_symlink() || entry.size() > limit {
        return Err(format!(
            "{} es inválido o demasiado grande.",
            printable(name, 60)
        ));
    }
    let mut bytes = Vec::new();
    entry
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() as u64 > limit {
        return Err(format!("{} es demasiado grande.", printable(name, 60)));
    }
    Ok(Some(bytes))
}

struct Listed {
    index: usize,
    relative: String,
    size: u64,
}

/// Valida todas las entradas elegidas por `select` sin escribir nada.
/// Rutas con `.` o `..` son un error; el resto de segmentos ocultos se omiten.
fn list_entries(
    archive: &mut ZipArchive<fs::File>,
    select: &dyn Fn(&str) -> Option<String>,
    total_limit: u64,
    ctx: &GameContext,
) -> Result<Vec<Listed>, String> {
    let mut seen = HashSet::new();
    let mut total = 0u64;
    let mut listed = Vec::new();
    for index in 0..archive.len() {
        ctx.cancel.check()?;
        let entry = archive.by_index(index).map_err(|error| error.to_string())?;
        let name = entry
            .name()
            .map_err(|error| error.to_string())?
            .into_owned();
        if entry.is_dir() {
            continue;
        }
        let Some(relative) = select(&name).filter(|relative| !relative.is_empty()) else {
            continue;
        };
        if relative.split('/').any(|part| part == "." || part == "..") {
            return Err(INVALID_PATH.into());
        }
        // Marcadores ocultos (p. ej. `mods/.index`) no son contenido del juego.
        if relative.split('/').any(|part| part.starts_with('.')) {
            continue;
        }
        if entry.is_symlink() {
            return Err("El archivo contiene enlaces simbólicos.".into());
        }
        if !seen.insert(relative.to_lowercase()) {
            return Err("El archivo contiene rutas duplicadas.".into());
        }
        file_path(Path::new(""), &relative)?;
        let size = entry.size();
        if size > FILE_LIMIT || total + size > total_limit {
            return Err("Los archivos incluidos superan el límite de extracción.".into());
        }
        total += size;
        listed.push(Listed {
            index,
            relative,
            size,
        });
    }
    Ok(listed)
}

/// Extrae las entradas ya validadas calculando su SHA-512.
fn extract(
    archive: &mut ZipArchive<fs::File>,
    listed: &[Listed],
    stage: &Path,
    ctx: &GameContext,
) -> Result<Vec<PackFile>, String> {
    let mut files = Vec::with_capacity(listed.len());
    for (number, item) in listed.iter().enumerate() {
        ctx.cancel.check()?;
        let dest = file_path(stage, &item.relative)?;
        fs::create_dir_all(dest.parent().ok_or(INVALID_PATH)?)
            .map_err(|error| error.to_string())?;
        let mut entry = archive
            .by_index(item.index)
            .map_err(|error| error.to_string())?;
        if entry.size() != item.size {
            return Err("El archivo cambió durante la importación.".into());
        }
        let mut output = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&dest)
            .map_err(|error| error.to_string())?;
        let mut input = entry.by_ref().take(item.size + 1);
        let mut hasher = Sha512::new();
        let mut buffer = [0u8; 64 * 1024];
        let mut copied = 0u64;
        loop {
            let read = input.read(&mut buffer).map_err(|error| error.to_string())?;
            if read == 0 {
                break;
            }
            hasher.update(&buffer[..read]);
            output
                .write_all(&buffer[..read])
                .map_err(|error| error.to_string())?;
            copied += read as u64;
        }
        if copied != item.size {
            return Err("El tamaño de un archivo del pack no coincide.".into());
        }
        files.push(PackFile {
            path: item.relative.clone(),
            sha512: format!("{:x}", hasher.finalize()),
            size: item.size,
            origin: Origin::Override,
        });
        if (number + 1) % 200 == 0 {
            ctx.progress(
                "extracting",
                ((number + 1) * 100 / listed.len()).min(100) as u8,
                &format!("Copiando archivos incluidos ({})", number + 1),
            );
        }
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(files)
}

// ---------------------------------------------------------------- CurseForge

struct CurseFile {
    project: u64,
    file: u64,
    required: bool,
}

struct Curse {
    pack: ForeignPack,
    files: Vec<CurseFile>,
}

fn curse_loader(minecraft: &Value) -> Result<(Loader, Option<String>), String> {
    let empty = Vec::new();
    let list = match &minecraft["modLoaders"] {
        Value::Null => &empty,
        Value::Array(list) if list.len() <= 8 => list,
        _ => return Err("La lista de cargadores del pack es inválida.".into()),
    };
    let chosen = match list.as_slice() {
        [] => return Ok((Loader::Vanilla, None)),
        [only] => only,
        many => {
            let mut primary = many.iter().filter(|entry| entry["primary"] == true);
            match (primary.next(), primary.next()) {
                (Some(entry), None) => entry,
                _ => return Err("El pack declara varios cargadores sin uno principal.".into()),
            }
        }
    };
    let id = chosen["id"].as_str().unwrap_or_default();
    let unsupported = || format!("Cargador no compatible: {}", printable(id, 40));
    let (name, version) = id.split_once('-').ok_or_else(unsupported)?;
    let loader = match name {
        "forge" => Loader::Forge,
        "neoforge" => Loader::NeoForge,
        "fabric" => Loader::Fabric,
        "quilt" => Loader::Quilt,
        _ => return Err(unsupported()),
    };
    if !loader_version_valid(version) {
        return Err(format!(
            "Versión de cargador inválida: {}",
            printable(id, 40)
        ));
    }
    Ok((loader, Some(version.to_owned())))
}

/// Lee y valida el pack completo (manifiesto y rutas de `overrides`) sin escribir nada.
fn parse_curseforge(
    path: &Path,
    ctx: &GameContext,
) -> Result<(Curse, ZipArchive<fs::File>, Vec<Listed>), String> {
    let mut archive = open_zip(path, PACK_LIMIT, CURSE_ENTRIES)?;
    let bytes = read_entry(&mut archive, "manifest.json", CURSE_MANIFEST_LIMIT)?
        .ok_or("No se encontró manifest.json de CurseForge.")?;
    let manifest: Value =
        serde_json::from_slice(&bytes).map_err(|_| "El manifiesto de CurseForge es inválido.")?;
    if manifest["manifestType"] != "minecraftModpack" || manifest["manifestVersion"] != 1 {
        return Err("Formato de pack de CurseForge incompatible.".into());
    }
    let minecraft = manifest["minecraft"]["version"]
        .as_str()
        .filter(|version| minecraft_version_valid(version))
        .ok_or("La versión de Minecraft del pack no es válida.")?
        .to_owned();
    let (loader, loader_version) = curse_loader(&manifest["minecraft"])?;
    let folder = match &manifest["overrides"] {
        Value::Null => "overrides",
        Value::String(folder) => folder.as_str(),
        _ => return Err(INVALID_PATH.into()),
    };
    if folder.contains('/') {
        return Err(INVALID_PATH.into());
    }
    file_path(Path::new(""), folder)?;
    let prefix = format!("{folder}/");
    let mut files = Vec::new();
    match &manifest["files"] {
        Value::Null => {}
        Value::Array(list) if list.len() <= CURSE_FILES => {
            for item in list {
                let (Some(project), Some(file)) =
                    (item["projectID"].as_u64(), item["fileID"].as_u64())
                else {
                    return Err("Un mod del pack no tiene projectID o fileID válidos.".into());
                };
                files.push(CurseFile {
                    project,
                    file,
                    required: item["required"].as_bool().unwrap_or(true),
                });
            }
        }
        _ => return Err("La lista de mods del pack es inválida o demasiado grande.".into()),
    }
    let listed = list_entries(
        &mut archive,
        &|name| name.strip_prefix(&prefix).map(str::to_owned),
        OVERRIDES_LIMIT,
        ctx,
    )?;
    let jars = listed
        .iter()
        .filter(|item| {
            item.relative.starts_with("mods/")
                && item.relative.to_ascii_lowercase().ends_with(".jar")
        })
        .count();
    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or_default();
    let pack = ForeignPack {
        name: manifest["name"]
            .as_str()
            .and_then(clean_name)
            .unwrap_or_else(|| fallback_name(stem)),
        version_number: printable(manifest["version"].as_str().unwrap_or_default(), 100)
            .trim()
            .to_owned(),
        minecraft,
        loader,
        loader_version,
        unresolved: files.iter().map(|f| (f.project, f.file)).collect(),
        override_files: listed.len(),
        override_jars: jars,
        warnings: Vec::new(),
    };
    Ok((Curse { pack, files }, archive, listed))
}

/// Inspección sin tocar disco (solo lee el zip).
pub fn inspect_curseforge(path: &Path) -> Result<ForeignPack, String> {
    let ctx = plain_context(Path::new("."));
    parse_curseforge(path, &ctx).map(|(curse, _, _)| curse.pack)
}

pub fn publish_curseforge(
    path: &Path,
    root: &Path,
    id: uuid::Uuid,
    state: &str,
) -> Result<PackManifest, String> {
    publish_curseforge_with(path, root, id, state, &plain_context(root))
}

/// Publica la instancia con solo los archivos incluidos (`overrides`). Los mods que
/// CurseForge no nos deja descargar quedan en `.openclient/curseforge-unresolved.json`.
pub fn publish_curseforge_with(
    path: &Path,
    root: &Path,
    id: uuid::Uuid,
    state: &str,
    ctx: &GameContext,
) -> Result<PackManifest, String> {
    let (curse, mut archive, listed) = parse_curseforge(path, ctx)?;
    let root = library_root(root)?;
    let target = root.join("instances").join(id.to_string());
    let mut manifest = PackManifest::default();
    publish_instance(&root, &target, state, |stage| {
        manifest.files = extract(&mut archive, &listed, &stage.path, ctx)?;
        ctx.cancel.check()?;
        write_manifest(&stage.path, &manifest)?;
        let unresolved: Vec<Value> = curse
            .files
            .iter()
            .map(|f| serde_json::json!({"projectID": f.project, "fileID": f.file, "required": f.required}))
            .collect();
        fs::write(
            stage.path.join(PACK_DIR).join(UNRESOLVED_FILE),
            serde_json::to_vec(&unresolved).map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())
    })?;
    Ok(manifest)
}

// ------------------------------------------------------------- MultiMC/Prism

enum Layout {
    Folder {
        game: Option<PathBuf>,
    },
    Zip {
        archive: ZipArchive<fs::File>,
        game_prefix: Option<String>,
    },
}

struct Mmc {
    pack: ForeignPack,
    layout: Layout,
}

/// `name=` de `instance.cfg` y si traía alguna clave que se ignora. Nada más se lee.
fn parse_cfg(bytes: &[u8]) -> (Option<String>, bool) {
    let text = String::from_utf8_lossy(bytes);
    let text = text.trim_start_matches('\u{feff}');
    let (mut name, mut ignored) = (None, false);
    for line in text.lines() {
        let line = line.trim();
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        if key == "name" && name.is_none() {
            name = clean_name(value);
        } else if IGNORED_CFG_KEYS.contains(&key) {
            ignored = true;
        }
    }
    (name, ignored)
}

fn requires_minecraft(component: &Value) -> bool {
    component["cachedRequires"]
        .as_array()
        .is_some_and(|requires| requires.iter().any(|r| r["uid"] == "net.minecraft"))
}

/// `mmc-pack.json` → versión de Minecraft y cargador.
fn parse_mmc_pack(bytes: &[u8]) -> Result<(String, Loader, Option<String>), String> {
    let value: Value =
        serde_json::from_slice(bytes).map_err(|_| "mmc-pack.json es inválido.".to_string())?;
    if value["formatVersion"] != 1 {
        return Err("Formato de mmc-pack.json incompatible.".into());
    }
    let components = value["components"]
        .as_array()
        .filter(|list| list.len() <= MMC_COMPONENTS)
        .ok_or("La lista de componentes de la instancia es inválida.")?;
    let mut minecraft: Option<String> = None;
    let mut loader: Option<(Loader, Option<String>)> = None;
    for component in components {
        let uid = component["uid"]
            .as_str()
            .ok_or("Un componente de la instancia no tiene uid.")?;
        let version = component["version"].as_str();
        let kind = match uid {
            "net.minecraft" => None,
            "net.fabricmc.fabric-loader" => Some(Loader::Fabric),
            "org.quiltmc.quilt-loader" => Some(Loader::Quilt),
            "net.minecraftforge" => Some(Loader::Forge),
            "net.neoforged" => Some(Loader::NeoForge),
            _ => {
                let auxiliary = uid == "net.fabricmc.intermediary"
                    || uid == "org.quiltmc.hashed"
                    || uid.starts_with("org.lwjgl")
                    || uid.starts_with("net.minecraft.");
                if !auxiliary
                    && (requires_minecraft(component)
                        || uid.contains("liteloader")
                        || uid.contains("rift"))
                {
                    return Err(format!("Componente no compatible: {}", printable(uid, 60)));
                }
                continue;
            }
        };
        match kind {
            None => {
                let version = version
                    .filter(|version| minecraft_version_valid(version))
                    .ok_or("La versión de Minecraft de la instancia no es válida.")?;
                if minecraft.replace(version.to_owned()).is_some() {
                    return Err("La instancia declara Minecraft más de una vez.".into());
                }
            }
            Some(kind) => {
                let version = version
                    .filter(|version| loader_version_valid(version))
                    .ok_or("La versión del cargador de la instancia no es válida.")?;
                if loader.replace((kind, Some(version.to_owned()))).is_some() {
                    return Err("La instancia declara más de un cargador.".into());
                }
            }
        }
    }
    let minecraft = minecraft.ok_or("La instancia no declara su versión de Minecraft.")?;
    let (loader, version) = loader.unwrap_or((Loader::Vanilla, None));
    Ok((minecraft, loader, version))
}

/// Archivo pequeño dentro de la carpeta de la instancia, sin enlaces.
fn read_small(path: &Path, limit: u64) -> Result<Option<Vec<u8>>, String> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.to_string()),
    };
    if is_link(&metadata) || !metadata.is_file() || metadata.len() > limit {
        return Err("Un archivo de la instancia es inválido o demasiado grande.".into());
    }
    let mut bytes = Vec::new();
    fs::File::open(path)
        .and_then(|file| file.take(limit + 1).read_to_end(&mut bytes))
        .map_err(|error| error.to_string())?;
    if bytes.len() as u64 > limit {
        return Err("Un archivo de la instancia es demasiado grande.".into());
    }
    Ok(Some(bytes))
}

fn pack_from(
    cfg: &[u8],
    pack: &[u8],
    fallback: &str,
    game_missing: bool,
) -> Result<ForeignPack, String> {
    let (name, ignored) = parse_cfg(cfg);
    let (minecraft, loader, loader_version) = parse_mmc_pack(pack)?;
    let mut warnings = Vec::new();
    if ignored {
        warnings.push(
            "Se ignoraron la configuración de Java, argumentos, memoria y comandos de la instancia original."
                .to_owned(),
        );
    }
    if game_missing {
        warnings.push("La instancia no tiene carpeta del juego; se creará vacía.".to_owned());
    }
    Ok(ForeignPack {
        name: name.unwrap_or_else(|| fallback_name(fallback)),
        version_number: String::new(),
        minecraft,
        loader,
        loader_version,
        unresolved: Vec::new(),
        override_files: 0,
        override_jars: 0,
        warnings,
    })
}

fn open_mmc_folder(path: &Path) -> Result<Mmc, String> {
    if !path.is_absolute() || !path.is_dir() {
        return Err("Selecciona una carpeta o un .zip de MultiMC/Prism.".into());
    }
    let path = path.canonicalize().map_err(|error| error.to_string())?;
    let cfg = read_small(&path.join("instance.cfg"), MMC_CFG_LIMIT)?
        .ok_or("No se encontró instance.cfg en la carpeta.")?;
    let pack = read_small(&path.join("mmc-pack.json"), MMC_PACK_LIMIT)?
        .ok_or("No se encontró mmc-pack.json en la carpeta.")?;
    let candidates = [path.join(".minecraft"), path.join("minecraft")];
    let existing: Vec<&PathBuf> = candidates
        .iter()
        .filter(|candidate| fs::symlink_metadata(candidate).is_ok())
        .collect();
    let game = match existing.as_slice() {
        [] => None,
        [only] => {
            let metadata = fs::symlink_metadata(only).map_err(|error| error.to_string())?;
            if is_link(&metadata) || !metadata.is_dir() {
                return Err("La carpeta del juego de la instancia no es válida.".into());
            }
            Some((*only).clone())
        }
        _ => return Err("La instancia tiene .minecraft y minecraft a la vez.".into()),
    };
    let fallback = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    Ok(Mmc {
        pack: pack_from(&cfg, &pack, fallback, game.is_none())?,
        layout: Layout::Folder { game },
    })
}

fn open_mmc_zip(path: &Path) -> Result<Mmc, String> {
    let mut archive = open_zip(path, MMC_TOTAL_LIMIT, MMC_ENTRIES)?;
    let mut names = Vec::with_capacity(archive.len());
    for index in 0..archive.len() {
        let entry = archive
            .by_index_raw(index)
            .map_err(|error| error.to_string())?;
        names.push(
            entry
                .name()
                .map_err(|error| error.to_string())?
                .into_owned(),
        );
    }
    let has = |name: &str| names.iter().any(|candidate| candidate == name);
    let prefix = if has("instance.cfg") && has("mmc-pack.json") {
        String::new()
    } else {
        // Un único directorio de primer nivel; dos niveles no se admiten.
        let top: HashSet<&str> = names
            .iter()
            .filter_map(|name| name.split_once('/'))
            .map(|(directory, _)| directory)
            .filter(|directory| !directory.is_empty())
            .collect();
        let mut roots = top.into_iter().filter(|directory| {
            has(&format!("{directory}/instance.cfg")) && has(&format!("{directory}/mmc-pack.json"))
        });
        match (roots.next(), roots.next()) {
            (Some(directory), None) => format!("{directory}/"),
            (None, _) => {
                return Err("No se encontró instance.cfg y mmc-pack.json en el zip.".into())
            }
            _ => return Err("El zip contiene varias instancias.".into()),
        }
    };
    let cfg = read_entry(
        &mut archive,
        &format!("{prefix}instance.cfg"),
        MMC_CFG_LIMIT,
    )?
    .ok_or("No se encontró instance.cfg en el zip.")?;
    let pack = read_entry(
        &mut archive,
        &format!("{prefix}mmc-pack.json"),
        MMC_PACK_LIMIT,
    )?
    .ok_or("No se encontró mmc-pack.json en el zip.")?;
    let under = |folder: &str| {
        let wanted = format!("{prefix}{folder}/");
        names.iter().any(|name| name.starts_with(&wanted))
    };
    let game_prefix = match (under(".minecraft"), under("minecraft")) {
        (true, true) => return Err("La instancia tiene .minecraft y minecraft a la vez.".into()),
        (true, false) => Some(format!("{prefix}.minecraft/")),
        (false, true) => Some(format!("{prefix}minecraft/")),
        (false, false) => None,
    };
    let fallback = if prefix.is_empty() {
        path.file_stem().and_then(|stem| stem.to_str())
    } else {
        Some(prefix.trim_end_matches('/'))
    };
    Ok(Mmc {
        pack: pack_from(
            &cfg,
            &pack,
            fallback.unwrap_or_default(),
            game_prefix.is_none(),
        )?,
        layout: Layout::Zip {
            archive,
            game_prefix,
        },
    })
}

fn open_mmc(path: &Path) -> Result<Mmc, String> {
    if path.is_dir() {
        open_mmc_folder(path)
    } else {
        open_mmc_zip(path)
    }
}

/// Inspección de una instancia MultiMC/Prism (carpeta o `.zip`).
pub fn inspect_mmc(path: &Path) -> Result<ForeignPack, String> {
    open_mmc(path).map(|mmc| mmc.pack)
}

/// Solo las raíces de la lista blanca de `portable_instance`.
fn mmc_select(name: &str, game_prefix: &str) -> Option<String> {
    let relative = name.strip_prefix(game_prefix)?;
    match relative.split_once('/') {
        Some((first, rest)) if DIRECTORIES.contains(&first) && !rest.is_empty() => {
            Some(relative.to_owned())
        }
        None if FILES.contains(&relative) => Some(relative.to_owned()),
        _ => None,
    }
}

struct Scan {
    entries: usize,
    total: u64,
}

fn scan_item(
    path: &Path,
    relative: &str,
    hidden: bool,
    scan: &mut Scan,
    ctx: &GameContext,
) -> Result<(), String> {
    ctx.cancel.check()?;
    let metadata = fs::symlink_metadata(path).map_err(|error| error.to_string())?;
    if is_link(&metadata) {
        return Err("La instancia contiene un enlace; no se puede importar.".into());
    }
    scan.entries += 1;
    if scan.entries > MMC_ENTRIES {
        return Err("La instancia contiene demasiados archivos.".into());
    }
    if !hidden {
        file_path(Path::new(""), relative)?;
    }
    if metadata.is_dir() {
        for entry in fs::read_dir(path).map_err(|error| error.to_string())? {
            let entry = entry.map_err(|error| error.to_string())?;
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| "Un archivo de la instancia tiene un nombre no válido.")?;
            scan_item(
                &entry.path(),
                &format!("{relative}/{name}"),
                hidden || name.starts_with('.'),
                scan,
                ctx,
            )?;
        }
    } else if metadata.is_file() {
        if metadata.len() > FILE_LIMIT || scan.total + metadata.len() > MMC_TOTAL_LIMIT {
            return Err("La instancia supera el límite de tamaño de importación.".into());
        }
        scan.total += metadata.len();
    } else {
        return Err("La instancia contiene un archivo no compatible.".into());
    }
    Ok(())
}

/// Comprueba enlaces, rutas y límites de la lista blanca antes de copiar nada.
fn scan_game(game: &Path, ctx: &GameContext) -> Result<(), String> {
    let mut scan = Scan {
        entries: 0,
        total: 0,
    };
    for name in DIRECTORIES.iter().chain(FILES) {
        let path = game.join(name);
        if fs::symlink_metadata(&path).is_ok() {
            scan_item(&path, name, false, &mut scan, ctx)?;
        }
    }
    Ok(())
}

/// `copy_game` copia también lo oculto (p. ej. `mods/.index`); aquí se retira de la copia.
fn remove_hidden(directory: &Path) -> Result<(), String> {
    for entry in fs::read_dir(directory).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path).map_err(|error| error.to_string())?;
        if entry.file_name().to_string_lossy().starts_with('.') {
            if metadata.is_dir() {
                fs::remove_dir_all(&path)
            } else {
                fs::remove_file(&path)
            }
            .map_err(|error| error.to_string())?;
        } else if metadata.is_dir() {
            remove_hidden(&path)?;
        }
    }
    Ok(())
}

pub fn publish_mmc(path: &Path, root: &Path, id: uuid::Uuid, state: &str) -> Result<(), String> {
    publish_mmc_with(path, root, id, state, &plain_context(root))
}

/// Importa solo los datos del jugador (lista blanca de `portable_instance`). De `instance.cfg`
/// únicamente se usa el nombre; Java, argumentos, comandos y servidor no se leen.
pub fn publish_mmc_with(
    path: &Path,
    root: &Path,
    id: uuid::Uuid,
    state: &str,
    ctx: &GameContext,
) -> Result<(), String> {
    let root = library_root(root)?;
    if path.is_dir() {
        isolated_path(&root, path)?;
    } else if path
        .canonicalize()
        .map_err(|error| format!("No se pudo leer el archivo: {error}"))?
        .starts_with(&root)
    {
        return Err("Elige un archivo fuera del directorio de datos de OpenClient.".into());
    }
    let mmc = open_mmc(path)?;
    let target = root.join("instances").join(id.to_string());
    ctx.progress("copying", 0, "Copiando la instancia");
    match mmc.layout {
        Layout::Folder { game } => publish_instance(&root, &target, state, |stage| {
            let Some(game) = game else { return Ok(()) };
            scan_game(&game, ctx)?;
            ctx.cancel.check()?;
            copy_game(&game, &stage.path)?;
            remove_hidden(&stage.path)?;
            ctx.cancel.check()
        }),
        Layout::Zip {
            mut archive,
            game_prefix,
        } => {
            let listed = match &game_prefix {
                Some(prefix) => list_entries(
                    &mut archive,
                    &|name| mmc_select(name, prefix),
                    MMC_TOTAL_LIMIT,
                    ctx,
                )?,
                None => Vec::new(),
            };
            publish_instance(&root, &target, state, |stage| {
                extract(&mut archive, &listed, &stage.path, ctx)?;
                ctx.cancel.check()
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modpack::read_manifest;
    use std::io::Cursor;

    fn temp_dir(label: &str) -> PathBuf {
        let path =
            std::env::temp_dir().join(format!("openclient-{label}-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn zip_with(entries: &[(&str, &str)]) -> Vec<u8> {
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        for (name, contents) in entries {
            zip.start_file(*name, options).unwrap();
            zip.write_all(contents.as_bytes()).unwrap();
        }
        zip.finish().unwrap().into_inner()
    }

    fn write_zip(dir: &Path, name: &str, bytes: &[u8]) -> PathBuf {
        let path = dir.join(name);
        fs::write(&path, bytes).unwrap();
        path
    }

    /// Cambia el tamaño sin comprimir declarado en el directorio central de todas las entradas.
    fn patch_sizes(bytes: &mut [u8], size: u32) {
        for index in 0..bytes.len().saturating_sub(28) {
            if bytes[index..index + 4] == [0x50, 0x4b, 0x01, 0x02] {
                bytes[index + 24..index + 28].copy_from_slice(&size.to_le_bytes());
            }
        }
    }

    const CURSE: &str = r#"{"minecraft":{"version":"1.20.1","modLoaders":[{"id":"forge-47.2.0","primary":true}]},"manifestType":"minecraftModpack","manifestVersion":1,"name":"Pack CF","version":"1.2","files":[{"projectID":1,"fileID":10,"required":true},{"projectID":2,"fileID":20,"required":false},{"projectID":3,"fileID":30,"required":true}],"overrides":"overrides"}"#;

    fn curse_zip(dir: &Path, manifest: &str, extra: &[(&str, &str)]) -> PathBuf {
        let mut entries = vec![("manifest.json", manifest)];
        entries.extend_from_slice(extra);
        write_zip(dir, "pack.zip", &zip_with(&entries))
    }

    fn assert_untouched(root: &Path) {
        let instances = root.join("instances");
        if instances.exists() {
            assert_eq!(
                fs::read_dir(&instances).unwrap().count(),
                0,
                "{instances:?}"
            );
        }
        assert!(!root.join("launcher_state.json").exists());
    }

    fn files_under(directory: &Path) -> Vec<String> {
        fn walk(base: &Path, directory: &Path, out: &mut Vec<String>) {
            for entry in fs::read_dir(directory).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    walk(base, &path, out);
                } else {
                    out.push(
                        path.strip_prefix(base)
                            .unwrap()
                            .to_string_lossy()
                            .replace('\\', "/"),
                    );
                }
            }
        }
        let mut out = Vec::new();
        walk(directory, directory, &mut out);
        out.sort();
        out
    }

    #[test]
    fn curseforge_inspection_reads_the_zip_without_writing_anything() {
        let base = temp_dir("cf-inspect");
        let root = base.join("datos");
        let zip = curse_zip(
            &base,
            CURSE,
            &[
                ("overrides/config/a.txt", "a"),
                ("overrides/mods/x.jar", "jar"),
            ],
        );
        let pack = inspect_curseforge(&zip).unwrap();
        assert_eq!(pack.name, "Pack CF");
        assert_eq!(pack.version_number, "1.2");
        assert_eq!(pack.minecraft, "1.20.1");
        assert_eq!(
            (pack.loader, pack.loader_version.as_deref()),
            (Loader::Forge, Some("47.2.0"))
        );
        assert_eq!(pack.unresolved, vec![(1, 10), (2, 20), (3, 30)]);
        assert_eq!((pack.override_files, pack.override_jars), (2, 1));
        let message = pack.curseforge_message();
        assert!(message.contains("los 3 mods"), "{message}");
        assert!(!message.contains("no arrancará"));
        let empty = ForeignPack {
            override_jars: 0,
            ..pack
        };
        assert!(empty
            .curseforge_message()
            .contains("probablemente no arrancará"));
        assert!(!root.exists());
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn curseforge_import_keeps_only_overrides_and_records_the_unresolved_mods() {
        let base = temp_dir("cf-publish");
        let root = base.join("datos");
        fs::create_dir_all(&root).unwrap();
        let zip = curse_zip(
            &base,
            CURSE,
            &[
                ("overrides/config/a.txt", "a"),
                ("overrides/mods/x.jar", "jar"),
                ("overrides/mods/.index/x.pw.toml", "oculto"),
                ("overrides/saves/Mundo/level.dat", "nivel"),
                ("otra/cosa.txt", "fuera de overrides"),
            ],
        );
        let id = uuid::Uuid::new_v4();
        let state = r#"{"instances":[{"id":"cf"}]}"#;
        let manifest = publish_curseforge(&zip, &root, id, state).unwrap();
        let game = root.join("instances").join(id.to_string());
        assert_eq!(
            files_under(&game),
            [
                ".openclient/curseforge-unresolved.json",
                ".openclient/modpack.json",
                "config/a.txt",
                "mods/x.jar",
                "saves/Mundo/level.dat"
            ]
        );
        let paths: Vec<&str> = manifest.files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(
            paths,
            ["config/a.txt", "mods/x.jar", "saves/Mundo/level.dat"]
        );
        assert!(manifest.files.iter().all(|f| f.origin == Origin::Override));
        assert_eq!(read_manifest(&game).unwrap().unwrap(), manifest);
        let unresolved: Value = serde_json::from_slice(
            &fs::read(game.join(".openclient/curseforge-unresolved.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(unresolved.as_array().unwrap().len(), 3);
        assert_eq!(
            unresolved[1],
            serde_json::json!({"projectID": 2, "fileID": 20, "required": false})
        );
        assert_eq!(
            fs::read_to_string(root.join("launcher_state.json")).unwrap(),
            state
        );
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn curseforge_loader_selection() {
        let manifest = |loaders: &str| {
            format!(
                r#"{{"minecraft":{{"version":"1.21.1","modLoaders":{loaders}}},"manifestType":"minecraftModpack","manifestVersion":1,"files":[]}}"#
            )
        };
        let base = temp_dir("cf-loader");
        let inspect =
            |loaders: &str| inspect_curseforge(&curse_zip(&base, &manifest(loaders), &[]));
        let ok = |loaders: &str| {
            let pack = inspect(loaders).unwrap_or_else(|e| panic!("{loaders}: {e}"));
            (pack.loader, pack.loader_version)
        };
        assert_eq!(
            ok(r#"[{"id":"fabric-0.16.14"}]"#),
            (Loader::Fabric, Some("0.16.14".into()))
        );
        assert_eq!(
            ok(r#"[{"id":"quilt-0.26.0"}]"#),
            (Loader::Quilt, Some("0.26.0".into()))
        );
        assert_eq!(
            ok(r#"[{"id":"neoforge-21.1.1"}]"#),
            (Loader::NeoForge, Some("21.1.1".into()))
        );
        assert_eq!(
            ok(r#"[{"id":"fabric-0.16.14"},{"id":"forge-47.2.0","primary":true}]"#),
            (Loader::Forge, Some("47.2.0".into()))
        );
        assert_eq!(ok("[]"), (Loader::Vanilla, None));
        for bad in [
            r#"[{"id":"rift-1","primary":true}]"#,
            r#"[{"id":"rift-1"}]"#,
            r#"[{"id":"fabric-0.16.14"},{"id":"forge-47.2.0"}]"#,
            r#"[{"id":"fabric-0.1","primary":true},{"id":"forge-47.2.0","primary":true}]"#,
            r#"[{"id":"forge-../x"}]"#,
            r#"[{"id":"forge"}]"#,
            r#""forge""#,
        ] {
            assert!(inspect(bad).is_err(), "{bad}");
        }
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn curseforge_rejects_bad_manifests() {
        let base = temp_dir("cf-bad");
        let with = |from: &str, to: &str| CURSE.replace(from, to);
        for manifest in [
            with("minecraftModpack", "otro"),
            with(r#""manifestVersion":1"#, r#""manifestVersion":2"#),
            with(r#""overrides":"overrides""#, r#""overrides":"../x""#),
            with(r#""overrides":"overrides""#, r#""overrides":"a/b""#),
            with(r#""overrides":"overrides""#, r#""overrides":""#),
            with(r#""overrides":"overrides""#, r#""overrides":"CON""#),
            with(r#""overrides":"overrides""#, r#""overrides":7"#),
            with("1.20.1", "../1"),
            with(r#""fileID":10"#, r#""fileID":"10""#),
            with(r#""projectID":1,"#, ""),
            "no es json".to_owned(),
        ] {
            let zip = curse_zip(&base, &manifest, &[]);
            assert!(inspect_curseforge(&zip).is_err(), "{manifest}");
        }
        // Sin manifest.json.
        let zip = write_zip(&base, "pack.zip", &zip_with(&[("overrides/a.txt", "a")]));
        assert!(inspect_curseforge(&zip)
            .unwrap_err()
            .contains("manifest.json"));
        // Más de 2500 mods.
        let many: Vec<String> = (0..2501)
            .map(|n| format!(r#"{{"projectID":{n},"fileID":{n}}}"#))
            .collect();
        let manifest = format!(
            r#"{{"minecraft":{{"version":"1.20.1"}},"manifestType":"minecraftModpack","manifestVersion":1,"files":[{}]}}"#,
            many.join(",")
        );
        assert!(inspect_curseforge(&curse_zip(&base, &manifest, &[])).is_err());
        let manifest = manifest.replace(&format!(",{}", many[2500]), "");
        assert_eq!(
            inspect_curseforge(&curse_zip(&base, &manifest, &[]))
                .unwrap()
                .unresolved
                .len(),
            2500
        );
        // Ruta que no es .zip o relativa.
        assert!(inspect_curseforge(Path::new("pack.zip")).is_err());
        assert!(inspect_curseforge(&base.join("pack.txt")).is_err());
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn curseforge_hostile_entries_are_rejected_and_nothing_is_published() {
        let base = temp_dir("cf-hostile");
        let root = base.join("datos");
        fs::create_dir_all(&root).unwrap();
        for name in [
            "overrides/../evil.txt",
            "overrides/mods/../../evil.txt",
            "overrides/./a.txt",
            "overrides/mods/CON.jar",
            "overrides/config/NUL.txt",
            "overrides/config/lpt1",
            "overrides/config/ok.txt:stream",
            "overrides/config/punto.",
            "overrides/config/espacio ",
            "overrides/config\\a.txt",
            "overrides/C:/a.txt",
            "overrides//a.txt",
            "overrides/config/a?.txt",
        ] {
            let zip = curse_zip(&base, CURSE, &[(name, "x")]);
            assert!(inspect_curseforge(&zip).is_err(), "{name}");
            assert!(
                publish_curseforge(&zip, &root, uuid::Uuid::new_v4(), r#"{"instances":[]}"#)
                    .is_err(),
                "{name}"
            );
            assert_untouched(&root);
            assert!(!base.join("evil.txt").exists());
        }
        // Duplicados sin distinguir mayúsculas.
        let zip = curse_zip(
            &base,
            CURSE,
            &[
                ("overrides/Config/a.txt", "1"),
                ("overrides/config/A.txt", "2"),
            ],
        );
        assert!(inspect_curseforge(&zip).unwrap_err().contains("duplicadas"));
        // Las rutas con punto inicial se omiten, no se escriben.
        let zip = curse_zip(
            &base,
            CURSE,
            &[
                ("overrides/.openclient/modpack.json", "{}"),
                ("overrides/config/a.txt", "1"),
            ],
        );
        assert_eq!(inspect_curseforge(&zip).unwrap().override_files, 1);
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn curseforge_symlinks_inside_the_zip_are_rejected() {
        let base = temp_dir("cf-symlink");
        let root = base.join("datos");
        fs::create_dir_all(&root).unwrap();
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default();
        zip.start_file("manifest.json", options).unwrap();
        zip.write_all(CURSE.as_bytes()).unwrap();
        zip.add_symlink("overrides/config/enlace", "/etc/passwd", options)
            .unwrap();
        let path = write_zip(&base, "pack.zip", &zip.finish().unwrap().into_inner());
        assert!(inspect_curseforge(&path).unwrap_err().contains("enlaces"));
        assert!(publish_curseforge(&path, &root, uuid::Uuid::new_v4(), "{}").is_err());
        assert_untouched(&root);
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn curseforge_size_bombs_and_entry_floods_are_rejected() {
        let base = temp_dir("cf-bomb");
        let root = base.join("datos");
        fs::create_dir_all(&root).unwrap();
        // Un archivo que declara más de 128 MiB.
        let mut bytes = zip_with(&[
            ("manifest.json", CURSE),
            ("overrides/config/a.txt", "contenido"),
        ]);
        patch_sizes(&mut bytes, 129 * 1024 * 1024);
        let path = write_zip(&base, "pack.zip", &bytes);
        // `manifest.json` también declara 129 MiB: se rechaza antes de leerlo.
        assert!(inspect_curseforge(&path).is_err());
        // Varios archivos pequeños que suman más de 512 MiB declarados.
        let entries: Vec<(String, String)> = (0..6)
            .map(|n| (format!("overrides/config/{n}.txt"), "x".repeat(8)))
            .collect();
        let mut all: Vec<(&str, &str)> = vec![("manifest.json", CURSE)];
        all.extend(entries.iter().map(|(n, c)| (n.as_str(), c.as_str())));
        let mut bytes = zip_with(&all);
        let manifest_end = bytes
            .windows(4)
            .position(|w| w == [0x50, 0x4b, 0x01, 0x02])
            .unwrap();
        // Parchea solo las entradas de overrides (todas menos la primera del directorio central).
        let mut first = true;
        for index in manifest_end..bytes.len().saturating_sub(28) {
            if bytes[index..index + 4] == [0x50, 0x4b, 0x01, 0x02] {
                if first {
                    first = false;
                    continue;
                }
                bytes[index + 24..index + 28]
                    .copy_from_slice(&(100u32 * 1024 * 1024).to_le_bytes());
            }
        }
        let path = write_zip(&base, "pack.zip", &bytes);
        let error = inspect_curseforge(&path).unwrap_err();
        assert!(error.contains("límite"), "{error}");
        assert!(publish_curseforge(&path, &root, uuid::Uuid::new_v4(), "{}").is_err());
        assert_untouched(&root);
        // Un tamaño declarado menor que el real no engaña a la extracción.
        let mut bytes = zip_with(&[
            ("manifest.json", CURSE),
            ("overrides/config/a.txt", "0123456789"),
        ]);
        let mut first = true;
        for index in 0..bytes.len().saturating_sub(28) {
            if bytes[index..index + 4] == [0x50, 0x4b, 0x01, 0x02] {
                if first {
                    first = false;
                    continue;
                }
                bytes[index + 24..index + 28].copy_from_slice(&1u32.to_le_bytes());
            }
        }
        let path = write_zip(&base, "pack.zip", &bytes);
        assert!(publish_curseforge(&path, &root, uuid::Uuid::new_v4(), "{}").is_err());
        assert_untouched(&root);
        // Más de 8000 entradas.
        let names: Vec<String> = (0..8000).map(|n| format!("overrides/c/{n}.txt")).collect();
        let mut all: Vec<(&str, &str)> = vec![("manifest.json", CURSE)];
        all.extend(names.iter().map(|n| (n.as_str(), "")));
        let path = write_zip(&base, "pack.zip", &zip_with(&all));
        assert!(inspect_curseforge(&path)
            .unwrap_err()
            .contains("demasiadas"));
        all.pop();
        let path = write_zip(&base, "pack.zip", &zip_with(&all));
        assert_eq!(inspect_curseforge(&path).unwrap().override_files, 7999);
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn curseforge_cancellation_leaves_no_instance() {
        let base = temp_dir("cf-cancel");
        let root = base.join("datos");
        fs::create_dir_all(&root).unwrap();
        let zip = curse_zip(&base, CURSE, &[("overrides/config/a.txt", "a")]);
        let ctx = plain_context(&root);
        ctx.cancel.cancel();
        let error =
            publish_curseforge_with(&zip, &root, uuid::Uuid::new_v4(), "{}", &ctx).unwrap_err();
        assert!(error.starts_with("CANCELLED:"), "{error}");
        assert_untouched(&root);
        fs::remove_dir_all(base).unwrap();
    }

    // ---------------------------------------------------------------- MMC

    fn mmc_pack(components: &str) -> String {
        format!(r#"{{"formatVersion":1,"components":{components}}}"#)
    }

    const FABRIC_PACK: &str = r#"[{"uid":"org.lwjgl3","version":"3.3.3"},{"uid":"net.minecraft","version":"1.20.1","important":true},{"uid":"net.fabricmc.intermediary","version":"1.20.1","cachedRequires":[{"uid":"net.minecraft","equals":"1.20.1"}]},{"uid":"net.fabricmc.fabric-loader","version":"0.15.7","cachedRequires":[{"uid":"net.fabricmc.intermediary"}]}]"#;

    fn mmc_folder(base: &Path, cfg: &str, pack: &str) -> PathBuf {
        let folder = base.join("Mi Instancia");
        fs::create_dir_all(folder.join(".minecraft")).unwrap();
        fs::write(folder.join("instance.cfg"), cfg).unwrap();
        fs::write(folder.join("mmc-pack.json"), pack).unwrap();
        folder
    }

    fn put(folder: &Path, relative: &str, contents: &str) {
        let path = folder.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }

    #[test]
    fn mmc_component_mapping() {
        let ok = |components: &str| parse_mmc_pack(mmc_pack(components).as_bytes()).unwrap();
        assert_eq!(
            ok(FABRIC_PACK),
            ("1.20.1".into(), Loader::Fabric, Some("0.15.7".into()))
        );
        assert_eq!(
            ok(
                r#"[{"uid":"net.minecraft","version":"1.21.1"},{"uid":"org.quiltmc.hashed","version":"1.21.1"},{"uid":"org.quiltmc.quilt-loader","version":"0.26.0"}]"#
            ),
            ("1.21.1".into(), Loader::Quilt, Some("0.26.0".into()))
        );
        assert_eq!(
            ok(
                r#"[{"uid":"net.minecraft","version":"1.20.1"},{"uid":"net.minecraftforge","version":"47.2.0","cachedRequires":[{"uid":"net.minecraft","equals":"1.20.1"}]}]"#
            ),
            ("1.20.1".into(), Loader::Forge, Some("47.2.0".into()))
        );
        assert_eq!(
            ok(
                r#"[{"uid":"net.minecraft","version":"1.21.4"},{"uid":"net.neoforged","version":"21.4.100"}]"#
            ),
            ("1.21.4".into(), Loader::NeoForge, Some("21.4.100".into()))
        );
        assert_eq!(
            ok(r#"[{"uid":"net.minecraft","version":"1.21.4"}]"#),
            ("1.21.4".into(), Loader::Vanilla, None)
        );
        let err = |text: String| parse_mmc_pack(text.as_bytes()).unwrap_err();
        let mc = r#"{"uid":"net.minecraft","version":"1.20.1"}"#;
        for components in [
            format!(r#"[{mc},{{"uid":"com.mumfrey.liteloader","version":"1.12.2"}}]"#),
            format!(r#"[{mc},{{"uid":"org.dimdev.rift","version":"1"}}]"#),
            format!(
                r#"[{mc},{{"uid":"com.otro.parche","version":"1","cachedRequires":[{{"uid":"net.minecraft"}}]}}]"#
            ),
            format!(
                r#"[{mc},{{"uid":"net.neoforged","version":"21.4.100"}},{{"uid":"net.minecraftforge","version":"47.2.0"}}]"#
            ),
            format!(r#"[{mc},{mc}]"#),
            r#"[{"uid":"net.fabricmc.fabric-loader","version":"0.15.7"}]"#.to_owned(),
            r#"[{"uid":"net.minecraft","version":"../1"}]"#.to_owned(),
            format!(r#"[{mc},{{"uid":"net.minecraftforge","version":"../x"}}]"#),
            format!(r#"[{mc},{{"uid":"net.minecraftforge"}}]"#),
            "{}".to_owned(),
        ] {
            let text = mmc_pack(&components);
            assert!(parse_mmc_pack(text.as_bytes()).is_err(), "{components}");
        }
        assert!(err(mmc_pack(&format!(
            r#"[{mc},{{"uid":"com.mumfrey.liteloader","version":"1"}}]"#
        )))
        .starts_with("Componente no compatible: com.mumfrey.liteloader"));
        assert!(parse_mmc_pack(br#"{"formatVersion":2,"components":[]}"#).is_err());
        assert!(parse_mmc_pack(b"no json").is_err());
        let many = format!("[{}]", vec![mc; 65].join(","));
        assert!(parse_mmc_pack(mmc_pack(&many).as_bytes()).is_err());
    }

    #[test]
    fn mmc_hostile_instance_cfg_only_contributes_its_name() {
        let base = temp_dir("mmc-hostile");
        let root = base.join("datos");
        fs::create_dir_all(&root).unwrap();
        let cfg = "[General]\nConfigVersion=1.2\nJavaPath=C:\\x\\evil.exe\nJvmArgs=-Devil=1\nPreLaunchCommand=evil.exe --steal\nPostExitCommand=evil.exe\nWrapperCommand=evil\nOverrideCommands=true\nJoinServerOnLaunch=true\nJoinServerOnLaunchAddress=evil.example.com\nMinMemAlloc=1\nMaxMemAlloc=99999\nname=Mi pack\n";
        let folder = mmc_folder(&base, cfg, &mmc_pack(FABRIC_PACK));
        put(&folder, ".minecraft/mods/a.jar", "jar");
        put(&folder, ".minecraft/mods/.index/a.pw.toml", "toml");
        put(&folder, ".minecraft/.openclient/modpack.json", "{}");
        put(&folder, ".minecraft/saves/Mundo/level.dat", "nivel");
        put(&folder, ".minecraft/options.txt", "fov:90");
        put(&folder, ".minecraft/servers.dat", "servidores");
        put(&folder, ".minecraft/config/c.toml", "c");
        put(&folder, ".minecraft/logs/latest.log", "registro");
        put(&folder, ".minecraft/libraries/x.jar", "no");
        put(&folder, ".minecraft/launcher_profiles.json", "tokens");

        let info = inspect_mmc(&folder).unwrap();
        assert_eq!(info.name, "Mi pack");
        assert_eq!(
            (info.loader, info.loader_version.as_deref()),
            (Loader::Fabric, Some("0.15.7"))
        );
        assert_eq!(info.minecraft, "1.20.1");
        assert_eq!(info.warnings.len(), 1);
        assert!(info.warnings[0].contains("Java"));

        let id = uuid::Uuid::new_v4();
        let state = r#"{"instances":[{"id":"mmc"}]}"#;
        publish_mmc(&folder, &root, id, state).unwrap();
        let game = root.join("instances").join(id.to_string());
        assert_eq!(
            files_under(&game),
            [
                "config/c.toml",
                "mods/a.jar",
                "options.txt",
                "saves/Mundo/level.dat",
                "servers.dat"
            ]
        );
        for path in files_under(&game) {
            let text = fs::read_to_string(game.join(&path)).unwrap();
            assert!(!text.contains("evil") && !text.contains("steal"), "{path}");
        }
        assert!(!game.join("mods/.index").exists());
        assert!(!game.join(".openclient").exists());
        assert_eq!(
            fs::read_to_string(root.join("launcher_state.json")).unwrap(),
            state
        );
        // El origen no se toca.
        assert!(folder.join("instance.cfg").is_file());
        assert!(folder.join(".minecraft/mods/.index/a.pw.toml").is_file());
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn mmc_name_comes_from_cfg_or_falls_back_to_the_folder() {
        let base = temp_dir("mmc-name");
        let pack = mmc_pack(FABRIC_PACK);
        for (cfg, expected) in [
            ("name=Bonito\n", "Bonito"),
            ("\u{feff}[General]\nname = Con espacios \n", "Con espacios"),
            ("", "Mi Instancia"),
            ("name=\n", "Mi Instancia"),
            (&format!("name={}\n", "a".repeat(65)), "Mi Instancia"),
            ("name=mal\u{7}nombre\n", "Mi Instancia"),
            ("Name=otro\nJvmArgs=-Dname=x\n", "Mi Instancia"),
        ] {
            let folder = mmc_folder(&base, cfg, &pack);
            assert_eq!(inspect_mmc(&folder).unwrap().name, expected, "{cfg:?}");
            fs::remove_dir_all(&folder).unwrap();
        }
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn mmc_folder_layout_errors() {
        let base = temp_dir("mmc-layout");
        let root = base.join("datos");
        fs::create_dir_all(&root).unwrap();
        let folder = mmc_folder(&base, "name=x\n", &mmc_pack(FABRIC_PACK));
        // `.minecraft` y `minecraft` a la vez.
        fs::create_dir_all(folder.join("minecraft")).unwrap();
        assert!(inspect_mmc(&folder).unwrap_err().contains("a la vez"));
        assert!(publish_mmc(&folder, &root, uuid::Uuid::new_v4(), "{}").is_err());
        assert_untouched(&root);
        fs::remove_dir(folder.join(".minecraft")).unwrap();
        // Solo `minecraft` vale; sin ninguna, la instancia queda vacía con un aviso.
        assert!(inspect_mmc(&folder).unwrap().warnings.is_empty());
        fs::remove_dir(folder.join("minecraft")).unwrap();
        let info = inspect_mmc(&folder).unwrap();
        assert!(info.warnings[0].contains("vacía"));
        let id = uuid::Uuid::new_v4();
        publish_mmc(&folder, &root, id, r#"{"instances":[]}"#).unwrap();
        assert_eq!(
            files_under(&root.join("instances").join(id.to_string())).len(),
            0
        );
        // Falta mmc-pack.json o es demasiado grande.
        fs::remove_file(folder.join("mmc-pack.json")).unwrap();
        assert!(inspect_mmc(&folder).is_err());
        fs::write(folder.join("mmc-pack.json"), "x".repeat(1024 * 1024 + 1)).unwrap();
        assert!(inspect_mmc(&folder).is_err());
        fs::write(folder.join("mmc-pack.json"), mmc_pack(FABRIC_PACK)).unwrap();
        fs::write(folder.join("instance.cfg"), "x".repeat(64 * 1024 + 1)).unwrap();
        assert!(inspect_mmc(&folder).is_err());
        // Rutas relativas o inexistentes.
        assert!(inspect_mmc(Path::new("relativa")).is_err());
        assert!(inspect_mmc(&base.join("no-existe")).is_err());
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn mmc_source_inside_the_library_is_refused() {
        let base = temp_dir("mmc-inside");
        let root = base.join("datos");
        fs::create_dir_all(root.join("instances")).unwrap();
        let folder = mmc_folder(&root.join("instances"), "name=x\n", &mmc_pack(FABRIC_PACK));
        let isolated = "Elige una carpeta fuera del directorio de datos de OpenClient.";
        assert_eq!(
            publish_mmc(&folder, &root, uuid::Uuid::new_v4(), "{}").unwrap_err(),
            isolated
        );
        assert_eq!(
            publish_mmc(&root, &root, uuid::Uuid::new_v4(), "{}").unwrap_err(),
            isolated
        );
        assert_eq!(
            publish_mmc(&base, &root, uuid::Uuid::new_v4(), "{}").unwrap_err(),
            isolated
        );
        let zip = write_zip(
            &root,
            "i.zip",
            &zip_with(&[("instance.cfg", ""), ("mmc-pack.json", "{}")]),
        );
        assert!(publish_mmc(&zip, &root, uuid::Uuid::new_v4(), "{}")
            .unwrap_err()
            .contains("fuera"));
        assert_eq!(fs::read_dir(root.join("instances")).unwrap().count(), 1);
        assert!(!root.join("launcher_state.json").exists());
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn mmc_folder_with_a_link_or_oversized_file_is_refused() {
        let base = temp_dir("mmc-link");
        let root = base.join("datos");
        fs::create_dir_all(&root).unwrap();
        let outside = base.join("fuera");
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("secreto.txt"), "no").unwrap();
        let folder = mmc_folder(&base, "name=x\n", &mmc_pack(FABRIC_PACK));
        put(&folder, ".minecraft/mods/a.jar", "a");
        let link = folder.join(".minecraft/config");
        let linked = {
            #[cfg(windows)]
            {
                std::os::windows::fs::symlink_dir(&outside, &link).is_ok()
                    || std::process::Command::new(
                        std::env::var_os("SystemRoot")
                            .map(|r| PathBuf::from(r).join("System32").join("cmd.exe"))
                            .unwrap_or_else(|| PathBuf::from("cmd")),
                    )
                    .args(["/C", "mklink", "/J"])
                    .arg(&link)
                    .arg(&outside)
                    .output()
                    .is_ok_and(|o| o.status.success())
            }
            #[cfg(unix)]
            {
                std::os::unix::fs::symlink(&outside, &link).is_ok()
            }
        };
        if linked {
            let error = publish_mmc(&folder, &root, uuid::Uuid::new_v4(), "{}").unwrap_err();
            assert!(error.contains("enlace"), "{error}");
            assert_untouched(&root);
            #[cfg(windows)]
            fs::remove_dir(&link).unwrap();
            #[cfg(unix)]
            fs::remove_file(&link).unwrap();
        } else {
            eprintln!("sin permiso para crear enlaces; se omite la comprobación");
        }
        // Un nombre inválido en Windows (dispositivo) se rechaza.
        put(&folder, ".minecraft/mods/CON.jar", "x");
        assert!(publish_mmc(&folder, &root, uuid::Uuid::new_v4(), "{}").is_err());
        assert_untouched(&root);
        fs::remove_dir_all(base).unwrap();
    }

    fn mmc_zip_entries<'a>(
        prefix: &'a str,
        game: &'a str,
        extra: &[(&'a str, &'a str)],
    ) -> Vec<(String, String)> {
        let mut entries = vec![
            (
                format!("{prefix}instance.cfg"),
                "name=Zip pack\nPreLaunchCommand=evil.exe\n".to_owned(),
            ),
            (format!("{prefix}mmc-pack.json"), mmc_pack(FABRIC_PACK)),
            (format!("{prefix}{game}/mods/a.jar"), "jar".to_owned()),
            (
                format!("{prefix}{game}/mods/.index/a.pw.toml"),
                "toml".to_owned(),
            ),
            (
                format!("{prefix}{game}/saves/W/level.dat"),
                "nivel".to_owned(),
            ),
            (format!("{prefix}{game}/options.txt"), "fov".to_owned()),
            (format!("{prefix}{game}/logs/latest.log"), "log".to_owned()),
            (format!("{prefix}{game}/libraries/l.jar"), "lib".to_owned()),
        ];
        entries.extend(
            extra
                .iter()
                .map(|(n, c)| (format!("{prefix}{game}/{n}"), (*c).to_owned())),
        );
        entries
    }

    fn as_zip(entries: &[(String, String)]) -> Vec<u8> {
        let refs: Vec<(&str, &str)> = entries
            .iter()
            .map(|(n, c)| (n.as_str(), c.as_str()))
            .collect();
        zip_with(&refs)
    }

    #[test]
    fn mmc_zip_accepts_one_level_of_nesting_and_only_the_allow_list() {
        let base = temp_dir("mmc-zip");
        let root = base.join("datos");
        fs::create_dir_all(&root).unwrap();
        for (prefix, game) in [
            ("", ".minecraft"),
            ("Mi Instancia/", "minecraft"),
            ("Mi Instancia/", ".minecraft"),
        ] {
            let zip = write_zip(&base, "i.zip", &as_zip(&mmc_zip_entries(prefix, game, &[])));
            let info = inspect_mmc(&zip).unwrap();
            assert_eq!(info.name, "Zip pack");
            assert_eq!(info.loader, Loader::Fabric);
            assert!(info.warnings.iter().any(|w| w.contains("Java")));
            let id = uuid::Uuid::new_v4();
            publish_mmc(&zip, &root, id, r#"{"instances":[]}"#).unwrap();
            let target = root.join("instances").join(id.to_string());
            assert_eq!(
                files_under(&target),
                ["mods/a.jar", "options.txt", "saves/W/level.dat"],
                "{prefix}{game}"
            );
        }
        // Dos niveles, o dos instancias, se rechazan.
        let deep = write_zip(
            &base,
            "d.zip",
            &as_zip(&mmc_zip_entries("a/b/", ".minecraft", &[])),
        );
        assert!(inspect_mmc(&deep).is_err());
        let mut two = mmc_zip_entries("a/", ".minecraft", &[]);
        two.extend(mmc_zip_entries("b/", ".minecraft", &[]));
        assert!(inspect_mmc(&write_zip(&base, "t.zip", &as_zip(&two)))
            .unwrap_err()
            .contains("varias"));
        // `.minecraft` y `minecraft` a la vez.
        let mut both = mmc_zip_entries("", ".minecraft", &[]);
        both.push(("minecraft/mods/b.jar".to_owned(), "jar".to_owned()));
        assert!(inspect_mmc(&write_zip(&base, "b.zip", &as_zip(&both)))
            .unwrap_err()
            .contains("a la vez"));
        // Sin la raíz esperada.
        assert!(inspect_mmc(&write_zip(&base, "n.zip", &zip_with(&[("hola.txt", "x")]))).is_err());
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn mmc_zip_hostile_entries_are_rejected() {
        let base = temp_dir("mmc-zip-bad");
        let root = base.join("datos");
        fs::create_dir_all(&root).unwrap();
        for name in [
            "mods/../../evil.txt",
            "config/../../../evil.txt",
            "mods/CON.jar",
            "config/a:b",
            "config/a\\b",
            "config/punto.",
        ] {
            let zip = write_zip(
                &base,
                "i.zip",
                &as_zip(&mmc_zip_entries("", ".minecraft", &[(name, "x")])),
            );
            assert!(
                publish_mmc(&zip, &root, uuid::Uuid::new_v4(), "{}").is_err(),
                "{name}"
            );
            assert_untouched(&root);
            assert!(!base.join("evil.txt").exists() && !root.join("evil.txt").exists());
        }
        // Enlace simbólico dentro de la lista blanca.
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default();
        for (name, contents) in [
            ("instance.cfg", "name=x"),
            ("mmc-pack.json", &mmc_pack(FABRIC_PACK) as &str),
        ] {
            zip.start_file(name, options).unwrap();
            zip.write_all(contents.as_bytes()).unwrap();
        }
        zip.add_symlink(".minecraft/config/enlace", "/etc/passwd", options)
            .unwrap();
        let path = write_zip(&base, "s.zip", &zip.finish().unwrap().into_inner());
        assert!(publish_mmc(&path, &root, uuid::Uuid::new_v4(), "{}")
            .unwrap_err()
            .contains("enlaces"));
        assert_untouched(&root);
        // Bomba de tamaño: un archivo que declara 129 MiB y total por encima del límite.
        let mut bytes = as_zip(&mmc_zip_entries("", ".minecraft", &[]));
        patch_sizes(&mut bytes, 129 * 1024 * 1024);
        let path = write_zip(&base, "b.zip", &bytes);
        // instance.cfg y mmc-pack.json también declaran 129 MiB: se rechaza al leerlos.
        assert!(publish_mmc(&path, &root, uuid::Uuid::new_v4(), "{}").is_err());
        assert_untouched(&root);
        let mut bytes = as_zip(&mmc_zip_entries(
            "",
            ".minecraft",
            &[("config/grande.bin", "x")],
        ));
        let mut count = 0;
        for index in 0..bytes.len().saturating_sub(28) {
            if bytes[index..index + 4] == [0x50, 0x4b, 0x01, 0x02] {
                count += 1;
                // Salta instance.cfg y mmc-pack.json (las dos primeras).
                if count > 2 {
                    bytes[index + 24..index + 28]
                        .copy_from_slice(&(129u32 * 1024 * 1024).to_le_bytes());
                }
            }
        }
        let path = write_zip(&base, "c.zip", &bytes);
        let error = publish_mmc(&path, &root, uuid::Uuid::new_v4(), "{}").unwrap_err();
        assert!(error.contains("límite"), "{error}");
        assert_untouched(&root);
        fs::remove_dir_all(base).unwrap();
    }
}
