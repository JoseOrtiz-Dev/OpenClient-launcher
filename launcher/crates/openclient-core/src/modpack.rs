use crate::{cancel::guard, CancelToken, GameContext};
use futures_util::{stream, StreamExt, TryStreamExt};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha512};
use std::{
    collections::HashSet,
    fs,
    io::{Cursor, Read, Write},
    path::{Path, PathBuf},
    time::Duration,
};

const FILE_LIMIT: u64 = 128 * 1024 * 1024;
const DOWNLOAD_LIMIT: u64 = 256 * 1024 * 1024;
const PACK_LIMIT: u64 = 256 * 1024 * 1024;
const OVERRIDES_LIMIT: u64 = 512 * 1024 * 1024;
const MANIFEST_LIMIT: u64 = 4 * 1024 * 1024;
const MANIFEST_ENTRIES: usize = 2500 + 8000;
const MANIFEST_DIR: &str = ".openclient";
const MANIFEST_FILE: &str = "modpack.json";

/// Cargador que declara un modpack.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Loader {
    Vanilla,
    Fabric,
    Quilt,
    Forge,
    NeoForge,
}

impl Loader {
    /// Categoría de instancia equivalente.
    pub fn category(&self) -> &'static str {
        match self {
            Self::Vanilla => "vanilla",
            Self::Fabric => "fabric",
            Self::Quilt => "quilt",
            Self::Forge => "forge",
            Self::NeoForge => "neoforge",
        }
    }

    /// Clave de `dependencies` en `modrinth.index.json` (`None` para Vanilla).
    pub fn index_key(&self) -> Option<&'static str> {
        match self {
            Self::Vanilla => None,
            Self::Fabric => Some("fabric-loader"),
            Self::Quilt => Some("quilt-loader"),
            Self::Forge => Some("forge"),
            Self::NeoForge => Some("neoforge"),
        }
    }

    fn label(&self) -> &'static str {
        match self {
            Self::Vanilla => "Vanilla",
            Self::Fabric => "Fabric",
            Self::Quilt => "Quilt",
            Self::Forge => "Forge",
            Self::NeoForge => "NeoForge",
        }
    }
}

/// Dependencias validadas de un modpack.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackDependencies {
    pub minecraft: String,
    pub loader: Loader,
    pub loader_version: Option<String>,
}

/// Origen de un archivo registrado en el manifiesto de la instancia.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Origin {
    Download,
    Override,
}

/// Archivo instalado por un modpack (ruta relativa a la carpeta del juego).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackFile {
    pub path: String,
    pub sha512: String,
    pub size: u64,
    pub origin: Origin,
}

/// Lista de archivos que un modpack dejó en una instancia.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackManifest {
    pub files: Vec<PackFile>,
}

fn manifest_path(game_dir: &Path) -> PathBuf {
    game_dir.join(MANIFEST_DIR).join(MANIFEST_FILE)
}

fn validate_manifest(manifest: &PackManifest) -> Result<(), String> {
    if manifest.files.len() > MANIFEST_ENTRIES {
        return Err("El manifiesto del modpack tiene demasiadas entradas.".into());
    }
    let mut seen = HashSet::new();
    for file in &manifest.files {
        file_path(Path::new(""), &file.path)?;
        if file.sha512.len() != 128 || !file.sha512.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err("El manifiesto del modpack tiene un SHA-512 inválido.".into());
        }
        if file.size > DOWNLOAD_LIMIT {
            return Err("El manifiesto del modpack tiene un tamaño inválido.".into());
        }
        if !seen.insert(file.path.to_lowercase()) {
            return Err("El manifiesto del modpack tiene rutas duplicadas.".into());
        }
    }
    Ok(())
}

fn reject_link(path: &Path) -> Result<Option<fs::Metadata>, String> {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return Ok(None);
    };
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return Err("El manifiesto del modpack no puede ser un enlace.".into());
        }
    }
    if metadata.file_type().is_symlink() {
        return Err("El manifiesto del modpack no puede ser un enlace.".into());
    }
    Ok(Some(metadata))
}

/// Lee `.openclient/modpack.json`. Ausente: `Ok(None)`. Corrupto o inválido: `Err`.
pub fn read_manifest(game_dir: &Path) -> Result<Option<PackManifest>, String> {
    if reject_link(&game_dir.join(MANIFEST_DIR))?.is_none() {
        return Ok(None);
    }
    let path = manifest_path(game_dir);
    let Some(metadata) = reject_link(&path)? else {
        return Ok(None);
    };
    if !metadata.is_file() || metadata.len() > MANIFEST_LIMIT {
        return Err("El manifiesto del modpack es inválido o demasiado grande.".into());
    }
    let mut bytes = Vec::new();
    fs::File::open(&path)
        .and_then(|file| file.take(MANIFEST_LIMIT + 1).read_to_end(&mut bytes))
        .map_err(|error| format!("No se pudo leer el manifiesto del modpack: {error}"))?;
    if bytes.len() as u64 > MANIFEST_LIMIT {
        return Err("El manifiesto del modpack es demasiado grande.".into());
    }
    let value: Value = serde_json::from_slice(&bytes)
        .map_err(|_| "El manifiesto del modpack de la instancia está dañado.")?;
    if value["format"] != 1 {
        return Err("Formato de manifiesto de modpack incompatible.".into());
    }
    let manifest: PackManifest = serde_json::from_value(value)
        .map_err(|_| "El manifiesto del modpack de la instancia está dañado.")?;
    validate_manifest(&manifest)?;
    Ok(Some(manifest))
}

/// Escribe `.openclient/modpack.json` de forma atómica (`.tmp` + `rename`).
pub fn write_manifest(game_dir: &Path, manifest: &PackManifest) -> Result<(), String> {
    validate_manifest(manifest)?;
    let directory = game_dir.join(MANIFEST_DIR);
    reject_link(&directory)?;
    fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    let bytes = serde_json::to_vec(&serde_json::json!({
        "format": 1, "files": manifest.files
    }))
    .map_err(|error| error.to_string())?;
    if bytes.len() as u64 > MANIFEST_LIMIT {
        return Err("El manifiesto del modpack es demasiado grande.".into());
    }
    let path = directory.join(MANIFEST_FILE);
    let temporary = directory.join(format!("{MANIFEST_FILE}.tmp"));
    let result = fs::File::create(&temporary)
        .and_then(|mut file| file.write_all(&bytes).and_then(|_| file.sync_all()))
        .and_then(|_| fs::rename(&temporary, &path));
    if let Err(error) = result {
        let _ = fs::remove_file(&temporary);
        return Err(format!(
            "No se pudo guardar el manifiesto del modpack: {error}"
        ));
    }
    Ok(())
}

/// Añade `new` a `files`; una ruta repetida (sin distinguir mayúsculas) sustituye a la anterior.
fn absorb(files: &mut Vec<PackFile>, new: Vec<PackFile>) {
    use std::collections::hash_map::Entry;
    let mut index: std::collections::HashMap<String, usize> = files
        .iter()
        .enumerate()
        .map(|(position, file)| (file.path.to_lowercase(), position))
        .collect();
    for file in new {
        match index.entry(file.path.to_lowercase()) {
            Entry::Occupied(slot) => files[*slot.get()] = file,
            Entry::Vacant(slot) => {
                slot.insert(files.len());
                files.push(file);
            }
        }
    }
}

pub fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .https_only(true)
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(600))
        .user_agent("OpenClient/0.1 (Minecraft desktop launcher)")
        .build()
        .map_err(|error| error.to_string())
}

pub(crate) fn download_url(value: &Value) -> Option<reqwest::Url> {
    reqwest::Url::parse(value.as_str()?).ok().filter(|url| {
        url.scheme() == "https"
            && url.host_str() == Some("cdn.modrinth.com")
            && url.port().is_none()
            && url.username().is_empty()
            && url.password().is_none()
    })
}

pub async fn download(
    client: &reqwest::Client,
    url: &Value,
    sha512: &Value,
    size: &Value,
) -> Result<Vec<u8>, String> {
    download_cancellable(client, url, sha512, size, &CancelToken::default()).await
}

pub async fn download_cancellable(
    client: &reqwest::Client,
    url: &Value,
    sha512: &Value,
    size: &Value,
    cancel: &CancelToken,
) -> Result<Vec<u8>, String> {
    let url =
        download_url(url).ok_or("Este modpack necesita un archivo fuera del CDN de Modrinth.")?;
    let hash = sha512
        .as_str()
        .filter(|hash| hash.len() == 128 && hash.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .ok_or("Falta el SHA-512 de un archivo del modpack.")?;
    let size = size
        .as_u64()
        .filter(|size| (1..=DOWNLOAD_LIMIT).contains(size))
        .ok_or("Tamaño de archivo de modpack fuera del límite.")?;
    let mut last = String::new();
    for attempt in 0..3u64 {
        cancel.check()?;
        match guard(cancel, fetch(client, url.clone(), hash, size, cancel)).await {
            Ok(bytes) => return Ok(bytes),
            Err(error) if error == crate::cancel::CANCELLED => return Err(error),
            Err(error) => last = error,
        }
        guard(cancel, async {
            tokio::time::sleep(Duration::from_millis(700 * (attempt + 1))).await;
            Ok(())
        })
        .await?;
    }
    Err(last)
}

async fn fetch(
    client: &reqwest::Client,
    url: reqwest::Url,
    hash: &str,
    size: u64,
    cancel: &CancelToken,
) -> Result<Vec<u8>, String> {
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|error| error.to_string())?;
    if !response.status().is_success() {
        return Err(format!(
            "Falló una descarga del modpack ({})",
            response.status()
        ));
    }
    let mut chunks = response.bytes_stream();
    let mut bytes = Vec::new();
    while let Some(chunk) = chunks.try_next().await.map_err(|error| error.to_string())? {
        cancel.check()?;
        if bytes.len() as u64 + chunk.len() as u64 > size {
            return Err("Una descarga supera el tamaño indicado.".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    if bytes.len() as u64 != size
        || !format!("{:x}", Sha512::digest(&bytes)).eq_ignore_ascii_case(hash)
    {
        return Err("Falló la verificación de un archivo del modpack.".into());
    }
    Ok(bytes)
}

pub struct Modpack {
    archive: zip::ZipArchive<Cursor<Vec<u8>>>,
    index: Value,
}

impl Modpack {
    pub fn open_local(path: &Path) -> Result<Self, String> {
        if !path.is_absolute()
            || !path
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| extension.eq_ignore_ascii_case("mrpack"))
        {
            return Err("Selecciona un archivo .mrpack.".into());
        }
        let metadata = fs::symlink_metadata(path)
            .map_err(|error| format!("No se pudo leer el archivo: {error}"))?;
        if !metadata.is_file() || !(1..=PACK_LIMIT).contains(&metadata.len()) {
            return Err("El archivo debe ser un .mrpack de hasta 256 MiB.".into());
        }
        let mut bytes = Vec::new();
        fs::File::open(path)
            .and_then(|file| file.take(PACK_LIMIT + 1).read_to_end(&mut bytes))
            .map_err(|error| format!("No se pudo abrir el modpack: {error}"))?;
        if bytes.len() as u64 > PACK_LIMIT {
            return Err("El modpack supera el límite de 256 MiB.".into());
        }
        Self::open(bytes)
    }

    pub fn open(bytes: Vec<u8>) -> Result<Self, String> {
        let mut archive =
            zip::ZipArchive::new(Cursor::new(bytes)).map_err(|error| error.to_string())?;
        if archive.len() > 8000 {
            return Err("El modpack contiene demasiados archivos.".into());
        }
        let manifest = archive
            .by_name("modrinth.index.json")
            .map_err(|_| "No se encontró el manifiesto del modpack.")?;
        if manifest.size() > 2 * 1024 * 1024 {
            return Err("El manifiesto del modpack es demasiado grande.".into());
        }
        let index: Value = serde_json::from_reader(manifest.take(2 * 1024 * 1024 + 1))
            .map_err(|_| "El manifiesto del modpack es inválido.")?;
        if index["formatVersion"] != 1 || index["game"] != "minecraft" {
            return Err("Formato de modpack incompatible.".into());
        }
        Ok(Self { archive, index })
    }

    /// Dependencias validadas, incluidos Quilt, Forge y NeoForge.
    pub fn pack_dependencies(&self) -> Result<PackDependencies, String> {
        let (minecraft, loader, loader_version) = parse_dependencies(&self.index)?;
        Ok(PackDependencies {
            minecraft: minecraft.to_owned(),
            loader,
            loader_version: loader_version.map(str::to_owned),
        })
    }

    /// Versión de Minecraft y de Fabric; solo admite packs Vanilla y Fabric.
    pub fn dependencies(&self) -> Result<(&str, Option<&str>), String> {
        let (minecraft, loader, version) = parse_dependencies(&self.index)?;
        match loader {
            Loader::Vanilla | Loader::Fabric => Ok((minecraft, version)),
            _ => Err("Esta versión de OpenClient admite modpacks Vanilla y Fabric.".into()),
        }
    }

    pub fn identity(&self) -> Result<(&str, &str), String> {
        let name = self.index["name"]
            .as_str()
            .filter(|value| !value.trim().is_empty())
            .ok_or("El modpack no declara su nombre.")?;
        let release = self.index["versionId"]
            .as_str()
            .filter(|value| !value.trim().is_empty())
            .ok_or("El modpack no declara su versión.")?;
        Ok((name, release))
    }

    /// Resumen para la interfaz: solo lee el manifiesto y los nombres del archivo.
    pub fn summary(&mut self) -> Result<Value, String> {
        let (name, version) = self.identity()?;
        let (name, version) = (name.to_owned(), version.to_owned());
        let dependencies = self.pack_dependencies()?;
        let fabric = (dependencies.loader == Loader::Fabric)
            .then(|| dependencies.loader_version.clone())
            .flatten();
        let mut groups: [Vec<String>; 3] = Default::default();
        let mut add = |path: &str| {
            let Some((folder, file)) = path.split_once('/') else {
                return;
            };
            let group = match folder {
                "mods" => 0,
                "shaderpacks" => 1,
                "resourcepacks" => 2,
                _ => return,
            };
            let lower = file.to_ascii_lowercase();
            if file.contains('/') || !(lower.ends_with(".jar") || lower.ends_with(".zip")) {
                return;
            }
            groups[group].push(display_name(file));
        };
        for file in self.index["files"].as_array().into_iter().flatten() {
            if file["env"]["client"] != "unsupported" {
                if let Some(path) = file["path"].as_str() {
                    add(path);
                }
            }
        }
        for index in 0..self.archive.len() {
            let entry = self
                .archive
                .by_index(index)
                .map_err(|error| error.to_string())?;
            let Ok(name) = entry.name() else { continue };
            let relative = name
                .strip_prefix("overrides/")
                .or_else(|| name.strip_prefix("client-overrides/"));
            if let Some(path) = relative.filter(|_| !entry.is_dir()) {
                add(path);
            }
        }
        for group in &mut groups {
            group.sort_by_key(|name| name.to_lowercase());
            group.dedup();
            group.truncate(500);
        }
        let [mods, shaders, resourcepacks] = groups;
        Ok(serde_json::json!({
            "name": name, "version": version, "minecraft": dependencies.minecraft, "fabric": fabric,
            "loader": dependencies.loader.category(), "loaderVersion": dependencies.loader_version,
            "mods": mods, "shaders": shaders, "resourcepacks": resourcepacks
        }))
    }

    pub fn publish(
        self,
        root: &Path,
        id: uuid::Uuid,
        state: &str,
        runtime: &tokio::runtime::Runtime,
        client: &reqwest::Client,
    ) -> Result<PackManifest, String> {
        let ctx = GameContext::new(root.to_path_buf(), env!("CARGO_PKG_VERSION"), |_, _| Ok(()));
        self.publish_with(root, id, state, runtime, client, &ctx)
    }

    /// Como `publish`, con cancelación y progreso a través de `ctx`.
    pub fn publish_with(
        mut self,
        root: &Path,
        id: uuid::Uuid,
        state: &str,
        runtime: &tokio::runtime::Runtime,
        client: &reqwest::Client,
        ctx: &GameContext,
    ) -> Result<PackManifest, String> {
        let root = root.canonicalize().map_err(|error| error.to_string())?;
        let instances = root.join("instances");
        if fs::symlink_metadata(&instances).is_ok()
            && instances
                .canonicalize()
                .map_err(|error| error.to_string())?
                != instances
        {
            return Err("La carpeta de instancias apunta a otra ubicación.".into());
        }
        let target = instances.join(id.to_string());
        let mut manifest = PackManifest::default();
        crate::instance_storage::publish_instance(&root, &target, state, |stage| {
            absorb(
                &mut manifest.files,
                runtime.block_on(self.install_files(&stage.path, client, ctx))?,
            );
            absorb(
                &mut manifest.files,
                self.extract_overrides(&stage.path, ctx)?,
            );
            ctx.cancel.check()?;
            write_manifest(&stage.path, &manifest)
        })?;
        Ok(manifest)
    }

    pub(crate) async fn install_files(
        &self,
        stage: &Path,
        client: &reqwest::Client,
        ctx: &GameContext,
    ) -> Result<Vec<PackFile>, String> {
        ctx.cancel.check()?;
        let files = self.index["files"]
            .as_array()
            .filter(|files| files.len() <= 2500)
            .ok_or("La lista de descargas del modpack es inválida o demasiado grande.")?;
        let mut seen = HashSet::new();
        let mut total = 0u64;
        let mut tasks = Vec::new();
        for file in files {
            if matches!(
                file["env"]["client"].as_str(),
                Some("unsupported" | "optional")
            ) {
                continue;
            }
            let path = file["path"]
                .as_str()
                .ok_or("Una descarga del modpack no tiene ruta.")?;
            let dest = file_path(stage, path)?;
            if !seen.insert(path.to_lowercase()) {
                return Err("El modpack contiene descargas duplicadas.".into());
            }
            let size = file["fileSize"]
                .as_u64()
                .filter(|size| (1..=DOWNLOAD_LIMIT).contains(size))
                .ok_or("Tamaño de descarga inválido.")?;
            total += size;
            if total > 4 * 1024 * 1024 * 1024 {
                return Err("El modpack supera el límite de 4 GiB.".into());
            }
            let url = file["downloads"]
                .as_array()
                .and_then(|urls| urls.iter().find(|url| download_url(url).is_some()))
                .ok_or("Este modpack necesita un archivo fuera del CDN de Modrinth.")?;
            tasks.push((
                dest,
                path,
                url,
                &file["hashes"]["sha512"],
                &file["fileSize"],
            ));
        }
        let count = tasks.len();
        let cancel = &ctx.cancel;
        let mut downloads = stream::iter(tasks.into_iter().map(
            |(dest, path, url, hash, size)| async move {
                let bytes = download_cancellable(client, url, hash, size, cancel).await?;
                cancel.check()?;
                fs::create_dir_all(dest.parent().ok_or("Ruta inválida.")?)
                    .map_err(|error| error.to_string())?;
                let length = bytes.len() as u64;
                fs::write(dest, bytes).map_err(|error| error.to_string())?;
                Ok::<_, String>(PackFile {
                    path: path.to_owned(),
                    sha512: hash.as_str().unwrap_or_default().to_ascii_lowercase(),
                    size: length,
                    origin: Origin::Download,
                })
            },
        ))
        .buffer_unordered(crate::download_concurrency());
        let mut installed = Vec::with_capacity(count);
        while let Some(file) = downloads.try_next().await? {
            installed.push(file);
            if installed.len() % 10 == 0 && installed.len() < count {
                ctx.progress(
                    "downloading",
                    (installed.len() * 100 / count) as u8,
                    &format!("Descargando archivos ({}/{count})", installed.len()),
                );
            }
        }
        if count > 0 {
            ctx.progress(
                "downloading",
                100,
                &format!("Descargando archivos ({count}/{count})"),
            );
        }
        installed.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(installed)
    }

    pub(crate) fn extract_overrides(
        &mut self,
        stage: &Path,
        ctx: &GameContext,
    ) -> Result<Vec<PackFile>, String> {
        ctx.cancel.check()?;
        let mut total = 0;
        let mut extracted = Vec::new();
        let entries = self.archive.len().max(1);
        for (pass, prefix) in ["overrides/", "client-overrides/"].into_iter().enumerate() {
            let mut seen = HashSet::new();
            for index in 0..self.archive.len() {
                ctx.cancel.check()?;
                let mut entry = self
                    .archive
                    .by_index(index)
                    .map_err(|error| error.to_string())?;
                let name = entry.name().map_err(|error| error.to_string())?;
                let Some(relative) = name.strip_prefix(prefix).filter(|name| !name.is_empty())
                else {
                    continue;
                };
                // Marcadores ocultos (p. ej. el .sha256 de un mod) no son contenido del juego.
                if entry.is_dir() || relative.split('/').any(|part| part.starts_with('.')) {
                    continue;
                }
                if entry.is_symlink() || !seen.insert(relative.to_lowercase()) {
                    return Err("El modpack contiene enlaces o rutas duplicadas.".into());
                }
                let relative = relative.to_owned();
                let dest = file_path(stage, &relative)?;
                let size = entry.size();
                if size > FILE_LIMIT || total + size > OVERRIDES_LIMIT {
                    return Err(
                        "Los archivos incluidos en el modpack superan el límite de extracción."
                            .into(),
                    );
                }
                total += size;
                fs::create_dir_all(dest.parent().ok_or("Ruta inválida.")?)
                    .map_err(|error| error.to_string())?;
                let mut output = fs::File::create(dest).map_err(|error| error.to_string())?;
                let mut input = entry.by_ref().take(size + 1);
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
                if copied != size {
                    return Err("El tamaño de un archivo del modpack no coincide.".into());
                }
                extracted.push(PackFile {
                    path: relative,
                    sha512: format!("{:x}", hasher.finalize()),
                    size,
                    origin: Origin::Override,
                });
                if extracted.len() % 200 == 0 {
                    ctx.progress(
                        "extracting",
                        ((pass * entries + index) * 100 / (2 * entries)).min(100) as u8,
                        &format!("Copiando archivos incluidos ({})", extracted.len()),
                    );
                }
            }
        }
        Ok(extracted)
    }
}

/// Nombre legible: sin extensión ni códigos de color de Minecraft.
fn display_name(file: &str) -> String {
    let stem = file.rsplit_once('.').map_or(file, |(stem, _)| stem);
    let mut out = String::new();
    let mut chars = stem.chars();
    while let Some(character) = chars.next() {
        if character == '\u{a7}' {
            chars.next();
        } else if !character.is_control() {
            out.push(character);
        }
    }
    out.trim().chars().take(80).collect()
}

fn version_token_valid(version: &str, max: usize) -> bool {
    !version.is_empty()
        && version.len() <= max
        && version.as_bytes()[0].is_ascii_digit()
        && version
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b".-_+".contains(&byte))
}

pub fn fabric_version_valid(version: &str) -> bool {
    version_token_valid(version, 48)
}

/// Versión de cargador (Fabric, Quilt, Forge, NeoForge): dígito inicial, `[A-Za-z0-9._+-]`, hasta 64.
pub fn loader_version_valid(version: &str) -> bool {
    version_token_valid(version, 64)
}

fn minecraft_version_valid(version: &str) -> bool {
    !version.is_empty()
        && version.len() <= 40
        && version.as_bytes()[0].is_ascii_digit()
        && version
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b".-_".contains(&byte))
}

fn parse_dependencies(index: &Value) -> Result<(&str, Loader, Option<&str>), String> {
    const LOADERS: [Loader; 4] = [
        Loader::Fabric,
        Loader::Quilt,
        Loader::Forge,
        Loader::NeoForge,
    ];
    let dependencies = index["dependencies"]
        .as_object()
        .ok_or("El modpack no declara sus dependencias.")?;
    let mut selected: Option<(Loader, &Value)> = None;
    for (key, value) in dependencies {
        if key == "minecraft" {
            continue;
        }
        let Some(loader) = LOADERS
            .into_iter()
            .find(|loader| loader.index_key() == Some(key.as_str()))
        else {
            let key: String = key.chars().filter(|c| !c.is_control()).take(40).collect();
            return Err(format!("Dependencia de modpack no admitida: {key}"));
        };
        if selected.replace((loader, value)).is_some() {
            return Err("El modpack declara más de un cargador.".into());
        }
    }
    let minecraft = dependencies
        .get("minecraft")
        .and_then(Value::as_str)
        .ok_or("El modpack no declara su versión de Minecraft.")?;
    if !minecraft_version_valid(minecraft) {
        return Err("La versión de Minecraft del modpack no es válida.".into());
    }
    let Some((loader, value)) = selected else {
        return Ok((minecraft, Loader::Vanilla, None));
    };
    let version = value
        .as_str()
        .filter(|version| loader_version_valid(version))
        .ok_or_else(|| format!("Versión de {} inválida.", loader.label()))?;
    Ok((minecraft, loader, Some(version)))
}

pub(crate) fn file_path(stage: &Path, relative: &str) -> Result<PathBuf, String> {
    let mut dest = stage.to_owned();
    if relative.len() > 240 {
        return Err("Ruta del modpack demasiado larga.".into());
    }
    for part in relative.split('/') {
        let device = part
            .split('.')
            .next()
            .unwrap_or_default()
            .trim_end()
            .to_ascii_uppercase();
        let reserved = matches!(device.as_str(), "CON" | "PRN" | "AUX" | "NUL")
            || device
                .strip_prefix("COM")
                .or_else(|| device.strip_prefix("LPT"))
                .is_some_and(|digit| {
                    matches!(
                        digit,
                        "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
                    )
                });
        if part.is_empty()
            || part.starts_with('.')
            || part.ends_with([' ', '.'])
            || reserved
            || part
                .chars()
                .any(|character| character.is_control() || "\\:<>|\"?*".contains(character))
        {
            return Err("Ruta de modpack inválida.".into());
        }
        dest.push(part);
    }
    Ok(dest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn pack_paths_reject_traversal_and_windows_devices() {
        let root = Path::new("instance");
        for path in [
            "../other.jar",
            "/other.jar",
            "mods//other.jar",
            "mods/../other.jar",
            "mods/.. /other.jar",
            "C:/other.jar",
            "mods\\other.jar",
            "mods/CON.jar",
            "mods/LPT².txt",
            "mods/ok.jar:stream",
            "mods/.openclient_managed.json",
        ] {
            assert!(file_path(root, path).is_err(), "{path}");
        }
        assert_eq!(
            file_path(root, "resourcepacks/Recursos bonitos.zip").unwrap(),
            root.join("resourcepacks").join("Recursos bonitos.zip"),
        );
    }

    #[test]
    fn pack_applies_client_overrides_and_ignores_server_files() {
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default();
        for (name, contents) in [
            (
                "modrinth.index.json",
                r#"{"formatVersion":1,"game":"minecraft","dependencies":{"minecraft":"1.21.4","fabric-loader":"0.16.14"},"files":[]}"#,
            ),
            ("overrides/config/test.txt", "common"),
            ("client-overrides/config/test.txt", "client"),
            ("server-overrides/config/test.txt", "server"),
        ] {
            zip.start_file(name, options).unwrap();
            zip.write_all(contents.as_bytes()).unwrap();
        }
        let mut pack = Modpack::open(zip.finish().unwrap().into_inner()).unwrap();
        assert_eq!(pack.dependencies().unwrap(), ("1.21.4", Some("0.16.14")));
        let target =
            std::env::temp_dir().join(format!("openclient-pack-test-{}", uuid::Uuid::new_v4()));
        let stage = crate::directory_install::PreparedDirectory::new(&target, false).unwrap();
        pack.extract_overrides(&stage.path, &test_ctx()).unwrap();
        assert_eq!(
            fs::read_to_string(stage.path.join("config/test.txt")).unwrap(),
            "client"
        );
    }

    #[test]
    fn hidden_overrides_are_skipped_and_summary_lists_content() {
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default();
        for (name, contents) in [
            (
                "modrinth.index.json",
                r#"{"formatVersion":1,"game":"minecraft","name":"Demo","versionId":"1.0.0","dependencies":{"minecraft":"1.21.4","fabric-loader":"0.16.14"},"files":[{"path":"mods/sodium.jar","env":{"client":"required"}},{"path":"mods/server.jar","env":{"client":"unsupported"}}]}"#,
            ),
            ("overrides/resourcepacks/\u{a7}bBonito \u{a7}a1.zip", "pack"),
            ("overrides/shaderpacks/Luz.zip.txt", "opciones"),
            ("overrides/resourcepacks/mod/.marca.sha256", "hash"),
        ] {
            zip.start_file(name, options).unwrap();
            zip.write_all(contents.as_bytes()).unwrap();
        }
        let mut pack = Modpack::open(zip.finish().unwrap().into_inner()).unwrap();
        let summary = pack.summary().unwrap();
        assert_eq!(summary["mods"], serde_json::json!(["sodium"]));
        assert_eq!(summary["resourcepacks"], serde_json::json!(["Bonito 1"]));
        assert_eq!(summary["shaders"], serde_json::json!([]));
        assert_eq!(summary["loader"], "fabric");
        assert_eq!(summary["loaderVersion"], "0.16.14");
        assert_eq!(summary["fabric"], "0.16.14");
        let target =
            std::env::temp_dir().join(format!("openclient-pack-test-{}", uuid::Uuid::new_v4()));
        let stage = crate::directory_install::PreparedDirectory::new(&target, false).unwrap();
        pack.extract_overrides(&stage.path, &test_ctx()).unwrap();
        assert!(stage
            .path
            .join("resourcepacks")
            .join("\u{a7}bBonito \u{a7}a1.zip")
            .is_file());
        assert!(!stage.path.join("resourcepacks/mod/.marca.sha256").exists());
    }

    /// Manual: OPENCLIENT_TEST_MRPACK=<ruta> cargo test --lib real_pack -- --ignored --nocapture
    #[test]
    #[ignore = "Requires a local .mrpack"]
    fn real_pack_opens_and_extracts() {
        let path = std::env::var("OPENCLIENT_TEST_MRPACK").unwrap();
        let mut pack = Modpack::open_local(Path::new(&path)).unwrap();
        let summary = pack.summary().unwrap();
        println!("{}", serde_json::to_string_pretty(&summary).unwrap());
        let target =
            std::env::temp_dir().join(format!("openclient-real-pack-{}", uuid::Uuid::new_v4()));
        let stage = crate::directory_install::PreparedDirectory::new(&target, false).unwrap();
        pack.extract_overrides(&stage.path, &test_ctx()).unwrap();
        assert!(stage.path.join("options.txt").is_file());
    }

    #[test]
    fn large_mod_downloads_are_accepted_but_not_unbounded_ones() {
        let index = |size: u64| {
            format!(
                r#"{{"formatVersion":1,"game":"minecraft","name":"Demo","versionId":"1","dependencies":{{"minecraft":"1.21.4"}},"files":[{{"path":"mods/grande.jar","fileSize":{size},"downloads":["https://example.com/a.jar"],"hashes":{{"sha512":"x"}}}}]}}"#
            )
        };
        let open = |size: u64| {
            let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
            zip.start_file(
                "modrinth.index.json",
                zip::write::SimpleFileOptions::default(),
            )
            .unwrap();
            zip.write_all(index(size).as_bytes()).unwrap();
            Modpack::open(zip.finish().unwrap().into_inner()).unwrap()
        };
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let client = client().unwrap();
        let stage = Path::new("instance");
        // 148 MiB pasa el tamaño y falla después, por el host que no es de Modrinth.
        let error = runtime
            .block_on(open(155_330_173).install_files(stage, &client, &test_ctx()))
            .unwrap_err();
        assert!(error.contains("fuera del CDN"), "{error}");
        let error = runtime
            .block_on(open(DOWNLOAD_LIMIT + 1).install_files(stage, &client, &test_ctx()))
            .unwrap_err();
        assert!(error.contains("Tamaño de descarga inválido"), "{error}");
    }

    fn test_ctx() -> GameContext {
        GameContext::new(PathBuf::from("."), "test", |_, _| Ok(()))
    }

    fn temp_dir(label: &str) -> PathBuf {
        let path =
            std::env::temp_dir().join(format!("openclient-{label}-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn pack_zip(index: &str, entries: &[(&str, &str)]) -> Vec<u8> {
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default();
        zip.start_file("modrinth.index.json", options).unwrap();
        zip.write_all(index.as_bytes()).unwrap();
        for (name, contents) in entries {
            zip.start_file(*name, options).unwrap();
            zip.write_all(contents.as_bytes()).unwrap();
        }
        zip.finish().unwrap().into_inner()
    }

    fn sha512_hex(contents: &str) -> String {
        format!("{:x}", Sha512::digest(contents.as_bytes()))
    }

    fn deps_pack(dependencies: &str) -> Result<PackDependencies, String> {
        let index = format!(
            r#"{{"formatVersion":1,"game":"minecraft","dependencies":{dependencies},"files":[]}}"#
        );
        Modpack::open(pack_zip(&index, &[]))?.pack_dependencies()
    }

    #[test]
    fn dependencies_table_accepts_one_loader_and_rejects_the_rest() {
        let ok = |json: &str, loader: Loader, version: Option<&str>| {
            let deps = deps_pack(json).unwrap_or_else(|error| panic!("{json}: {error}"));
            assert_eq!(deps.loader, loader, "{json}");
            assert_eq!(deps.loader_version.as_deref(), version, "{json}");
            assert_eq!(deps.loader.category(), loader.category());
        };
        ok(r#"{"minecraft":"1.21.4"}"#, Loader::Vanilla, None);
        ok(
            r#"{"minecraft":"1.21.4","fabric-loader":"0.16.14"}"#,
            Loader::Fabric,
            Some("0.16.14"),
        );
        ok(
            r#"{"minecraft":"1.21.1","quilt-loader":"0.26.0-beta.1"}"#,
            Loader::Quilt,
            Some("0.26.0-beta.1"),
        );
        ok(
            r#"{"minecraft":"1.20.1","forge":"47.2.0"}"#,
            Loader::Forge,
            Some("47.2.0"),
        );
        ok(
            r#"{"minecraft":"1.21.4","neoforge":"21.4.100"}"#,
            Loader::NeoForge,
            Some("21.4.100"),
        );
        for json in [
            r#"{"minecraft":"1.21.4","forge":"47.2.0","fabric-loader":"0.16.14"}"#,
            r#"{"minecraft":"1.21.4","quilt-loader":"0.26.0","neoforge":"21.4.100"}"#,
            r#"{"minecraft":"1.21.4","iris":"1.8.0"}"#,
            r#"{"minecraft":"../1"}"#,
            r#"{"minecraft":".1.21"}"#,
            r#"{"minecraft":"1.21/../x"}"#,
            r#"{"minecraft":"1.21.4","forge":"../1"}"#,
            r#"{"minecraft":"1.21.4","neoforge":7}"#,
            r#"{"fabric-loader":"0.16.14"}"#,
        ] {
            assert!(deps_pack(json).is_err(), "{json}");
        }
        let long = "1".repeat(65);
        assert!(deps_pack(&format!(r#"{{"minecraft":"1.21.4","forge":"{long}"}}"#)).is_err());
        assert!(loader_version_valid(&"1".repeat(64)));
        assert!(!loader_version_valid(""));
        assert!(!loader_version_valid("v1"));
        assert!(fabric_version_valid("0.16.14") && !fabric_version_valid(&"1".repeat(49)));
    }

    #[test]
    fn legacy_dependencies_only_accept_vanilla_and_fabric() {
        let open = |dependencies: &str| {
            Modpack::open(pack_zip(
                &format!(
                    r#"{{"formatVersion":1,"game":"minecraft","dependencies":{dependencies},"files":[]}}"#
                ),
                &[],
            ))
            .unwrap()
        };
        assert_eq!(
            open(r#"{"minecraft":"1.21.4"}"#).dependencies().unwrap(),
            ("1.21.4", None)
        );
        assert!(open(r#"{"minecraft":"1.21.4","quilt-loader":"0.26.0"}"#)
            .dependencies()
            .is_err());
    }

    #[test]
    fn publish_writes_manifest_sidecar_with_override_hashes() {
        let root = temp_dir("publish");
        let index = r#"{"formatVersion":1,"game":"minecraft","name":"Demo","versionId":"1.0","dependencies":{"minecraft":"1.21.4"},"files":[]}"#;
        let pack = Modpack::open(pack_zip(
            index,
            &[
                ("overrides/config/a.txt", "comun"),
                ("overrides/config/b.txt", "solo comun"),
                ("client-overrides/config/a.txt", "cliente"),
                ("server-overrides/config/a.txt", "servidor"),
            ],
        ))
        .unwrap();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let id = uuid::Uuid::new_v4();
        let manifest = pack
            .publish(
                &root,
                id,
                r#"{"instances":[]}"#,
                &runtime,
                &client().unwrap(),
            )
            .unwrap();
        let game = root
            .canonicalize()
            .unwrap()
            .join("instances")
            .join(id.to_string());
        assert!(game.join(".openclient/modpack.json").is_file());
        assert!(!game.join(".openclient/modpack.json.tmp").exists());
        let stored = read_manifest(&game).unwrap().unwrap();
        assert_eq!(stored, manifest);
        assert_eq!(stored.files.len(), 2);
        let a = stored
            .files
            .iter()
            .find(|f| f.path == "config/a.txt")
            .unwrap();
        assert_eq!(a.origin, Origin::Override);
        assert_eq!(a.sha512, sha512_hex("cliente"));
        assert_eq!(a.size, 7);
        let b = stored
            .files
            .iter()
            .find(|f| f.path == "config/b.txt")
            .unwrap();
        assert_eq!(b.sha512, sha512_hex("solo comun"));
        assert_eq!(
            fs::read_to_string(game.join("config/a.txt")).unwrap(),
            "cliente"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn cancelled_publish_leaves_no_instance_behind() {
        let root = temp_dir("publish-cancel");
        let index = r#"{"formatVersion":1,"game":"minecraft","dependencies":{"minecraft":"1.21.4"},"files":[]}"#;
        let pack = Modpack::open(pack_zip(index, &[("overrides/config/a.txt", "x")])).unwrap();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let ctx = test_ctx();
        ctx.cancel.cancel();
        let id = uuid::Uuid::new_v4();
        let error = pack
            .publish_with(
                &root,
                id,
                r#"{"instances":[]}"#,
                &runtime,
                &client().unwrap(),
                &ctx,
            )
            .unwrap_err();
        assert!(error.starts_with("CANCELLED:"), "{error}");
        let instances = root.join("instances");
        assert!(!instances.join(id.to_string()).exists());
        let leftovers = fs::read_dir(&instances).map(|dir| dir.count()).unwrap_or(0);
        assert_eq!(leftovers, 0);
        assert!(!root.join("launcher_state.json").exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn progress_is_reported_every_two_hundred_overrides() {
        use std::sync::{Arc, Mutex};
        let names: Vec<String> = (0..400)
            .map(|n| format!("overrides/config/f{n}.txt"))
            .collect();
        let entries: Vec<(&str, &str)> = names.iter().map(|n| (n.as_str(), "x")).collect();
        let index = r#"{"formatVersion":1,"game":"minecraft","dependencies":{"minecraft":"1.21.4"},"files":[]}"#;
        let mut pack = Modpack::open(pack_zip(index, &entries)).unwrap();
        let events = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&events);
        let ctx = GameContext::new(PathBuf::from("."), "test", move |event, payload| {
            sink.lock().unwrap().push((event.to_owned(), payload));
            Ok(())
        });
        let stage = temp_dir("progress");
        let files = pack.extract_overrides(&stage, &ctx).unwrap();
        assert_eq!(files.len(), 400);
        let events = events.lock().unwrap();
        assert_eq!(events.len(), 2);
        assert!(events
            .iter()
            .all(|(event, _)| event == "operation-progress"));
        let _ = fs::remove_dir_all(stage);
    }

    fn sample_manifest() -> PackManifest {
        PackManifest {
            files: vec![PackFile {
                path: "mods/a.jar".into(),
                sha512: sha512_hex("a"),
                size: 1,
                origin: Origin::Download,
            }],
        }
    }

    #[test]
    fn manifest_round_trips_and_missing_is_none() {
        let game = temp_dir("manifest");
        assert_eq!(read_manifest(&game).unwrap(), None);
        write_manifest(&game, &sample_manifest()).unwrap();
        assert_eq!(read_manifest(&game).unwrap(), Some(sample_manifest()));
        let raw: Value = serde_json::from_str(
            &fs::read_to_string(game.join(".openclient/modpack.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(raw["format"], 1);
        assert_eq!(raw["files"][0]["origin"], "download");
        let _ = fs::remove_dir_all(game);
    }

    #[test]
    fn manifest_rejects_hostile_or_corrupt_sidecars() {
        let game = temp_dir("manifest-bad");
        let path = game.join(".openclient");
        fs::create_dir_all(&path).unwrap();
        let file = path.join("modpack.json");
        let hash = sha512_hex("a");
        let entry =
            |p: &str| format!(r#"{{"path":"{p}","sha512":"{hash}","size":1,"origin":"override"}}"#);
        for (label, body) in [
            ("traversal", format!(r#"{{"format":1,"files":[{}]}}"#, entry("../x"))),
            ("hidden", format!(r#"{{"format":1,"files":[{}]}}"#, entry("mods/.x"))),
            ("format", r#"{"format":2,"files":[]}"#.to_owned()),
            ("corrupt", "{no es json".to_owned()),
            ("bad hash", r#"{"format":1,"files":[{"path":"a.txt","sha512":"zz","size":1,"origin":"override"}]}"#.to_owned()),
            (
                "duplicate",
                format!(r#"{{"format":1,"files":[{},{}]}}"#, entry("a.txt"), entry("A.txt")),
            ),
            ("origin", format!(r#"{{"format":1,"files":[{}]}}"#, entry("a.txt").replace("override", "evil"))),
        ] {
            fs::write(&file, body).unwrap();
            assert!(read_manifest(&game).is_err(), "{label}");
        }
        fs::write(&file, vec![b' '; MANIFEST_LIMIT as usize + 1]).unwrap();
        assert!(read_manifest(&game).is_err(), "oversized");
        let _ = fs::remove_dir_all(game);
    }

    #[test]
    fn write_manifest_refuses_invalid_entries() {
        let game = temp_dir("manifest-write");
        let mut manifest = sample_manifest();
        manifest.files[0].path = "mods/../x".into();
        assert!(write_manifest(&game, &manifest).is_err());
        let mut manifest = sample_manifest();
        manifest.files = vec![manifest.files[0].clone(); MANIFEST_ENTRIES + 1];
        assert!(write_manifest(&game, &manifest).is_err());
        assert!(!game.join(".openclient/modpack.json").exists());
        let _ = fs::remove_dir_all(game);
    }

    #[test]
    fn later_entries_replace_earlier_ones_case_insensitively() {
        let file = |path: &str, origin| PackFile {
            path: path.into(),
            sha512: sha512_hex(path),
            size: 1,
            origin,
        };
        let mut files = vec![
            file("mods/A.jar", Origin::Download),
            file("b.txt", Origin::Download),
        ];
        absorb(
            &mut files,
            vec![
                file("mods/a.jar", Origin::Override),
                file("c.txt", Origin::Override),
            ],
        );
        assert_eq!(files.len(), 3);
        assert_eq!(files[0].path, "mods/a.jar");
        assert_eq!(files[0].origin, Origin::Override);
    }
}
