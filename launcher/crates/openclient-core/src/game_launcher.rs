//! Motor real de lanzamiento de Minecraft Java (Vanilla + Fabric).
//!
//! Flujo de `launch_game`:
//!   1. Java compatible (verifica / descarga) -> `java_runtime`
//!   2. Manifiesto de Mojang + JSON de la versión (con caché local para jugar offline)
//!   3. Perfil de Fabric Loader (si la instancia es Fabric)
//!   4. Librerías, natives, client.jar y assets (descarga paralela, SHA-1 verificado)
//!   5. Mods de Modrinth (con dependencias requeridas, p. ej. Fabric API)
//!   6. Ejecuta Java con las flags de optimización y la cuenta del usuario
//!
//! Eventos hacia el frontend:
//!   - `launch-progress` { stage, percent, message }
//!   - `game-exited`     { instance_id, exit_code, log_path }

use crate::cancel::{guard, CancelToken, CANCELLED};
use crate::GameContext;
use futures_util::{stream, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha1::{Digest, Sha1};
use std::collections::{HashMap, HashSet};
use std::path::{Component, Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use crate::performance::{build_god_jvm_flags, GodOptimizationConfig};

const LAUNCHER_NAME: &str = "OpenClient";

#[cfg(target_os = "windows")]
const CP_SEP: &str = ";";
#[cfg(not(target_os = "windows"))]
const CP_SEP: &str = ":";

// Tipos públicos

#[derive(Debug, Deserialize, Clone)]
pub struct LaunchAccount {
    pub username: String,
    pub uuid: String,
    /// "microsoft" | "offline"
    pub account_type: String,
    pub access_token: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct LaunchMod {
    #[serde(default)]
    pub version_id: Option<String>,
    pub id: String,
    pub source: String,
    pub enabled: bool,
    #[serde(default)]
    pub content_type: String,
    #[serde(default)]
    pub download_url: Option<String>,
    #[serde(default)]
    pub file_name: Option<String>,
    #[serde(default)]
    pub sha1: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct LaunchRequest {
    pub instance_id: String,
    pub mc_version: String,
    /// "vanilla" | "fabric" | "quilt" | "forge" | "neoforge"
    pub loader: String,
    /// Versión fijada del cargador (fabric, quilt, forge y neoforge); `None` usa la recomendada.
    #[serde(default, alias = "fabric_version")]
    pub loader_version: Option<String>,
    pub account: LaunchAccount,
    pub optimization: GodOptimizationConfig,
    #[serde(default)]
    pub mods: Vec<LaunchMod>,
    #[serde(default)]
    pub java_override: Option<String>,
    #[serde(default)]
    pub hooks: LaunchHooks,
    /// Servidor validado con `server_address`; vacío si no se une a ninguno.
    #[serde(default)]
    pub join_server: String,
}

/// Normaliza `host` o `host:puerto`. Devuelve una cadena vacía si no hay servidor.
pub fn server_address(text: &str) -> Result<String, String> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(String::new());
    }
    let invalid = || "Escribe el servidor como dominio o IP, con «:puerto» opcional.".to_string();
    let (host, port) = match text.rsplit_once(':') {
        Some((host, port)) if !host.contains(':') => (host, Some(port)),
        _ => (text, None),
    };
    if host.is_empty()
        || host.len() > 253
        || !host.bytes().all(|byte| byte.is_ascii_alphanumeric() || b".-_".contains(&byte))
        || host.starts_with(['.', '-'])
    {
        return Err(invalid());
    }
    match port {
        None => Ok(host.to_owned()),
        Some(port) => match port.parse::<u16>() {
            Ok(number) if number > 0 && port.len() <= 5 => Ok(format!("{host}:{number}")),
            _ => Err(invalid()),
        },
    }
}

/// Comandos ya validados; una cadena vacía significa que no hay comando.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct LaunchHooks {
    pub pre_launch: String,
    pub post_exit: String,
    pub wrapper: String,
}

#[derive(Debug, Serialize)]
pub struct LaunchResult {
    pub pid: u32,
    pub game_dir: String,
    pub log_path: String,
    pub java_path: String,
    pub warnings: Vec<String>,
}

#[derive(Debug, Serialize, Clone)]
struct LaunchProgress {
    stage: String,
    percent: u8,
    message: String,
}

#[derive(Debug, Serialize, Clone)]
struct GameExited {
    instance_id: String,
    exit_code: Option<i32>,
    log_path: String,
    elapsed_seconds: u64,
}

// Utilidades

fn emit(app: &GameContext, stage: &str, percent: u8, message: impl Into<String>) {
    let _ = app.emit(
        "launch-progress",
        LaunchProgress {
            stage: stage.to_string(),
            percent,
            message: message.into(),
        },
    );
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

fn sanitize_component(s: &str) -> String {
    let r: String = s
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let r = r.trim_matches('.').to_string();
    if r.is_empty() {
        "default".to_string()
    } else {
        r
    }
}

/// Une una ruta relativa proveniente de un JSON externo, bloqueando `..` y rutas absolutas.
fn safe_join(base: &Path, rel: &str) -> Option<PathBuf> {
    let rel_path = Path::new(rel);
    if rel_path.is_absolute()
        || rel_path.components().any(|c| {
            matches!(
                c,
                Component::ParentDir | Component::Prefix(_) | Component::RootDir
            )
        })
    {
        return None;
    }
    Some(base.join(rel_path))
}

fn valid_project_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

fn required_dependency(dependency: &Value) -> Result<Option<String>, String> {
    if dependency["dependency_type"] != "required" {
        return Ok(None);
    }
    if let Some(id) = dependency["version_id"]
        .as_str()
        .filter(|id| valid_project_id(id))
    {
        return Ok(Some(format!("version:{}", id)));
    }
    if let Some(id) = dependency["project_id"]
        .as_str()
        .filter(|id| valid_project_id(id))
    {
        return Ok(Some(id.into()));
    }
    Err("Un mod tiene una dependencia obligatoria sin identificador válido; se conserva la instalación anterior.".into())
}

fn build_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .https_only(true)
        .connect_timeout(Duration::from_secs(20))
        .timeout(Duration::from_secs(300))
        .user_agent("OpenClient/2.0")
        .build()
        .map_err(|e| format!("No se pudo iniciar el cliente HTTP: {}", e))
}

async fn fetch_json(client: &reqwest::Client, url: &str) -> Result<Value, String> {
    let mut last = String::new();
    for attempt in 0..3u64 {
        match client.get(url).send().await {
            Ok(resp) => match resp.error_for_status() {
                Ok(resp) => match resp.json::<Value>().await {
                    Ok(v) => return Ok(v),
                    Err(e) => last = e.to_string(),
                },
                Err(e) => last = e.to_string(),
            },
            Err(e) => last = e.to_string(),
        }
        tokio::time::sleep(Duration::from_millis(400 * (attempt + 1))).await;
    }
    Err(format!("No se pudo obtener {}: {}", url, last))
}

/// Descarga un JSON y lo guarda en caché; si no hay red, usa la copia local.
async fn fetch_json_cached(
    client: &reqwest::Client,
    url: &str,
    cache: &Path,
) -> Result<Value, String> {
    match fetch_json(client, url).await {
        Ok(v) => {
            if let Some(parent) = cache.parent() {
                let _ = tokio::fs::create_dir_all(parent).await;
            }
            let _ = tokio::fs::write(cache, v.to_string()).await;
            Ok(v)
        }
        Err(e) => {
            if let Ok(text) = tokio::fs::read_to_string(cache).await {
                if let Ok(v) = serde_json::from_str::<Value>(&text) {
                    return Ok(v);
                }
            }
            Err(e)
        }
    }
}

#[derive(Clone)]
struct DlTask {
    url: String,
    dest: PathBuf,
    sha1: Option<String>,
    size: Option<u64>,
}

// Use Mojang's patched configuration also on versions whose Log4j predates
// formatMsgNoLookups. Missing/malformed metadata must not silently bypass it.
fn logging_download(version: &Value, assets: &Path) -> Result<Option<(DlTask, String)>, String> {
    let logging = match version.pointer("/logging/client") {
        Some(value) => value,
        None => {
            let uses_log4j = version["libraries"]
                .as_array()
                .map(|libs| {
                    libs.iter().any(|lib| {
                        lib["name"]
                            .as_str()
                            .unwrap_or("")
                            .starts_with("org.apache.logging.log4j:log4j-core:")
                    })
                })
                .unwrap_or(false);
            return if uses_log4j {
                Err("Falta la configuración de seguridad de Log4j. Conéctate para actualizar los metadatos de Minecraft antes de jugar.".into())
            } else {
                Ok(None)
            };
        }
    };
    let error =
        || "Configuración de seguridad de Log4j inválida; se ha cancelado el inicio.".to_string();
    let id = logging["file"]["id"].as_str().ok_or_else(error)?;
    if id.is_empty()
        || !id.ends_with(".xml")
        || !id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"-_.".contains(&c))
    {
        return Err(error());
    }
    let url = logging["file"]["url"].as_str().ok_or_else(error)?;
    let parsed = reqwest::Url::parse(url).map_err(|_| error())?;
    if parsed.scheme() != "https"
        || !matches!(
            parsed.host_str(),
            Some("launcher.mojang.com" | "piston-data.mojang.com")
        )
    {
        return Err(error());
    }
    let hash = logging["file"]["sha1"]
        .as_str()
        .filter(|s| s.len() == 40 && s.bytes().all(|c| c.is_ascii_hexdigit()))
        .ok_or_else(error)?;
    let size = logging["file"]["size"]
        .as_u64()
        .filter(|s| *s > 0 && *s <= 1_048_576)
        .ok_or_else(error)?;
    if logging["argument"].as_str() != Some("-Dlog4j.configurationFile=${path}") {
        return Err(error());
    }
    let dest = assets.join("log_configs").join(id);
    let argument = format!("-Dlog4j.configurationFile={}", dest.display());
    Ok(Some((
        DlTask {
            url: url.into(),
            dest,
            sha1: Some(hash.into()),
            size: Some(size),
        },
        argument,
    )))
}

/// Borra el `.part` si la descarga termina sin publicarlo (error, cancelación o future abandonado).
struct PartFile {
    path: PathBuf,
    armed: bool,
}

impl Drop for PartFile {
    fn drop(&mut self) {
        if self.armed {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

async fn try_download(
    client: &reqwest::Client,
    t: &DlTask,
    cancel: &CancelToken,
) -> Result<(), String> {
    use std::io::Write;
    cancel.check()?;
    let response = client
        .get(&t.url)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?;

    let mut tmp = t.dest.clone().into_os_string();
    tmp.push(".part");
    let tmp = PathBuf::from(tmp);
    // `part` se declara antes que `file`: el archivo se cierra antes de borrarlo.
    let mut part = PartFile {
        path: tmp.clone(),
        armed: true,
    };
    let mut file = std::fs::File::create(&tmp)
        .map_err(|e| format!("Error escribiendo {}: {}", tmp.display(), e))?;
    let mut hasher = t.sha1.as_ref().map(|_| Sha1::new());
    let mut received = 0u64;
    let mut body = response.bytes_stream();
    while let Some(chunk) = body.next().await {
        cancel.check()?;
        let chunk = chunk.map_err(|e| e.to_string())?;
        received += chunk.len() as u64;
        if t.size.is_some_and(|size| received > size) {
            return Err(format!("Tamaño inválido en {}", t.url));
        }
        if let Some(hasher) = hasher.as_mut() {
            hasher.update(&chunk);
        }
        file.write_all(&chunk)
            .map_err(|e| format!("Error escribiendo {}: {}", tmp.display(), e))?;
    }
    cancel.check()?;
    file.flush()
        .map_err(|e| format!("Error escribiendo {}: {}", tmp.display(), e))?;
    drop(file);

    if t.size.is_some_and(|size| size != received) {
        return Err(format!("Tamaño inválido en {}", t.url));
    }
    if let (Some(hasher), Some(expected)) = (hasher, &t.sha1) {
        if !hex(&hasher.finalize()).eq_ignore_ascii_case(expected) {
            return Err(format!("SHA-1 inválido en {}", t.url));
        }
    }

    tokio::fs::rename(&tmp, &t.dest)
        .await
        .map_err(|e| format!("Error moviendo {}: {}", t.dest.display(), e))?;
    part.armed = false;
    Ok(())
}

async fn download(
    client: &reqwest::Client,
    t: &DlTask,
    cancel: &CancelToken,
) -> Result<(), String> {
    cancel.check()?;
    if let Ok(meta) = tokio::fs::metadata(&t.dest).await {
        if meta.len() > 0 && t.size.map(|s| s == meta.len()).unwrap_or(true) {
            if let Some(expected) = &t.sha1 {
                let bytes = tokio::fs::read(&t.dest).await.map_err(|e| e.to_string())?;
                if hex(&Sha1::digest(&bytes)).eq_ignore_ascii_case(expected) {
                    return Ok(());
                }
            } else {
                return Ok(());
            }
        }
    }
    if let Some(parent) = t.dest.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| format!("No se pudo crear {}: {}", parent.display(), e))?;
    }

    let mut last = String::new();
    for attempt in 0..3u64 {
        cancel.check()?;
        match try_download(client, t, cancel).await {
            Ok(()) => return Ok(()),
            Err(e) if e == CANCELLED => return Err(e),
            Err(e) => last = e,
        }
        guard(cancel, async {
            tokio::time::sleep(Duration::from_millis(500 * (attempt + 1))).await;
            Ok(())
        })
        .await?;
    }
    Err(format!("Falló la descarga de {}: {}", t.url, last))
}

async fn run_downloads(
    app: &GameContext,
    client: &reqwest::Client,
    tasks: Vec<DlTask>,
    stage: &str,
    from: u8,
    to: u8,
    label: &str,
) -> Result<(), String> {
    app.cancel.check()?;
    let total = tasks.len();
    if total == 0 {
        return Ok(());
    }

    let mut done = 0usize;
    let cancel = &app.cancel;
    let mut s = stream::iter(tasks.into_iter().map(|t| {
        let c = client.clone();
        async move { guard(cancel, download(&c, &t, cancel)).await }
    }))
    .buffer_unordered(crate::download_concurrency());

    // Al recibir el primer error (también la cancelación) se suelta el stream
    // y con él las descargas pendientes; cada `.part` se borra en su `Drop`.
    while let Some(res) = s.next().await {
        res?;
        done += 1;
        if done.is_multiple_of(10) || done == total {
            let pct = from as usize + (to - from) as usize * done / total;
            emit(
                app,
                stage,
                pct as u8,
                format!("{} ({}/{})", label, done, total),
            );
        }
    }
    Ok(())
}

// Reglas y argumentos

fn rule_os_matches(os: &Value) -> bool {
    if let Some(name) = os.get("name").and_then(|v| v.as_str()) {
        if name != "windows" {
            return false;
        }
    }
    if let Some(arch) = os.get("arch").and_then(|v| v.as_str()) {
        if arch != "x86_64" && arch != "amd64" {
            return false;
        }
    }
    os.get("version").is_none()
}

fn rules_allow(rules: Option<&Value>) -> bool {
    let arr = match rules.and_then(|r| r.as_array()) {
        Some(a) if !a.is_empty() => a,
        _ => return true,
    };

    let mut allowed = false;
    for rule in arr {
        let action = rule
            .get("action")
            .and_then(|a| a.as_str())
            .unwrap_or("allow");
        let applies = if rule.get("features").is_some() {
            false
        } else {
            match rule.get("os") {
                Some(os) => rule_os_matches(os),
                None => true,
            }
        };
        if applies {
            allowed = action == "allow";
        }
    }
    allowed
}

fn subst(s: &str, vars: &HashMap<&str, String>) -> String {
    let mut r = s.to_string();
    for (k, v) in vars {
        r = r.replace(&format!("${{{}}}", k), v);
    }
    r
}

fn collect_args(arr: Option<&Value>, vars: &HashMap<&str, String>) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(items) = arr.and_then(|v| v.as_array()) {
        for item in items {
            match item {
                Value::String(s) => out.push(subst(s, vars)),
                Value::Object(_) => {
                    if !rules_allow(item.get("rules")) {
                        continue;
                    }
                    match item.get("value") {
                        Some(Value::String(s)) => out.push(subst(s, vars)),
                        Some(Value::Array(vs)) => {
                            for v in vs {
                                if let Some(s) = v.as_str() {
                                    out.push(subst(s, vars));
                                }
                            }
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }
    }
    out
}

// Librerías

fn maven_path(name: &str) -> Option<String> {
    let (coords, ext) = match name.split_once('@') {
        Some((c, e)) => (c, e),
        None => (name, "jar"),
    };
    let parts: Vec<&str> = coords.split(':').collect();
    if parts.len() < 3 {
        return None;
    }
    let group = parts[0].replace('.', "/");
    let (artifact, version) = (parts[1], parts[2]);
    let file = if parts.len() >= 4 {
        format!("{}-{}-{}.{}", artifact, version, parts[3], ext)
    } else {
        format!("{}-{}.{}", artifact, version, ext)
    };
    Some(format!("{}/{}/{}/{}", group, artifact, version, file))
}

fn library_key(name: &str) -> String {
    let coords = name.split('@').next().unwrap_or(name);
    let parts: Vec<&str> = coords.split(':').collect();
    if parts.len() >= 4 {
        format!("{}:{}:{}", parts[0], parts[1], parts[3])
    } else if parts.len() >= 2 {
        format!("{}:{}", parts[0], parts[1])
    } else {
        coords.to_string()
    }
}

fn artifact_task(art: &Value, libs_dir: &Path) -> Option<(DlTask, PathBuf)> {
    let path = art.get("path")?.as_str()?;
    let url = art.get("url")?.as_str()?;
    if url.is_empty() {
        return None;
    }
    let dest = safe_join(libs_dir, path)?;
    Some((
        DlTask {
            url: url.to_string(),
            dest: dest.clone(),
            sha1: art.get("sha1").and_then(|s| s.as_str()).map(String::from),
            size: art.get("size").and_then(|s| s.as_u64()),
        },
        dest,
    ))
}

fn collect_libraries(
    libs: Option<&Value>,
    libs_dir: &Path,
    tasks: &mut Vec<DlTask>,
    classpath: &mut Vec<PathBuf>,
    native_jars: &mut Vec<PathBuf>,
    seen: &mut HashSet<String>,
) {
    let arr = match libs.and_then(|l| l.as_array()) {
        Some(a) => a,
        None => return,
    };

    for lib in arr {
        if !rules_allow(lib.get("rules")) {
            continue;
        }
        let name = lib.get("name").and_then(|n| n.as_str()).unwrap_or("");

        // Natives del formato antiguo (< 1.19): se extraen a la carpeta natives.
        if let Some(classifier) = lib
            .get("natives")
            .and_then(|n| n.get("windows"))
            .and_then(|c| c.as_str())
        {
            let classifier = classifier.replace("${arch}", "64");
            if let Some(art) = lib.pointer(&format!("/downloads/classifiers/{}", classifier)) {
                if let Some((task, dest)) = artifact_task(art, libs_dir) {
                    tasks.push(task);
                    native_jars.push(dest);
                }
            }
        }

        let key = library_key(name);
        if !name.is_empty() && !seen.insert(key) {
            continue;
        }

        if let Some(art) = lib.pointer("/downloads/artifact") {
            if let Some((task, dest)) = artifact_task(art, libs_dir) {
                tasks.push(task);
                classpath.push(dest);
            }
        } else if let (Some(base), false) =
            (lib.get("url").and_then(|u| u.as_str()), name.is_empty())
        {
            // Estilo Maven (Fabric): coordenadas + URL base.
            if let Some(rel) = maven_path(name) {
                if let Some(dest) = safe_join(libs_dir, &rel) {
                    let base = if base.ends_with('/') {
                        base.to_string()
                    } else {
                        format!("{}/", base)
                    };
                    tasks.push(DlTask {
                        url: format!("{}{}", base, rel),
                        dest: dest.clone(),
                        sha1: lib.get("sha1").and_then(|s| s.as_str()).map(String::from),
                        size: lib.get("size").and_then(|s| s.as_u64()),
                    });
                    classpath.push(dest);
                }
            }
        }
    }
}

fn extract_natives(jars: &[PathBuf], dest: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dest).map_err(|e| format!("No se pudo crear natives: {}", e))?;
    for jar in jars {
        let file = std::fs::File::open(jar).map_err(|e| e.to_string())?;
        let mut archive = zip::ZipArchive::new(file)
            .map_err(|e| format!("Natives inválidos ({}): {}", jar.display(), e))?;
        for i in 0..archive.len() {
            let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
            if entry.is_dir() {
                continue;
            }
            let name = entry.name().map_err(|e| e.to_string())?.into_owned();
            if name.starts_with("META-INF") {
                continue;
            }
            let lower = name.to_lowercase();
            if !(lower.ends_with(".dll") || lower.ends_with(".so") || lower.ends_with(".dylib")) {
                continue;
            }
            let file_name = match Path::new(&name).file_name() {
                Some(f) => f.to_owned(),
                None => continue,
            };
            let mut out = std::fs::File::create(dest.join(file_name)).map_err(|e| e.to_string())?;
            std::io::copy(&mut entry, &mut out).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

// Mojang / Fabric

async fn load_vanilla_json(
    client: &reqwest::Client,
    mc: &str,
    cache: &Path,
) -> Result<Value, String> {
    if let Ok(manifest) = fetch_json(
        client,
        "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json",
    )
    .await
    {
        let url = manifest["versions"]
            .as_array()
            .and_then(|a| a.iter().find(|v| v["id"].as_str() == Some(mc)))
            .and_then(|v| v["url"].as_str())
            .map(String::from)
            .ok_or_else(|| format!("La versión {} no existe en Mojang.", mc))?;
        return fetch_json_cached(client, &url, cache).await;
    }

    // Sin red: usar la copia local si existe.
    let text = tokio::fs::read_to_string(cache).await.map_err(|_| {
        "No hay conexión con Mojang y esta versión aún no está descargada.".to_string()
    })?;
    serde_json::from_str::<Value>(&text).map_err(|e| format!("Caché de versión dañada: {}", e))
}

fn loader_display(loader: &str) -> &'static str {
    match loader {
        "fabric" => "Fabric",
        "quilt" => "Quilt",
        "forge" => "Forge",
        "neoforge" => "NeoForge",
        _ => "el cargador",
    }
}

/// Valida la versión fijada de un cargador antes de usarla en una URL o en una ruta.
fn pinned_loader_version<'a>(
    loader: &str,
    pinned: Option<&'a str>,
) -> Result<Option<&'a str>, String> {
    match pinned {
        Some(version) if !crate::modpack::fabric_version_valid(version) => {
            Err(format!("Versión de {} inválida.", loader_display(loader)))
        }
        other => Ok(other),
    }
}

fn quilt_profile_url(mc: &str, version: &str) -> String {
    format!("https://meta.quiltmc.org/v3/versions/loader/{mc}/{version}/profile/json")
}

fn forge_installer_url_pinned(mc: &str, version: &str) -> String {
    format!("https://maven.minecraftforge.net/net/minecraftforge/forge/{mc}-{version}/forge-{mc}-{version}-installer.jar")
}

fn neoforge_installer_url_pinned(version: &str) -> String {
    format!("https://maven.neoforged.net/releases/net/neoforged/neoforge/{version}/neoforge-{version}-installer.jar")
}

/// Nombre del perfil en caché de Forge/NeoForge; con versión fijada la incluye.
fn forge_cache_name(loader: &str, mc: &str, pinned: Option<&str>) -> String {
    match pinned {
        Some(version) => format!("{loader}-{mc}-{version}.json"),
        None => format!("{loader}-{mc}.json"),
    }
}

async fn load_fabric_profile(
    client: &reqwest::Client,
    mc: &str,
    cache: &Path,
    pinned: Option<&str>,
) -> Result<Value, String> {
    if let Some(version) = pinned_loader_version("fabric", pinned)? {
        let url =
            format!("https://meta.fabricmc.net/v2/versions/loader/{mc}/{version}/profile/json");
        return fetch_json_cached(
            client,
            &url,
            &cache.with_file_name(format!("fabric-loader-{version}.json")),
        )
        .await;
    }
    let list_url = format!("https://meta.fabricmc.net/v2/versions/loader/{}", mc);
    match fetch_json(client, &list_url).await {
        Ok(list) => {
            let arr = list.as_array().cloned().unwrap_or_default();
            let chosen = arr
                .iter()
                .find(|v| v["loader"]["stable"].as_bool() == Some(true))
                .or_else(|| arr.first())
                .and_then(|v| v["loader"]["version"].as_str())
                .map(String::from)
                .ok_or_else(|| format!("Fabric no está disponible para Minecraft {}.", mc))?;

            let profile_url = format!(
                "https://meta.fabricmc.net/v2/versions/loader/{}/{}/profile/json",
                mc, chosen
            );
            fetch_json_cached(client, &profile_url, cache).await
        }
        Err(e) => {
            if let Ok(text) = tokio::fs::read_to_string(cache).await {
                if let Ok(v) = serde_json::from_str::<Value>(&text) {
                    return Ok(v);
                }
            }
            Err(e)
        }
    }
}

async fn load_quilt_profile(
    client: &reqwest::Client,
    mc: &str,
    cache: &Path,
    pinned: Option<&str>,
) -> Result<Value, String> {
    if let Some(version) = pinned_loader_version("quilt", pinned)? {
        return fetch_json_cached(
            client,
            &quilt_profile_url(mc, version),
            &cache.with_file_name(format!("quilt-loader-{version}.json")),
        )
        .await;
    }
    let list_url = format!("https://meta.quiltmc.org/v3/versions/loader/{}", mc);
    match fetch_json(client, &list_url).await {
        Ok(list) => {
            let arr = list.as_array().cloned().unwrap_or_default();
            let chosen = arr
                .first()
                .and_then(|v| v["loader"]["version"].as_str())
                .map(String::from)
                .ok_or_else(|| format!("Quilt no está disponible para Minecraft {}.", mc))?;
            fetch_json_cached(client, &quilt_profile_url(mc, &chosen), cache).await
        }
        Err(e) => {
            if let Ok(text) = tokio::fs::read_to_string(cache).await {
                if let Ok(v) = serde_json::from_str::<Value>(&text) {
                    return Ok(v);
                }
            }
            Err(e)
        }
    }
}

/// URL del instalador de NeoForge según la lista de versiones de su Maven.
fn neoforge_installer_url(mc: &str, meta: &Value) -> Result<String, String> {
    let versions = meta["versions"]
        .as_array()
        .ok_or("No se pudo consultar la lista de versiones de NeoForge.")?;
    let prefix_dot = crate::versions::neoforge_prefix(mc);
    let chosen = versions
        .iter()
        .rev()
        .filter_map(|v| v.as_str())
        .find(|v| v.starts_with(&prefix_dot) && !v.contains("beta"))
        .or_else(|| {
            versions
                .iter()
                .rev()
                .filter_map(|v| v.as_str())
                .find(|v| v.starts_with(&prefix_dot))
        })
        .ok_or_else(|| format!("NeoForge no está disponible para Minecraft {mc}."))?;
    Ok(format!("https://maven.neoforged.net/releases/net/neoforged/neoforge/{chosen}/neoforge-{chosen}-installer.jar"))
}

/// URL del instalador de Forge según `promotions_slim.json`.
fn forge_installer_url(mc: &str, promos: &Value) -> Result<String, String> {
    let map = promos["promos"]
        .as_object()
        .ok_or("No se pudo leer el manifiesto de Forge.")?;
    let ver = map
        .get(&format!("{mc}-recommended"))
        .or_else(|| map.get(&format!("{mc}-latest")))
        .and_then(|v| v.as_str())
        .ok_or_else(|| format!("Forge no está disponible para Minecraft {mc}."))?;
    Ok(format!("https://maven.minecraftforge.net/net/minecraftforge/forge/{mc}-{ver}/forge-{mc}-{ver}-installer.jar"))
}

/// Perfil que el instalador dejó en `versions/<nombre>/<nombre>.json`; lo guarda en caché.
async fn read_installed_profile(
    root: &Path,
    loader: &str,
    mc: &str,
    pinned: Option<&str>,
    cache: &Path,
) -> Option<Value> {
    let mut entries = tokio::fs::read_dir(root.join("versions")).await.ok()?;
    while let Ok(Some(entry)) = entries.next_entry().await {
        let name = entry.file_name().to_string_lossy().to_lowercase();
        if name.contains(loader)
            && name.contains(&mc.to_lowercase())
            && pinned.is_none_or(|version| name.contains(&version.to_lowercase()))
        {
            let candidate = entry.path().join(format!(
                "{}.json",
                entry.file_name().to_string_lossy()
            ));
            if let Ok(text) = tokio::fs::read_to_string(&candidate).await {
                if let Ok(v) = serde_json::from_str::<Value>(&text) {
                    let _ = tokio::fs::write(cache, &text).await;
                    return Some(v);
                }
            }
        }
    }
    None
}

/// Lee `version.json` dentro del instalador de Forge/NeoForge.
fn installer_version_json(installer: &Path) -> Result<Value, String> {
    let file = std::fs::File::open(installer).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    let mut ver_entry = archive
        .by_name("version.json")
        .map_err(|_| "El instalador del cargador no contiene version.json.".to_string())?;
    let mut text = String::new();
    std::io::Read::read_to_string(&mut ver_entry, &mut text).map_err(|e| e.to_string())?;
    serde_json::from_str::<Value>(&text).map_err(|e| e.to_string())
}

#[allow(clippy::too_many_arguments)]
async fn load_forge_profile(
    client: &reqwest::Client,
    root: &Path,
    java_path: &Path,
    mc: &str,
    loader: &str,
    cache: &Path,
    pinned: Option<&str>,
    cancel: &CancelToken,
) -> Result<Value, String> {
    let pinned = pinned_loader_version(loader, pinned)?;
    if let Ok(text) = tokio::fs::read_to_string(cache).await {
        if let Ok(v) = serde_json::from_str::<Value>(&text) {
            return Ok(v);
        }
    }
    cancel.check()?;
    let installer_url = match (pinned, loader) {
        (Some(version), "neoforge") => neoforge_installer_url_pinned(version),
        (Some(version), _) => forge_installer_url_pinned(mc, version),
        (None, "neoforge") => {
            let meta = guard(
                cancel,
                fetch_json(
                    client,
                    "https://maven.neoforged.net/api/maven/versions/releases/net/neoforged/neoforge",
                ),
            )
            .await?;
            neoforge_installer_url(mc, &meta)?
        }
        (None, _) => {
            let promos = guard(
                cancel,
                fetch_json(
                    client,
                    "https://files.minecraftforge.net/net/minecraftforge/forge/promotions_slim.json",
                ),
            )
            .await?;
            forge_installer_url(mc, &promos)?
        }
    };

    let installer_dest = match pinned {
        Some(version) => cache.with_file_name(format!("{loader}-{mc}-{version}-installer.jar")),
        None => cache.with_file_name(format!("{loader}-{mc}-installer.jar")),
    };
    download(
        client,
        &DlTask {
            url: installer_url,
            dest: installer_dest.clone(),
            sha1: None,
            size: None,
        },
        cancel,
    )
    .await?;
    // El instalador, una vez en marcha, no se interrumpe: se cancela antes de lanzarlo.
    cancel.check()?;

    let profiles_file = root.join("launcher_profiles.json");
    if !profiles_file.exists() {
        let _ = tokio::fs::write(&profiles_file, r#"{"profiles":{}}"#).await;
    }

    let java = java_path.to_owned();
    let inst = installer_dest.clone();
    let root_dir = root.to_owned();
    let _ = tokio::task::spawn_blocking(move || {
        let mut cmd = std::process::Command::new(&java);
        cmd.arg("-jar")
            .arg(&inst)
            .arg("--installClient")
            .arg(&root_dir)
            .current_dir(&root_dir)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x08000000);
        }
        cmd.status()
    })
    .await;

    // Look for installed profile in versions/ or extract version.json from the installer archive.
    if let Some(profile) = read_installed_profile(root, loader, mc, pinned, cache).await {
        return Ok(profile);
    }

    let inst_path = installer_dest;
    let extracted = tokio::task::spawn_blocking(move || installer_version_json(&inst_path))
        .await
        .map_err(|e| e.to_string())??;
    let _ = tokio::fs::write(cache, extracted.to_string()).await;
    Ok(extracted)
}

// Mods (Modrinth)

#[derive(Clone)]
struct ModFile {
    filename: String,
    url: String,
    sha1: Option<String>,
    size: Option<u64>,
}

async fn sync_mods(
    app: &GameContext,
    client: &reqwest::Client,
    mods_dir: &Path,
    mc: &str,
    loader: &str,
    mods: &[LaunchMod],
    warnings: &mut Vec<String>,
) -> Result<(), String> {
    static MOD_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
    let _guard = MOD_LOCK.lock().await;
    app.cancel.check()?;
    crate::directory_install::recover(mods_dir)?;
    tokio::fs::create_dir_all(mods_dir)
        .await
        .map_err(|e| format!("No se pudo crear la carpeta mods: {}", e))?;

    let is_mod_type = |m: &LaunchMod| m.content_type.is_empty() || m.content_type == "mod";
    let mut direct_files: Vec<ModFile> = Vec::new();

    for m in mods.iter().filter(|m| m.enabled && is_mod_type(m)) {
        if m.source.eq_ignore_ascii_case("curseforge") {
            if let (Some(url), Some(raw_name)) = (&m.download_url, &m.file_name) {
                let filename = Path::new(raw_name)
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                let valid_host = reqwest::Url::parse(url).is_ok_and(|u| {
                    u.scheme() == "https"
                        && matches!(
                            u.host_str(),
                            Some("edge.forgecdn.net" | "mediafilez.forgecdn.net")
                        )
                });
                if filename.to_lowercase().ends_with(".jar") && valid_host {
                    direct_files.push(ModFile {
                        filename,
                        url: url.clone(),
                        sha1: m.sha1.clone().filter(|s| s.len() == 40),
                        size: None,
                    });
                    continue;
                }
            }
        }
        if !m.source.eq_ignore_ascii_case("modrinth") {
            warnings.push(format!(
                "'{}' es de {} y no se descarga automáticamente; colócalo en la carpeta mods.",
                m.id, m.source
            ));
        }
    }

    let mut queue: Vec<(String, u8)> = mods
        .iter()
        .filter(|m| {
            m.enabled
                && is_mod_type(m)
                && m.source.eq_ignore_ascii_case("modrinth")
                && valid_project_id(&m.id)
        })
        .map(|m| {
            (
                m.version_id
                    .as_ref()
                    .map(|id| format!("version:{}", id))
                    .unwrap_or_else(|| m.id.clone()),
                0u8,
            )
        })
        .collect();

    let lock_path = if mods_dir.join(".alba_versions.json").exists() {
        mods_dir.join(".alba_versions.json")
    } else {
        mods_dir.join(".openclient_versions.json")
    };
    let mut resolved: HashMap<String, Value> = tokio::fs::read_to_string(&lock_path)
        .await
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    let mut visited: HashSet<String> = HashSet::new();
    let mut files: Vec<ModFile> = direct_files;
    let mut selected_projects: HashMap<String, String> = HashMap::new();
    let mut had_errors = false;
    let mut i = 0usize;

    while i < queue.len() {
        app.cancel.check()?;
        if queue.len() > 256 {
            return Err("Demasiadas dependencias; revisa la selección de mods.".into());
        }
        let (pid, depth) = queue[i].clone();
        i += 1;
        if !visited.insert(pid.clone()) {
            continue;
        }

        emit(app, "mods", 80, format!("Resolviendo mod '{}'...", pid));

        let pinned = pid.strip_prefix("version:");
        let url = match pinned {
            Some(id) => format!("https://api.modrinth.com/v2/version/{}", id),
            None => format!("https://api.modrinth.com/v2/project/{}/version", pid),
        };
        let resp = if resolved.contains_key(&pid) {
            None
        } else {
            Some(
                guard(
                    &app.cancel,
                    async {
                        Ok(client
                            .get(&url)
                            .query(&[
                                ("game_versions", format!("[\"{}\"]", mc)),
                                ("loaders", format!("[\"{}\"]", loader)),
                            ])
                            .send()
                            .await)
                    },
                )
                .await?,
            )
        };

        let versions: Vec<Value> = if let Some(saved) = resolved.get(&pid) {
            vec![saved.clone()]
        } else {
            match resp.ok_or("No hay versión resuelta")? {
                Ok(r) => match r.error_for_status() {
                    Ok(r) => {
                        let value = r
                            .json::<Value>()
                            .await
                            .map_err(|e| format!("Respuesta de mods inválida: {}", e))?;
                        if pinned.is_some() {
                            vec![value]
                        } else {
                            value.as_array().cloned().ok_or("Lista de mods inválida")?
                        }
                    }
                    Err(_) => {
                        had_errors = true;
                        warnings.push(format!("No se encontró el mod '{}' en Modrinth.", pid));
                        continue;
                    }
                },
                Err(_) => {
                    had_errors = true;
                    continue;
                }
            }
        };

        let version = versions
            .iter()
            .find(|v| v["version_type"] == "release")
            .or_else(|| versions.first());

        let version = match version {
            Some(v) => v,
            None => {
                warnings.push(format!(
                    "'{}' no tiene versión compatible con Fabric {}.",
                    pid, mc
                ));
                return Err(format!("No existe una versión compatible para {}", pid));
            }
        };

        if !version["game_versions"]
            .as_array()
            .map(|a| a.iter().any(|v| v.as_str() == Some(mc)))
            .unwrap_or(false)
            || !version["loaders"]
                .as_array()
                .map(|a| {
                    a.iter().any(|v| {
                        v.as_str() == Some(loader)
                            || (loader == "quilt" && v.as_str() == Some("fabric"))
                    })
                })
                .unwrap_or(false)
        {
            return Err(format!(
                "La versión fijada de {} no es compatible con {} {}",
                pid, loader, mc
            ));
        }
        let project = version["project_id"]
            .as_str()
            .ok_or("Mod sin identificador de proyecto")?
            .to_string();
        let version_id = version["id"]
            .as_str()
            .ok_or("Mod sin identificador de versión")?
            .to_string();
        if let Some(existing) = selected_projects.insert(project.clone(), version_id.clone()) {
            if existing != version_id {
                return Err(format!(
                    "Dependencias incompatibles para {}. Revisa las versiones seleccionadas.",
                    project
                ));
            }
        }
        resolved.insert(pid.clone(), version.clone());
        let file = version["files"].as_array().and_then(|fs| {
            fs.iter()
                .find(|f| f["primary"].as_bool() == Some(true))
                .or_else(|| fs.first())
        });

        if let Some(f) = file {
            let raw_name = f["filename"].as_str().unwrap_or("");
            let filename = Path::new(raw_name)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            let furl = f["url"].as_str().unwrap_or("");
            if filename.to_lowercase().ends_with(".jar") && furl.starts_with("https://") {
                files.push(ModFile {
                    filename,
                    url: furl.to_string(),
                    sha1: f["hashes"]["sha1"].as_str().map(String::from),
                    size: f["size"].as_u64(),
                });
            }
        }

        {
            if let Some(deps) = version["dependencies"].as_array() {
                for d in deps {
                    if let Some(dep) = required_dependency(d)? {
                        queue.push((dep, depth + 1));
                    }
                }
            }
        }
    }

    if had_errors {
        return Err(
            "No se pudieron resolver todos los mods. Se conserva la instalación anterior.".into(),
        );
    }

    // Quitar duplicados por nombre de archivo.
    let mut names: HashSet<String> = HashSet::new();
    files.retain(|f| names.insert(f.filename.clone()));

    app.cancel.check()?;
    let staged = crate::directory_install::PreparedDirectory::new(mods_dir, true)?;
    let manifest_path = if staged.path.join(".alba_managed.json").exists() {
        staged.path.join(".alba_managed.json")
    } else {
        staged.path.join(".openclient_managed.json")
    };
    let old: Vec<String> = match tokio::fs::read_to_string(&manifest_path).await {
        Ok(text) => serde_json::from_str(&text).map_err(|_| {
            "La lista de mods guardada está dañada; se conserva la instalación anterior."
        })?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(error) => return Err(format!("No se pudo leer la lista de mods: {}", error)),
    };
    let mut tasks = Vec::new();
    for file in files {
        let dest = staged.path.join(&file.filename);
        if dest.exists() && !old.contains(&file.filename) {
            if let Some(expected) = &file.sha1 {
                let bytes = tokio::fs::read(&dest)
                    .await
                    .map_err(|error| error.to_string())?;
                if hex(&Sha1::digest(&bytes)).eq_ignore_ascii_case(expected) {
                    names.remove(&file.filename);
                    continue;
                }
            }
            return Err(format!(
                "El mod {} coincide con un archivo local distinto.",
                file.filename
            ));
        }
        tasks.push(DlTask {
            url: file.url,
            dest,
            sha1: file.sha1,
            size: file.size,
        });
    }
    run_downloads(app, client, tasks, "mods", 80, 88, "Descargando mods").await?;

    for old_name in old {
        if !names.contains(&old_name) {
            let path = safe_join(&staged.path, &old_name)
                .ok_or("La lista de mods contiene una ruta inválida")?;
            match tokio::fs::remove_file(path).await {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(format!("No se pudo retirar el mod anterior: {}", error)),
            }
        }
    }
    let list: Vec<&String> = names.iter().collect();
    tokio::fs::write(
        &manifest_path,
        serde_json::to_vec(&list).map_err(|e| e.to_string())?,
    )
    .await
    .map_err(|e| format!("No se pudo guardar la lista de mods: {}", e))?;

    tokio::fs::write(
        staged
            .path
            .join(lock_path.file_name().ok_or("Ruta de versiones inválida")?),
        serde_json::to_vec(&resolved).map_err(|e| e.to_string())?,
    )
    .await
    .map_err(|e| e.to_string())?;
    // Último punto de cancelación: a partir de `commit` la publicación ya no se interrumpe.
    app.cancel.check()?;
    staged.commit()?;
    Ok(())
}

fn pack_name(project: &str, version: &str) -> Option<String> {
    [project, version]
        .iter()
        .all(|id| {
            !id.is_empty() && id.len() <= 32 && id.bytes().all(|byte| byte.is_ascii_alphanumeric())
        })
        .then(|| format!("OpenClient-{project}-{version}.zip"))
}

fn managed_pack_name(name: &str) -> bool {
    name.strip_prefix("OpenClient-")
        .and_then(|rest| rest.strip_suffix(".zip"))
        .and_then(|ids| ids.split_once('-'))
        .is_some_and(|(project, version)| pack_name(project, version).is_some())
}

#[derive(Deserialize, Serialize)]
struct PackFile {
    url: String,
    sha1: String,
    size: u64,
}

impl PackFile {
    fn valid(&self) -> bool {
        reqwest::Url::parse(&self.url).is_ok_and(|url| {
            url.scheme() == "https"
                && url.host_str() == Some("cdn.modrinth.com")
                && url.port().is_none()
                && url.username().is_empty()
                && url.password().is_none()
        }) && self.sha1.len() == 40
            && self.sha1.bytes().all(|byte| byte.is_ascii_hexdigit())
            && (1..=128 * 1024 * 1024).contains(&self.size)
    }
}

async fn sync_packs(
    app: &GameContext,
    client: &reqwest::Client,
    game_dir: &Path,
    mc: &str,
    mods: &[LaunchMod],
) -> Result<(), String> {
    static PACK_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
    let _guard = PACK_LOCK.lock().await;
    const MANIFEST: &str = ".openclient-packs.json";

    for (kind, directory) in [("resourcepack", "resourcepacks"), ("shader", "shaderpacks")] {
        let folder = game_dir.join(directory);
        crate::directory_install::recover(&folder)?;
        let active: Vec<_> = mods
            .iter()
            .filter(|item| {
                item.enabled
                    && item.content_type == kind
                    && item.source.eq_ignore_ascii_case("modrinth")
            })
            .collect();
        if active.is_empty() && !folder.join(MANIFEST).exists() {
            continue;
        }
        app.cancel.check()?;
        let staged = crate::directory_install::PreparedDirectory::new(&folder, true)?;
        let manifest = staged.path.join(MANIFEST);
        let mut previous: HashMap<String, PackFile> = match tokio::fs::read_to_string(&manifest)
            .await
        {
            Ok(text) => {
                serde_json::from_str(&text).map_err(|_| "El registro de paquetes está dañado.")?
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => HashMap::new(),
            Err(error) => return Err(format!("No se pudo leer el registro de paquetes: {error}")),
        };
        let mut current = HashMap::new();
        let mut tasks = Vec::new();
        for item in active {
            let version = item
                .version_id
                .as_deref()
                .ok_or("El paquete no tiene una versión asignada.")?;
            let name = pack_name(&item.id, version).ok_or("Identificador de paquete inválido.")?;
            let dest = staged.path.join(&name);
            let file = if let Some(file) = previous.remove(&name).filter(PackFile::valid) {
                file
            } else {
                if dest.exists() {
                    return Err(format!("Ya existe un archivo local con el nombre {name}."));
                }
                let info = guard(
                    &app.cancel,
                    fetch_json(
                        client,
                        &format!("https://api.modrinth.com/v2/version/{version}"),
                    ),
                )
                .await?;
                if info["id"] != version
                    || info["project_id"] != item.id
                    || !info["game_versions"]
                        .as_array()
                        .is_some_and(|versions| versions.iter().any(|value| value == mc))
                    || (kind == "shader"
                        && !info["loaders"]
                            .as_array()
                            .is_some_and(|loaders| loaders.iter().any(|value| value == "iris")))
                {
                    return Err("La versión del paquete no es compatible con la instancia.".into());
                }
                let mut files = info["files"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|file| {
                        file["filename"]
                            .as_str()
                            .is_some_and(|name| name.to_ascii_lowercase().ends_with(".zip"))
                    });
                let file = files
                    .clone()
                    .find(|file| file["primary"] == true)
                    .or_else(|| files.next())
                    .ok_or("Modrinth no ofrece un ZIP para este paquete.")?;
                PackFile {
                    url: file["url"].as_str().unwrap_or_default().to_owned(),
                    sha1: file["hashes"]["sha1"]
                        .as_str()
                        .unwrap_or_default()
                        .to_owned(),
                    size: file["size"].as_u64().unwrap_or_default(),
                }
            };
            if !file.valid() {
                return Err("El paquete no cumple los requisitos de descarga.".into());
            }
            tasks.push(DlTask {
                url: file.url.clone(),
                dest,
                sha1: Some(file.sha1.clone()),
                size: Some(file.size),
            });
            current.insert(name, file);
        }
        run_downloads(
            app,
            client,
            tasks,
            "mods",
            88,
            91,
            "Descargando paquetes",
        )
        .await?;
        for name in previous.keys() {
            if !managed_pack_name(name) {
                return Err("El registro de paquetes contiene un nombre inválido.".into());
            }
            match tokio::fs::remove_file(staged.path.join(name)).await {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(format!("No se pudo retirar el paquete: {error}")),
            }
        }
        let data = serde_json::to_vec(&current).map_err(|error| error.to_string())?;
        tokio::fs::write(manifest, data)
            .await
            .map_err(|error| error.to_string())?;
        app.cancel.check()?;
        staged.commit()?;
    }
    Ok(())
}

pub async fn sync_instance_content(
    app: &GameContext,
    instance_id: &str,
    mc_version: &str,
    loader: &str,
    content: &[LaunchMod],
) -> Result<(), String> {
    let client = build_client()?;
    let game_dir = instance_game_dir(&app.root, instance_id)?;
    if matches!(loader, "fabric" | "quilt" | "forge" | "neoforge") {
        let mut warnings = Vec::new();
        sync_mods(
            app,
            &client,
            &game_dir.join("mods"),
            mc_version,
            loader,
            content,
            &mut warnings,
        )
        .await?;
        if !warnings.is_empty() {
            return Err(warnings.join("\n"));
        }
    }
    sync_packs(app, &client, &game_dir, mc_version, content).await
}

fn instance_game_dir(root: &Path, id: &str) -> Result<PathBuf, String> {
    let canonical_root = root.canonicalize().map_err(|error| error.to_string())?;
    let parent = canonical_root.join("instances");
    let game = parent.join(sanitize_component(id));
    for path in [&parent, &game] {
        match std::fs::symlink_metadata(path) {
            Ok(metadata) => {
                if !metadata.is_dir()
                    || path.canonicalize().map_err(|error| error.to_string())? != *path
                {
                    return Err("La carpeta de la instancia apunta a otra ubicación.".into());
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.to_string()),
        }
    }
    Ok(root.join("instances").join(sanitize_component(id)))
}

// Comando principal

pub async fn launch_game<
    F: std::future::Future<Output = Result<crate::identity::MinecraftAccount, String>>,
>(
    app: GameContext,
    mut request: LaunchRequest,
    refresh: impl FnOnce(String) -> F,
) -> Result<LaunchResult, String> {
    let mut warnings: Vec<String> = Vec::new();

    let mc = request.mc_version.trim().to_string();
    if mc.is_empty()
        || !mc
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_')
    {
        return Err("Versión de Minecraft inválida.".into());
    }

    let loader = request.loader.trim().to_lowercase();
    if !matches!(loader.as_str(), "vanilla" | "fabric" | "quilt" | "forge" | "neoforge") {
        return Err(format!(
            "El cargador '{}' no es compatible.",
            request.loader
        ));
    }

    // La versión fijada se valida antes de cualquier petición de red.
    let pinned = if loader == "vanilla" {
        None
    } else {
        pinned_loader_version(&loader, request.loader_version.as_deref())?
    };
    app.cancel.check()?;

    let client = build_client()?;
    let root = &app.root;
    let versions_dir = root.join("versions");
    let libs_dir = root.join("libraries");
    let assets_dir = root.join("assets");
    let game_dir = instance_game_dir(root, &request.instance_id)?;

    // 2) Versión de Minecraft
    emit(
        &app,
        "version",
        16,
        format!("Obteniendo Minecraft {}...", mc),
    );
    let vjson = guard(
        &app.cancel,
        load_vanilla_json(
            &client,
            &mc,
            &versions_dir.join(&mc).join(format!("{}.json", mc)),
        ),
    )
    .await?;

    let logging_argument = if let Some((task, argument)) = logging_download(&vjson, &assets_dir)? {
        download(&client, &task, &app.cancel).await.map_err(|e| {
            if e == CANCELLED {
                e
            } else {
                format!("No se pudo preparar la protección de Log4j: {}", e)
            }
        })?;
        Some(argument)
    } else {
        None
    };
    app.cancel.check()?;

    let required = vjson
        .pointer("/javaVersion/majorVersion")
        .and_then(|v| v.as_u64())
        .filter(|v| *v >= 8 && *v <= 99)
        .map(|v| v as u8)
        .unwrap_or_else(|| crate::java_runtime::required_java_major(&mc));
    let custom = request
        .java_override
        .clone()
        .filter(|path| !path.trim().is_empty())
        .or_else(|| {
            request
                .optimization
                .java_paths
                .get(&required.to_string())
                .cloned()
        });
    let java = crate::java_runtime::ensure_java_major(app.clone(), required, custom).await?;
    app.cancel.check()?;
    let java_path = PathBuf::from(
        java.java_path
            .clone()
            .ok_or("No se encontró Java compatible")?,
    );

    // 3) Cargador de mods (Fabric / Quilt / Forge / NeoForge)
    let fabric_json = match loader.as_str() {
        "fabric" => {
            emit(&app, "fabric", 18, "Obteniendo Fabric Loader...");
            Some(
                guard(
                    &app.cancel,
                    load_fabric_profile(
                        &client,
                        &mc,
                        &versions_dir.join(&mc).join(format!("fabric-{}.json", mc)),
                        pinned,
                    ),
                )
                .await?,
            )
        }
        "quilt" => {
            emit(&app, "fabric", 18, "Obteniendo Quilt Loader...");
            Some(
                guard(
                    &app.cancel,
                    load_quilt_profile(
                        &client,
                        &mc,
                        &versions_dir.join(&mc).join(format!("quilt-{}.json", mc)),
                        pinned,
                    ),
                )
                .await?,
            )
        }
        "forge" | "neoforge" => {
            emit(
                &app,
                "fabric",
                18,
                format!("Preparando {} para Minecraft {}...", loader, mc),
            );
            Some(
                load_forge_profile(
                    &client,
                    root,
                    &java_path,
                    &mc,
                    &loader,
                    &versions_dir
                        .join(&mc)
                        .join(forge_cache_name(&loader, &mc, pinned)),
                    pinned,
                    &app.cancel,
                )
                .await?,
            )
        }
        _ => None,
    };
    app.cancel.check()?;

    // 4) Librerías + client.jar
    emit(&app, "libraries", 20, "Preparando librerías...");
    let mut lib_tasks: Vec<DlTask> = Vec::new();
    let mut classpath: Vec<PathBuf> = Vec::new();
    let mut native_jars: Vec<PathBuf> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    if let Some(f) = &fabric_json {
        collect_libraries(
            f.get("libraries"),
            &libs_dir,
            &mut lib_tasks,
            &mut classpath,
            &mut native_jars,
            &mut seen,
        );
    }
    collect_libraries(
        vjson.get("libraries"),
        &libs_dir,
        &mut lib_tasks,
        &mut classpath,
        &mut native_jars,
        &mut seen,
    );

    let client_info = vjson
        .pointer("/downloads/client")
        .ok_or("Esta versión de Minecraft no tiene client.jar descargable.")?;
    let client_jar = versions_dir.join(&mc).join(format!("{}.jar", mc));
    lib_tasks.push(DlTask {
        url: client_info["url"]
            .as_str()
            .ok_or("client.jar sin URL")?
            .to_string(),
        dest: client_jar.clone(),
        sha1: client_info["sha1"].as_str().map(String::from),
        size: client_info["size"].as_u64(),
    });
    classpath.push(client_jar);

    run_downloads(
        &app,
        &client,
        lib_tasks,
        "libraries",
        20,
        50,
        "Descargando librerías",
    )
    .await?;
    app.cancel.check()?;

    // Natives (versiones antiguas)
    let natives_dir = game_dir.join(".openclient").join("natives").join(&mc);
    {
        let jars = native_jars.clone();
        let dest = natives_dir.clone();
        tokio::task::spawn_blocking(move || extract_natives(&jars, &dest))
            .await
            .map_err(|e| format!("Error extrayendo natives: {}", e))??;
    }

    // 5) Assets
    emit(&app, "assets", 52, "Verificando recursos del juego...");
    let asset_index = vjson
        .get("assetIndex")
        .ok_or("La versión no define assetIndex.")?;
    let asset_id = asset_index["id"]
        .as_str()
        .ok_or("assetIndex sin id")?
        .to_string();
    let index_path = assets_dir
        .join("indexes")
        .join(format!("{}.json", sanitize_component(&asset_id)));

    download(
        &client,
        &DlTask {
            url: asset_index["url"]
                .as_str()
                .ok_or("assetIndex sin url")?
                .to_string(),
            dest: index_path.clone(),
            sha1: asset_index["sha1"].as_str().map(String::from),
            size: asset_index["size"].as_u64(),
        },
        &app.cancel,
    )
    .await?;

    let index_text = tokio::fs::read_to_string(&index_path)
        .await
        .map_err(|e| format!("No se pudo leer el índice de assets: {}", e))?;
    let index_json: Value = serde_json::from_str(&index_text)
        .map_err(|e| format!("Índice de assets inválido: {}", e))?;

    let mut asset_tasks: Vec<DlTask> = Vec::new();
    let mut hashes: HashSet<String> = HashSet::new();
    if let Some(objects) = index_json["objects"].as_object() {
        for (_name, obj) in objects {
            let hash = match obj["hash"].as_str() {
                Some(h) if h.len() >= 2 && h.chars().all(|c| c.is_ascii_hexdigit()) => h,
                _ => continue,
            };
            if !hashes.insert(hash.to_string()) {
                continue;
            }
            let prefix = &hash[..2];
            asset_tasks.push(DlTask {
                url: format!(
                    "https://resources.download.minecraft.net/{}/{}",
                    prefix, hash
                ),
                dest: assets_dir.join("objects").join(prefix).join(hash),
                sha1: Some(hash.to_string()),
                size: obj["size"].as_u64(),
            });
        }
    }

    run_downloads(
        &app,
        &client,
        asset_tasks,
        "assets",
        52,
        78,
        "Descargando recursos",
    )
    .await?;
    app.cancel.check()?;

    // 6) Carpeta de la instancia + mods
    tokio::fs::create_dir_all(&game_dir)
        .await
        .map_err(|e| format!("No se pudo crear la carpeta de la instancia: {}", e))?;

    if matches!(loader.as_str(), "fabric" | "quilt" | "forge" | "neoforge") {
        sync_mods(
            &app,
            &client,
            &game_dir.join("mods"),
            &mc,
            &loader,
            &request.mods,
            &mut warnings,
        )
        .await?;
    }
    sync_packs(&app, &client, &game_dir, &mc, &request.mods).await?;
    // Última comprobación: desde aquí empieza el arranque y ya no se cancela.
    app.cancel.check()?;

    // 7) Argumentos
    emit(&app, "launch", 92, "Preparando arranque...");

    let optimization = request.optimization.clone();
    {
        let mut sys = sysinfo::System::new();
        sys.refresh_memory();
        let requested = optimization.allocated_ram_mb;
        let installed = crate::performance::installed_ram_mb(sys.total_memory() / (1024 * 1024));
        crate::performance::validate_ram(requested, installed)?;
        let available = sys.available_memory() / (1024 * 1024);
        if requested.saturating_add(1024) > available {
            warnings.push(format!(
                "Elegiste {} MB para Minecraft; hay {} MB disponibles en este momento. Se conserva tu selección. Windows y otras aplicaciones también necesitan memoria.",
                requested, available
            ));
        }
    }

    let classpath_str = classpath
        .iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join(CP_SEP);

    if request.account.account_type == "microsoft" {
        let current = refresh(format!("ms-{}", request.account.uuid)).await?;
        request.account.username = current.username;
        request.account.uuid = current.uuid;
        request.account.access_token = current.access_token;
    }
    let user_type = if request.account.account_type == "microsoft" {
        "msa"
    } else {
        "legacy"
    };
    let version_type = vjson["type"].as_str().unwrap_or("release").to_string();

    let mut vars: HashMap<&str, String> = HashMap::new();
    vars.insert("auth_player_name", request.account.username.clone());
    vars.insert("version_name", mc.clone());
    vars.insert("game_directory", game_dir.to_string_lossy().to_string());
    vars.insert("assets_root", assets_dir.to_string_lossy().to_string());
    vars.insert("game_assets", assets_dir.to_string_lossy().to_string());
    vars.insert("assets_index_name", asset_id.clone());
    vars.insert("auth_uuid", request.account.uuid.replace('-', ""));
    vars.insert("auth_access_token", request.account.access_token.clone());
    vars.insert("auth_session", request.account.access_token.clone());
    vars.insert("clientid", "openclient".to_string());
    vars.insert("auth_xuid", "0".to_string());
    vars.insert("user_type", user_type.to_string());
    vars.insert("user_properties", "{}".to_string());
    vars.insert("version_type", version_type);
    vars.insert("launcher_name", LAUNCHER_NAME.to_string());
    vars.insert("launcher_version", app.launcher_version.to_string());
    vars.insert(
        "natives_directory",
        natives_dir.to_string_lossy().to_string(),
    );
    vars.insert("library_directory", libs_dir.to_string_lossy().to_string());
    vars.insert("classpath_separator", CP_SEP.to_string());
    vars.insert("classpath", classpath_str.clone());

    let main_class = fabric_json
        .as_ref()
        .and_then(|f| f["mainClass"].as_str())
        .or_else(|| vjson["mainClass"].as_str())
        .ok_or("No se encontró la clase principal de Minecraft.")?
        .to_string();

    // JVM: seguridad (Log4Shell) + flags de optimización + argumentos de la versión
    let mut jvm_args: Vec<String> = vec!["-Dlog4j2.formatMsgNoLookups=true".to_string()];
    if let Some(argument) = logging_argument {
        jvm_args.push(argument);
    }
    jvm_args.extend(build_god_jvm_flags(
        optimization.clone(),
        java.installed_major.unwrap_or(java.required_major),
    ));

    if vjson.pointer("/arguments/jvm").is_some() {
        jvm_args.extend(collect_args(vjson.pointer("/arguments/jvm"), &vars));
    } else {
        jvm_args.push(format!(
            "-Djava.library.path={}",
            natives_dir.to_string_lossy()
        ));
        jvm_args.push("-cp".to_string());
        jvm_args.push(classpath_str.clone());
    }
    if let Some(f) = &fabric_json {
        jvm_args.extend(collect_args(f.pointer("/arguments/jvm"), &vars));
    }

    // Juego
    let mut game_args: Vec<String> = Vec::new();
    if vjson.pointer("/arguments/game").is_some() {
        game_args.extend(collect_args(vjson.pointer("/arguments/game"), &vars));
    } else if let Some(legacy) = vjson["minecraftArguments"].as_str() {
        game_args.extend(legacy.split_whitespace().map(|a| subst(a, &vars)));
    }
    if let Some(f) = &fabric_json {
        game_args.extend(collect_args(f.pointer("/arguments/game"), &vars));
    }
    if optimization.window_width >= 320
        && optimization.window_height >= 240
        && !game_args.iter().any(|arg| arg == "--width" || arg == "--height")
    {
        game_args.extend([
            "--width".to_string(),
            optimization.window_width.to_string(),
            "--height".to_string(),
            optimization.window_height.to_string(),
        ]);
    }
    if optimization.fullscreen && !game_args.iter().any(|arg| arg == "--fullscreen") {
        game_args.push("--fullscreen".to_string());
    }
    if !request.join_server.is_empty() {
        let address = server_address(&request.join_server)?;
        let quick_play = vjson
            .pointer("/arguments/game")
            .is_some_and(|arguments| arguments.to_string().contains("quickPlayMultiplayer"));
        if quick_play {
            game_args.extend(["--quickPlayMultiplayer".to_string(), address]);
        } else {
            let (host, port) = address.split_once(':').unwrap_or((&address, "25565"));
            game_args.extend([
                "--server".to_string(),
                host.to_string(),
                "--port".to_string(),
                port.to_string(),
            ]);
        }
    }

    // 8) Ejecutar
    let logs_dir = game_dir.join("logs");
    tokio::fs::create_dir_all(&logs_dir)
        .await
        .map_err(|e| format!("No se pudo crear la carpeta de logs: {}", e))?;
    rotate_logs(&logs_dir);
    let log_path = logs_dir.join("openclient-latest.log");

    let mut log_file =
        std::fs::File::create(&log_path).map_err(|e| format!("No se pudo crear el log: {}", e))?;
    {
        use std::io::Write;
        // El token de acceso NUNCA se escribe en el log.
        let _ = writeln!(
            log_file,
            "[OpenClient] Minecraft {} ({}) | Java {} | {}\n[OpenClient] Usuario: {} ({})\n[OpenClient] RAM: {} MB | Clase principal: {}\n",
            mc,
            loader,
            java.installed_major.unwrap_or(0),
            java_path.display(),
            request.account.username,
            request.account.account_type,
            optimization.allocated_ram_mb,
            main_class
        );
    }
    let log_err = log_file
        .try_clone()
        .map_err(|e| format!("No se pudo duplicar el log: {}", e))?;

    // `java.exe` (con stdout redirigido al log) en lugar de `javaw.exe`, sin ventana de consola.
    let mut exe = java_path.clone();
    #[cfg(target_os = "windows")]
    {
        let console = java_path.with_file_name("java.exe");
        if console.exists() {
            exe = console;
        }
    }

    let hook_environment = vec![
        ("OPENCLIENT_INSTANCE_ID", request.instance_id.clone()),
        ("OPENCLIENT_MC_VERSION", mc.clone()),
        ("OPENCLIENT_LOADER", loader.clone()),
        ("OPENCLIENT_GAME_DIR", game_dir.to_string_lossy().into_owned()),
    ];
    if !request.hooks.pre_launch.is_empty() {
        emit(&app, "launch", 96, "Ejecutando el comando previo...");
        let command = request.hooks.pre_launch.clone();
        let directory = game_dir.clone();
        let environment = hook_environment.clone();
        tokio::task::spawn_blocking(move || crate::hooks::run(&command, &directory, &environment))
            .await
            .map_err(|error| error.to_string())?
            .map_err(|error| format!("El comando previo falló: {error}"))?;
    }
    let mut cmd = if request.hooks.wrapper.is_empty() {
        let mut direct = std::process::Command::new(&exe);
        direct.args(&jvm_args);
        direct
    } else {
        let parts = crate::hooks::split_command(&request.hooks.wrapper)?;
        let mut wrapped = std::process::Command::new(&parts[0]);
        wrapped.args(&parts[1..]).arg(&exe).args(&jvm_args);
        wrapped
    };
    cmd.arg(&main_class)
        .args(&game_args)
        .current_dir(&game_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::from(log_file))
        .stderr(Stdio::from(log_err));

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        const HIGH_PRIORITY_CLASS: u32 = 0x00000080;
        let mut flags = CREATE_NO_WINDOW;
        if optimization.high_process_priority {
            flags |= HIGH_PRIORITY_CLASS;
        }
        cmd.creation_flags(flags);
    }

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("No se pudo iniciar Minecraft: {}", e))?;
    let pid = child.id();
    if let Err(error) = app.emit(
        "game-started",
        serde_json::json!({
            "pid": pid, "version": mc, "loader": loader, "minimize": optimization.minimize_on_launch
        }),
    ) {
        warnings.push(error);
    }

    let refocus = optimization.refocus_on_exit;
    let app_for_exit = app.clone();
    let instance_id = request.instance_id.clone();
    let log_for_exit = log_path.to_string_lossy().to_string();
    let post_exit = request.hooks.post_exit.clone();
    let log_for_hook = log_path.clone();
    let post_exit_directory = game_dir.clone();
    let started_at = std::time::Instant::now();
    std::thread::spawn(move || {
        let exit_code = child.wait().ok().and_then(|s| s.code());
        let elapsed_seconds = started_at.elapsed().as_secs();
        let _ = app_for_exit.emit(
            "game-stopped",
            serde_json::json!({"pid": pid, "refocus": refocus}),
        );
        let _ = app_for_exit.emit(
            "game-exited",
            GameExited {
                instance_id,
                exit_code,
                log_path: log_for_exit,
                elapsed_seconds,
            },
        );
        if !post_exit.is_empty() {
            let mut environment = hook_environment;
            environment.push((
                "OPENCLIENT_EXIT_CODE",
                exit_code.map_or_else(String::new, |code| code.to_string()),
            ));
            if let Err(error) = crate::hooks::run(&post_exit, &post_exit_directory, &environment) {
                use std::io::Write;
                if let Ok(mut log) = std::fs::OpenOptions::new().append(true).open(&log_for_hook) {
                    let _ = writeln!(log, "
[OpenClient] El comando posterior falló: {error}");
                }
            }
        }
    });

    emit(&app, "running", 100, "Minecraft se está iniciando...");

    Ok(LaunchResult {
        pid,
        game_dir: game_dir.to_string_lossy().to_string(),
        log_path: log_path.to_string_lossy().to_string(),
        java_path: exe.to_string_lossy().to_string(),
        warnings,
    })
}

pub async fn install_instance_mods(
    app: GameContext,
    instance_id: String,
    mc_version: String,
    mods: Vec<LaunchMod>,
) -> Result<(), String> {
    if mc_version.is_empty()
        || !mc_version
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_')
    {
        return Err("Versión inválida".into());
    }
    let mut warnings = Vec::new();
    sync_mods(
        &app,
        &build_client()?,
        &app.root
            .join("instances")
            .join(sanitize_component(&instance_id))
            .join("mods"),
        &mc_version,
        "fabric",
        &mods,
        &mut warnings,
    )
    .await?;
    if !warnings.is_empty() {
        return Err(warnings.join("\n"));
    }
    Ok(())
}

/// Conserva los registros de las dos partidas anteriores junto al más reciente.
fn rotate_logs(directory: &Path) {
    let file = |name: &str| directory.join(format!("openclient-{name}.log"));
    let _ = std::fs::remove_file(file("previous-2"));
    let _ = std::fs::rename(file("previous-1"), file("previous-2"));
    let _ = std::fs::rename(file("latest"), file("previous-1"));
}

pub fn read_instance_log(root: &Path, instance_id: String) -> Result<String, String> {
    use std::io::{Read, Seek, SeekFrom};
    let path = root
        .join("instances")
        .join(sanitize_component(&instance_id))
        .join("logs")
        .join("openclient-latest.log");
    let path = if path.exists() {
        path
    } else {
        path.with_file_name("alba-launcher-latest.log")
    };
    let mut file = std::fs::File::open(path).map_err(|_| "Sin registro todavía".to_string())?;
    let len = file.metadata().map_err(|e| e.to_string())?.len();
    file.seek(SeekFrom::Start(len.saturating_sub(65536)))
        .map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).map_err(|e| e.to_string())?;
    Ok(String::from_utf8_lossy(&bytes).to_string())
}

#[cfg(test)]
mod regression_tests {
    use super::*;
    #[test]
    fn server_addresses_are_normalized_or_rejected() {
        assert_eq!(server_address("  play.example.net ").unwrap(), "play.example.net");
        assert_eq!(server_address("192.168.1.20:25570").unwrap(), "192.168.1.20:25570");
        assert_eq!(server_address("").unwrap(), "");
        for bad in ["host:0", "host:99999", "host:", "-host", "ho st", "a;calc", "host:25565:1", "--server"] {
            assert!(server_address(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn log_rotation_keeps_the_two_previous_games() {
        let directory = std::env::temp_dir().join(format!("openclient-logs-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&directory).unwrap();
        for game in ["uno", "dos", "tres", "cuatro"] {
            rotate_logs(&directory);
            std::fs::write(directory.join("openclient-latest.log"), game).unwrap();
        }
        let read = |name: &str| std::fs::read_to_string(directory.join(name)).unwrap();
        assert_eq!(read("openclient-latest.log"), "cuatro");
        assert_eq!(read("openclient-previous-1.log"), "tres");
        assert_eq!(read("openclient-previous-2.log"), "dos");
        assert_eq!(std::fs::read_dir(&directory).unwrap().count(), 3);
        std::fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn native_zip_extraction_keeps_only_native_files_inside_destination() {
        use std::io::Write;
        let root = std::env::temp_dir().join(format!(
            "openclient-native-zip-test-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let archive = root.join("native.jar");
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&archive).unwrap());
        let options = zip::write::SimpleFileOptions::default();
        zip.start_file("windows/x64/test.dll", options).unwrap();
        zip.write_all(b"native-fixture").unwrap();
        zip.start_file("README.txt", options).unwrap();
        zip.write_all(b"not-native").unwrap();
        zip.finish().unwrap();
        let destination = root.join("natives");
        extract_natives(&[archive], &destination).unwrap();
        assert_eq!(
            std::fs::read(destination.join("test.dll")).unwrap(),
            b"native-fixture"
        );
        assert!(!destination.join("README.txt").exists());
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn logging_fails_closed_and_does_not_accept_external_paths_or_hosts() {
        let missing = serde_json::json!({"libraries":[{"name":"org.apache.logging.log4j:log4j-core:2.0-beta9"}]});
        assert!(logging_download(&missing, Path::new("assets")).is_err());
        assert!(
            logging_download(&serde_json::json!({}), Path::new("assets"))
                .unwrap()
                .is_none()
        );
        let mut version = serde_json::json!({"logging":{"client":{
            "argument":"-Dlog4j.configurationFile=${path}",
            "file":{"id":"client-1.7.xml","url":"https://launcher.mojang.com/config.xml","sha1":"1234567890123456789012345678901234567890","size":100}
        }}});
        let (task, arg) = logging_download(&version, Path::new("assets with spaces"))
            .unwrap()
            .unwrap();
        assert_eq!(
            task.dest,
            Path::new("assets with spaces/log_configs/client-1.7.xml")
        );
        assert!(arg.ends_with(&task.dest.to_string_lossy().to_string()));
        version["logging"]["client"]["file"]["id"] = "../escape.xml".into();
        assert!(logging_download(&version, Path::new("assets")).is_err());
        version["logging"]["client"]["file"]["id"] = "client.xml".into();
        version["logging"]["client"]["file"]["url"] = "https://attacker.example/config.xml".into();
        assert!(logging_download(&version, Path::new("assets")).is_err());
    }
    #[test]
    fn required_version_only_dependency_is_not_silently_skipped() {
        assert_eq!(required_dependency(&serde_json::json!({"dependency_type":"required","project_id":null,"version_id":"Ab123"})).unwrap(), Some("version:Ab123".into()));
        assert!(required_dependency(
            &serde_json::json!({"dependency_type":"required","project_id":null})
        )
        .is_err());
        assert_eq!(
            required_dependency(&serde_json::json!({"dependency_type":"optional"})).unwrap(),
            None
        );
    }
    #[tokio::test]
    #[ignore = "Requires access to official Mojang services"]
    async fn official_logging_configs_are_verified_for_old_and_current_versions() {
        let client = build_client().unwrap();
        let manifest = fetch_json(
            &client,
            "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json",
        )
        .await
        .unwrap();
        let root =
            std::env::temp_dir().join(format!("openclient-logging-test-{}", uuid::Uuid::new_v4()));
        for id in ["1.8.9", "1.12.2", "1.17.1", "1.21.4"] {
            let entry = manifest["versions"]
                .as_array()
                .unwrap()
                .iter()
                .find(|v| v["id"] == id)
                .unwrap();
            let version = fetch_json(&client, entry["url"].as_str().unwrap())
                .await
                .unwrap();
            let (task, arg) = logging_download(&version, &root).unwrap().unwrap();
            download(&client, &task, &CancelToken::default()).await.unwrap();
            assert!(arg.contains("log4j.configurationFile="));
            assert_eq!(
                hex(&Sha1::digest(std::fs::read(&task.dest).unwrap())),
                task.sha1.unwrap()
            );
            println!("Mojang {}: configuración y SHA-1 verificados", id);
        }
        std::fs::remove_dir_all(root).unwrap();
    }
    #[tokio::test]
    async fn cached_corrupt_file_is_not_accepted_by_size() {
        let path =
            std::env::temp_dir().join(format!("openclient-cache-test-{}", uuid::Uuid::new_v4()));
        tokio::fs::write(&path, b"bad").await.unwrap();
        let task = DlTask {
            url: "http://127.0.0.1:1/unavailable".into(),
            dest: path.clone(),
            sha1: Some(hex(&Sha1::digest(b"yes"))),
            size: Some(3),
        };
        let client = reqwest::Client::builder()
            .timeout(Duration::from_millis(100))
            .build()
            .unwrap();
        let result = download(&client, &task, &CancelToken::default()).await;
        tokio::fs::remove_file(path).await.unwrap();
        assert!(result.is_err());
    }
    #[test]
    fn external_paths_stay_inside_instance() {
        assert!(safe_join(Path::new("instance"), "../escape.jar").is_none());
        assert!(safe_join(Path::new("instance"), "mods/valid.jar").is_some());
    }
}

/// Pruebas sin red: perfiles Forge/NeoForge/Quilt de ejemplo, resolución de
/// dependencias con el registro local de versiones y paquetes ya descargados.
#[cfg(test)]
mod offline_tests {
    use super::*;
    use serde_json::json;
    use std::fs;

    fn temp_dir(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("openclient-{label}-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn context(root: &Path) -> GameContext {
        GameContext::new(root.to_owned(), "test", |_, _| Ok(()))
    }

    fn item(id: &str, source: &str, kind: &str, enabled: bool, version: Option<&str>) -> LaunchMod {
        LaunchMod {
            version_id: version.map(String::from),
            id: id.into(),
            source: source.into(),
            enabled,
            content_type: kind.into(),
            download_url: None,
            file_name: None,
            sha1: None,
        }
    }

    fn modrinth(id: &str) -> LaunchMod {
        item(id, "modrinth", "mod", true, None)
    }

    // Perfiles de cargador

    fn neoforge_profile() -> Value {
        json!({
            "id": "neoforge-21.0.5", "inheritsFrom": "1.21",
            "mainClass": "cpw.mods.bootstraplauncher.BootstrapLauncher",
            "arguments": {
                "game": ["--fml.neoForgeVersion", "21.0.5", "--launchTarget", "forgeclient"],
                "jvm": [
                    "-DlibraryDirectory=${library_directory}",
                    "-DignoreList=bootstraplauncher,${version_name}.jar",
                    "-p",
                    "${library_directory}/a/a.jar${classpath_separator}${library_directory}/b/b.jar",
                    {"rules": [{"action": "allow", "os": {"name": "osx"}}], "value": "-XstartOnFirstThread"},
                    {"rules": [{"action": "allow", "os": {"name": "windows"}}], "value": ["-Dwin=1", "-Dwin2=${version_name}"]},
                    {"value": "-Dsin.reglas=1"},
                    {"rules": [{"action": "allow", "features": {"is_demo_user": true}}], "value": "--demo"},
                    42
                ]
            },
            "libraries": [
                {"name": "org.ow2.asm:asm:9.7", "downloads": {"artifact": {
                    "path": "org/ow2/asm/asm/9.7/asm-9.7.jar",
                    "url": "https://maven.neoforged.net/releases/org/ow2/asm/asm/9.7/asm-9.7.jar",
                    "sha1": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", "size": 100}}},
                {"name": "net.neoforged:neoforge:21.0.5:universal", "downloads": {"artifact": {
                    "path": "net/neoforged/neoforge/21.0.5/neoforge-21.0.5-universal.jar", "url": "", "size": 5}}},
                {"name": "org.lwjgl:lwjgl:3.3.3", "rules": [{"action": "allow", "os": {"name": "osx"}}],
                 "downloads": {"artifact": {"path": "org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3.jar", "url": "https://libraries.minecraft.net/x.jar"}}},
                {"name": "evil:escape:1.0", "downloads": {"artifact": {"path": "../escape.jar", "url": "https://example.net/escape.jar"}}}
            ]
        })
    }

    fn collect(libs: &[&Value], base: &Path) -> (Vec<DlTask>, Vec<PathBuf>, Vec<PathBuf>) {
        let (mut tasks, mut classpath, mut natives) = (Vec::new(), Vec::new(), Vec::new());
        let mut seen = HashSet::new();
        for list in libs {
            collect_libraries(Some(*list), base, &mut tasks, &mut classpath, &mut natives, &mut seen);
        }
        (tasks, classpath, natives)
    }

    #[test]
    fn neoforge_installer_prefers_newest_stable_build_for_the_minecraft_line() {
        let meta = json!({"versions": ["20.4.237", "21.0.1-beta", "21.0.5", "21.0.6-beta", "21.1.1"]});
        assert_eq!(
            neoforge_installer_url("1.21", &meta).unwrap(),
            "https://maven.neoforged.net/releases/net/neoforged/neoforge/21.0.5/neoforge-21.0.5-installer.jar"
        );
        assert!(neoforge_installer_url("1.20.4", &meta).unwrap().contains("/20.4.237/"));
        assert!(neoforge_installer_url("1.21.1", &meta).unwrap().contains("/21.1.1/"));
        // Sin builds estables se acepta la beta más reciente de esa línea.
        let betas = json!({"versions": ["21.0.1-beta", "21.0.2-beta"]});
        assert!(neoforge_installer_url("1.21", &betas).unwrap().contains("/21.0.2-beta/"));
        assert!(neoforge_installer_url("1.19.2", &meta)
            .unwrap_err()
            .contains("NeoForge no está disponible para Minecraft 1.19.2"));
        assert!(neoforge_installer_url("1.21", &json!({"versions": "no"})).is_err());
        assert!(neoforge_installer_url("1.21", &json!({})).is_err());
    }

    #[test]
    fn forge_installer_uses_recommended_then_latest_and_rejects_unknown_versions() {
        let promos = json!({"promos": {
            "1.20.1-latest": "47.3.0", "1.20.1-recommended": "47.2.0", "1.19.4-latest": "45.2.0"}});
        assert_eq!(
            forge_installer_url("1.20.1", &promos).unwrap(),
            "https://maven.minecraftforge.net/net/minecraftforge/forge/1.20.1-47.2.0/forge-1.20.1-47.2.0-installer.jar"
        );
        assert!(forge_installer_url("1.19.4", &promos).unwrap().contains("1.19.4-45.2.0"));
        assert_eq!(
            forge_installer_url("1.8.9", &promos).unwrap_err(),
            "Forge no está disponible para Minecraft 1.8.9."
        );
        assert!(forge_installer_url("1.20.1", &json!({"promos": []})).is_err());
        // Una versión no textual no se convierte en URL.
        assert!(forge_installer_url("1.20.1", &json!({"promos": {"1.20.1-latest": 5}})).is_err());
    }

    #[test]
    fn installer_archive_yields_its_version_json() {
        use std::io::Write;
        let dir = temp_dir("installer");
        let write_zip = |name: &str, entry: &str, body: &str| {
            let path = dir.join(name);
            let mut zip = zip::ZipWriter::new(fs::File::create(&path).unwrap());
            zip.start_file(entry, zip::write::SimpleFileOptions::default()).unwrap();
            zip.write_all(body.as_bytes()).unwrap();
            zip.finish().unwrap();
            path
        };
        let good = write_zip("good.jar", "version.json", &neoforge_profile().to_string());
        assert_eq!(installer_version_json(&good).unwrap()["id"], "neoforge-21.0.5");
        let missing = write_zip("missing.jar", "install_profile.json", "{}");
        assert_eq!(
            installer_version_json(&missing).unwrap_err(),
            "El instalador del cargador no contiene version.json."
        );
        assert!(installer_version_json(&write_zip("bad.jar", "version.json", "{no json")).is_err());
        fs::write(dir.join("plain.jar"), b"no es un zip").unwrap();
        assert!(installer_version_json(&dir.join("plain.jar")).is_err());
        assert!(installer_version_json(&dir.join("absent.jar")).is_err());
        fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn installed_profile_is_found_by_loader_and_version_and_cached() {
        let root = temp_dir("installed");
        let cache = root.join("cache").join("neoforge-1.21.4.json");
        fs::create_dir_all(cache.parent().unwrap()).unwrap();
        assert!(read_installed_profile(&root, "neoforge", "1.21.4", None, &cache).await.is_none());
        let write_profile = |dir: &str, text: &str| {
            fs::create_dir_all(root.join("versions").join(dir)).unwrap();
            fs::write(root.join("versions").join(dir).join(format!("{dir}.json")), text).unwrap();
        };
        write_profile("fabric-loader-0.16.9-1.21.4", r#"{"id":"fabric"}"#);
        write_profile("neoforge-1.21.1-21.1.1", r#"{"id":"otra-version"}"#);
        write_profile("NeoForge-1.21.4-21.4.1", "{corrupto");
        assert!(read_installed_profile(&root, "neoforge", "1.21.4", None, &cache).await.is_none());
        assert!(!cache.exists());
        write_profile("neoforge-1.21.4-21.4.2", r#"{"id":"buena"}"#);
        let found = read_installed_profile(&root, "neoforge", "1.21.4", None, &cache).await.unwrap();
        assert_eq!(found["id"], "buena");
        assert_eq!(fs::read_to_string(&cache).unwrap(), r#"{"id":"buena"}"#);
        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn cached_forge_profile_is_used_without_network_or_java() {
        let root = temp_dir("forge-cache");
        let client = build_client().unwrap();
        for loader in ["forge", "neoforge"] {
            let cache = root.join("versions").join("1.21").join(format!("{loader}-1.21.json"));
            fs::create_dir_all(cache.parent().unwrap()).unwrap();
            fs::write(&cache, neoforge_profile().to_string()).unwrap();
            let profile = load_forge_profile(
                &client,
                &root,
                &root.join("no-existe").join("java.exe"),
                "1.21",
                loader,
                &cache,
                None,
                &CancelToken::default(),
            )
            .await
            .unwrap();
            assert_eq!(profile["mainClass"], "cpw.mods.bootstraplauncher.BootstrapLauncher");
        }
        // Con la caché local no se crea el perfil de launcher ni se lanza el instalador.
        assert!(!root.join("launcher_profiles.json").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn pinned_fabric_version_is_validated_before_any_request() {
        let client = build_client().unwrap();
        let too_long = "1".repeat(49);
        for bad in ["", "../../x", "v0.16", "0.16 9", "0.16/9", too_long.as_str()] {
            let error = load_fabric_profile(&client, "1.21.4", Path::new("fabric.json"), Some(bad))
                .await
                .unwrap_err();
            assert_eq!(error, "Versión de Fabric inválida.", "{bad}");
        }
    }

    #[test]
    fn loader_libraries_win_over_vanilla_duplicates_and_unsafe_entries_are_dropped() {
        let base = Path::new("libs");
        let forge = neoforge_profile();
        let vanilla = json!([
            {"name": "org.ow2.asm:asm:9.6", "downloads": {"artifact": {
                "path": "org/ow2/asm/asm/9.6/asm-9.6.jar", "url": "https://libraries.minecraft.net/asm-9.6.jar"}}},
            {"name": "com.google.code.gson:gson:2.10.1", "downloads": {"artifact": {
                "path": "com/google/code/gson/gson/2.10.1/gson-2.10.1.jar", "url": "https://libraries.minecraft.net/gson.jar"}}}
        ]);
        let (tasks, classpath, natives) = collect(&[&forge["libraries"], &vanilla], base);
        assert!(natives.is_empty());
        // asm: gana la versión del cargador. Quedan fuera: universal sin URL,
        // lwjgl con regla solo para macOS y la ruta con `..`.
        assert_eq!(
            classpath,
            vec![
                base.join("org/ow2/asm/asm/9.7/asm-9.7.jar"),
                base.join("com/google/code/gson/gson/2.10.1/gson-2.10.1.jar")
            ]
        );
        assert_eq!(tasks.len(), 2);
        assert_eq!(tasks[0].sha1.as_deref(), Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"));
        assert_eq!(tasks[0].size, Some(100));
        assert!(tasks.iter().all(|task| task.dest.starts_with(base)));
    }

    #[test]
    fn libraries_with_classifiers_and_legacy_natives_are_kept_apart() {
        let base = Path::new("libs");
        let libs = json!([
            {"name": "org.lwjgl:lwjgl:3.3.3", "downloads": {"artifact": {"path": "l/lwjgl.jar", "url": "https://x.net/l.jar"}}},
            {"name": "org.lwjgl:lwjgl:3.3.3:natives-windows", "downloads": {"artifact": {"path": "l/lwjgl-natives.jar", "url": "https://x.net/n.jar"}}},
            {"name": "org.lwjgl:lwjgl:3.3.2", "downloads": {"artifact": {"path": "l/duplicada.jar", "url": "https://x.net/d.jar"}}},
            {"name": "legacy:natives:1.0", "natives": {"windows": "natives-windows-${arch}"},
             "downloads": {"classifiers": {"natives-windows-64": {"path": "legacy/n64.jar", "url": "https://x.net/n64.jar"}},
                           "artifact": {"path": "legacy/main.jar", "url": "https://x.net/main.jar"}}}
        ]);
        let (tasks, classpath, natives) = collect(&[&libs], base);
        assert_eq!(
            classpath,
            vec![base.join("l/lwjgl.jar"), base.join("l/lwjgl-natives.jar"), base.join("legacy/main.jar")]
        );
        assert_eq!(natives, vec![base.join("legacy/n64.jar")]);
        assert_eq!(tasks.len(), 4);
        // Sin lista de librerías no hay nada que hacer.
        assert_eq!(collect(&[&json!(null), &json!({})], base).0.len(), 0);
    }

    #[test]
    fn maven_style_libraries_from_fabric_and_quilt_profiles_get_a_repository_url() {
        let base = Path::new("libs");
        let quilt = json!([
            {"name": "org.quiltmc:quilt-loader:0.26.4", "url": "https://maven.quiltmc.org/repository/release"},
            {"name": "net.fabricmc:intermediary:1.21.4", "url": "https://maven.fabricmc.net/", "sha1": "b".repeat(40), "size": 7},
            {"name": "sin.url:lib:1.0"},
            {"name": "mala:coordenada", "url": "https://maven.example.net/"}
        ]);
        let (tasks, classpath, _) = collect(&[&quilt], base);
        assert_eq!(tasks.len(), 2);
        assert_eq!(
            tasks[0].url,
            "https://maven.quiltmc.org/repository/release/org/quiltmc/quilt-loader/0.26.4/quilt-loader-0.26.4.jar"
        );
        assert_eq!(tasks[1].url, "https://maven.fabricmc.net/net/fabricmc/intermediary/1.21.4/intermediary-1.21.4.jar");
        assert_eq!(tasks[1].sha1.as_deref(), Some("b".repeat(40).as_str()));
        assert_eq!(classpath[0], base.join("org/quiltmc/quilt-loader/0.26.4/quilt-loader-0.26.4.jar"));
        assert_eq!(classpath.len(), 2);
    }

    #[test]
    fn maven_coordinates_and_dedup_keys() {
        assert_eq!(maven_path("net.fabricmc:fabric-loader:0.16.9").unwrap(), "net/fabricmc/fabric-loader/0.16.9/fabric-loader-0.16.9.jar");
        assert_eq!(
            maven_path("org.lwjgl:lwjgl:3.3.3:natives-windows").unwrap(),
            "org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3-natives-windows.jar"
        );
        assert_eq!(
            maven_path("net.minecraftforge:forge:1.20.1-47.2.0:universal@zip").unwrap(),
            "net/minecraftforge/forge/1.20.1-47.2.0/forge-1.20.1-47.2.0-universal.zip"
        );
        assert!(maven_path("solo:dos").is_none());
        assert_eq!(library_key("org.ow2.asm:asm:9.7"), "org.ow2.asm:asm");
        assert_eq!(library_key("org.lwjgl:lwjgl:3.3.3:natives-windows"), "org.lwjgl:lwjgl:natives-windows");
        assert_eq!(library_key("a:b:1@zip"), "a:b");
        assert_eq!(library_key("suelto"), "suelto");
    }

    #[test]
    fn loader_arguments_apply_rules_variables_and_nested_values() {
        let profile = neoforge_profile();
        let mut vars: HashMap<&str, String> = HashMap::new();
        vars.insert("library_directory", "C:/libs".into());
        vars.insert("version_name", "1.21".into());
        vars.insert("classpath_separator", CP_SEP.into());
        let jvm = collect_args(profile.pointer("/arguments/jvm"), &vars);
        assert_eq!(
            jvm,
            vec![
                "-DlibraryDirectory=C:/libs".to_string(),
                "-DignoreList=bootstraplauncher,1.21.jar".into(),
                "-p".into(),
                format!("C:/libs/a/a.jar{CP_SEP}C:/libs/b/b.jar"),
                "-Dwin=1".into(),
                "-Dwin2=1.21".into(),
                "-Dsin.reglas=1".into(),
            ]
        );
        // Los argumentos de juego se copian tal cual; las variables desconocidas no se tocan.
        assert_eq!(
            collect_args(profile.pointer("/arguments/game"), &vars),
            vec!["--fml.neoForgeVersion", "21.0.5", "--launchTarget", "forgeclient"]
        );
        assert_eq!(collect_args(Some(&json!(["${desconocida}"])), &vars), vec!["${desconocida}"]);
        assert!(collect_args(None, &vars).is_empty());
        assert!(collect_args(Some(&json!("no es lista")), &vars).is_empty());
    }

    #[test]
    fn rule_lists_follow_the_last_matching_rule() {
        assert!(rules_allow(None));
        assert!(rules_allow(Some(&json!([]))));
        let windows = json!({"name": "windows", "arch": "x86_64"});
        assert!(rules_allow(Some(&json!([{"action": "allow", "os": windows}]))));
        assert!(!rules_allow(Some(&json!([{"action": "allow", "os": {"name": "linux"}}]))));
        assert!(!rules_allow(Some(&json!([{"action": "allow"}, {"action": "disallow", "os": {"name": "windows"}}]))));
        assert!(rules_allow(Some(&json!([{"action": "disallow", "os": {"name": "osx"}}, {"action": "allow"}]))));
        assert!(!rules_allow(Some(&json!([{"action": "allow", "os": {"name": "windows", "arch": "x86"}}]))));
        assert!(!rules_allow(Some(&json!([{"action": "allow", "os": {"name": "windows", "version": "^10"}}]))));
        assert!(!rules_allow(Some(&json!([{"action": "allow", "features": {"has_custom_resolution": true}}]))));
    }

    // Rutas

    #[test]
    fn component_names_and_instance_directories_cannot_escape() {
        assert_eq!(sanitize_component("mi instancia/ñ"), "mi_instancia__");
        assert_eq!(sanitize_component("../../escape"), "_.._escape");
        assert_eq!(sanitize_component("..."), "default");
        assert_eq!(sanitize_component(""), "default");
        let root = temp_dir("game-dir");
        assert_eq!(
            instance_game_dir(&root, "../x").unwrap(),
            root.join("instances").join("_x")
        );
        // `instances` no puede ser un archivo.
        fs::write(root.join("instances"), b"archivo").unwrap();
        assert!(instance_game_dir(&root, "abc").is_err());
        assert!(instance_game_dir(&root.join("falta"), "abc").is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn instance_log_prefers_the_new_name_falls_back_to_legacy_and_reads_only_the_tail() {
        let root = temp_dir("log");
        assert_eq!(read_instance_log(&root, "x".into()).unwrap_err(), "Sin registro todavía");
        let logs = root.join("instances").join("x").join("logs");
        fs::create_dir_all(&logs).unwrap();
        fs::write(logs.join("alba-launcher-latest.log"), "antiguo").unwrap();
        assert_eq!(read_instance_log(&root, "x".into()).unwrap(), "antiguo");
        fs::write(logs.join("openclient-latest.log"), format!("{}FIN", "a".repeat(70_000))).unwrap();
        let tail = read_instance_log(&root, "x".into()).unwrap();
        assert_eq!(tail.len(), 65_536);
        assert!(tail.ends_with("aaFIN"));
        // El identificador se sanea antes de construir la ruta.
        assert!(read_instance_log(&root, "../x".into()).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    // sync_mods / required_dependency

    fn version(project: &str, dependencies: Vec<Value>) -> Value {
        json!({
            "id": format!("v{project}"), "project_id": project, "version_type": "release",
            "game_versions": ["1.21.4"], "loaders": ["fabric"], "dependencies": dependencies, "files": []
        })
    }

    fn required(project: &str) -> Value {
        json!({"dependency_type": "required", "project_id": project})
    }

    /// Prepara `<tmp>/mods` con un registro de versiones ya resueltas: así no hace falta red.
    fn mods_dir_with_lock(entries: &[(&str, Value)]) -> (PathBuf, PathBuf) {
        let root = temp_dir("sync-mods");
        let mods = root.join("mods");
        fs::create_dir_all(&mods).unwrap();
        let lock: serde_json::Map<String, Value> =
            entries.iter().map(|(key, value)| (key.to_string(), value.clone())).collect();
        fs::write(mods.join(".openclient_versions.json"), Value::Object(lock).to_string()).unwrap();
        (root, mods)
    }

    async fn sync(mods: &Path, loader: &str, content: &[LaunchMod]) -> (Result<(), String>, Vec<String>) {
        let mut warnings = Vec::new();
        let result = sync_mods(
            &context(mods.parent().unwrap()),
            &build_client().unwrap(),
            mods,
            "1.21.4",
            loader,
            content,
            &mut warnings,
        )
        .await;
        (result, warnings)
    }

    #[test]
    fn only_required_dependencies_with_a_valid_identifier_are_followed() {
        let dep = |kind: &str, field: &str, id: Value| json!({"dependency_type": kind, field: id});
        for kind in ["optional", "incompatible", "embedded"] {
            assert_eq!(required_dependency(&dep(kind, "project_id", json!("abc"))).unwrap(), None, "{kind}");
            // Ni siquiera se valida el identificador de las dependencias que no son obligatorias.
            assert_eq!(required_dependency(&dep(kind, "project_id", json!("a b!"))).unwrap(), None);
        }
        assert_eq!(required_dependency(&dep("required", "project_id", json!("P-r_o1"))).unwrap(), Some("P-r_o1".into()));
        // version_id tiene prioridad sobre project_id.
        let both = json!({"dependency_type": "required", "project_id": "proj", "version_id": "ver"});
        assert_eq!(required_dependency(&both).unwrap(), Some("version:ver".into()));
        // Un version_id inválido cae al project_id; sin ninguno válido, error.
        let fallback = json!({"dependency_type": "required", "project_id": "proj", "version_id": "../x"});
        assert_eq!(required_dependency(&fallback).unwrap(), Some("proj".into()));
        for bad in [json!("a b"), json!(""), json!("x".repeat(65)), json!(7), Value::Null] {
            assert!(required_dependency(&dep("required", "project_id", bad.clone())).is_err(), "{bad}");
        }
        // Sin tipo de dependencia no es obligatoria.
        assert_eq!(required_dependency(&json!({})).unwrap(), None);
        assert!(valid_project_id(&"x".repeat(64)) && !valid_project_id(&"x".repeat(65)));
    }

    #[tokio::test]
    async fn dependency_cycles_and_self_references_terminate() {
        let (root, mods) = mods_dir_with_lock(&[
            ("a", version("a", vec![required("b"), required("a")])),
            ("b", version("b", vec![required("a"), json!({"dependency_type": "required", "version_id": "vc"})])),
            ("version:vc", version("c", vec![required("b")])),
        ]);
        let (result, warnings) = sync(&mods, "fabric", &[modrinth("a")]).await;
        result.unwrap();
        assert!(warnings.is_empty());
        assert_eq!(fs::read_to_string(mods.join(".openclient_managed.json")).unwrap(), "[]");
        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn optional_dependencies_and_pinned_versions_do_not_need_the_network() {
        let optional = json!({"dependency_type": "optional", "project_id": "nunca"});
        let (root, mods) = mods_dir_with_lock(&[("version:vp", version("p", vec![optional]))]);
        let pinned = item("p", "Modrinth", "mod", true, Some("vp"));
        sync(&mods, "fabric", &[pinned]).await.0.unwrap();
        assert!(mods.join(".openclient_managed.json").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn dependency_queue_is_limited_to_256_entries() {
        let dependencies = |count: usize| (0..count).map(|n| required(&format!("d{n}"))).collect::<Vec<_>>();
        // Raíz + 255 dependencias = 256 entradas: se resuelve todo.
        let mut entries = vec![("root".to_string(), version("root", dependencies(255)))];
        entries.extend((0..255).map(|n| (format!("d{n}"), version(&format!("pd{n}"), vec![]))));
        let borrowed: Vec<(&str, Value)> = entries.iter().map(|(k, v)| (k.as_str(), v.clone())).collect();
        let (root, mods) = mods_dir_with_lock(&borrowed);
        sync(&mods, "fabric", &[modrinth("root")]).await.0.unwrap();
        assert!(mods.join(".openclient_managed.json").exists());
        fs::remove_dir_all(root).unwrap();

        // Una dependencia más: 257 entradas, se aborta sin tocar la carpeta.
        let (root, mods) = mods_dir_with_lock(&[("root", version("root", dependencies(256)))]);
        let (result, _) = sync(&mods, "fabric", &[modrinth("root")]).await;
        assert_eq!(result.unwrap_err(), "Demasiadas dependencias; revisa la selección de mods.");
        assert!(!mods.join(".openclient_managed.json").exists());
        fs::remove_dir_all(root).unwrap();

        // El límite también cuenta los mods elegidos por el usuario.
        let (root, mods) = mods_dir_with_lock(&[]);
        let many: Vec<LaunchMod> = (0..257).map(|n| modrinth(&format!("m{n}"))).collect();
        assert!(sync(&mods, "fabric", &many).await.0.unwrap_err().starts_with("Demasiadas dependencias"));
        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn disabled_and_non_mod_entries_are_not_resolved_and_stale_managed_jars_are_removed() {
        let (root, mods) = mods_dir_with_lock(&[]);
        fs::write(mods.join("viejo.jar"), b"gestionado").unwrap();
        fs::write(mods.join("usuario.jar"), b"propio").unwrap();
        fs::write(mods.join(".openclient_managed.json"), r#"["viejo.jar","ausente.jar"]"#).unwrap();
        let content = [
            item("desactivado", "modrinth", "mod", false, None),
            item("textura", "modrinth", "resourcepack", true, None),
            item("sombras", "modrinth", "shader", true, None),
        ];
        // Ninguna entrada está en el registro: si se resolviera alguna, intentaría usar la red.
        let (result, warnings) = sync(&mods, "fabric", &content).await;
        result.unwrap();
        assert!(warnings.is_empty());
        assert!(!mods.join("viejo.jar").exists());
        assert_eq!(fs::read(mods.join("usuario.jar")).unwrap(), b"propio");
        assert_eq!(fs::read_to_string(mods.join(".openclient_managed.json")).unwrap(), "[]");
        let leftovers = fs::read_dir(&root).unwrap().count();
        assert_eq!(leftovers, 1, "no deben quedar carpetas temporales");
        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn damaged_or_hostile_managed_lists_keep_the_current_installation() {
        for (manifest, expected) in [
            ("{no json", "La lista de mods guardada está dañada; se conserva la instalación anterior."),
            (r#"["../fuera.jar"]"#, "La lista de mods contiene una ruta inválida"),
            (r#"["/etc/passwd"]"#, "La lista de mods contiene una ruta inválida"),
        ] {
            let (root, mods) = mods_dir_with_lock(&[]);
            fs::write(mods.join("usuario.jar"), b"propio").unwrap();
            fs::write(mods.join(".openclient_managed.json"), manifest).unwrap();
            fs::write(root.join("fuera.jar"), b"ajeno").unwrap();
            let (result, _) = sync(&mods, "fabric", &[]).await;
            assert_eq!(result.unwrap_err(), expected);
            assert_eq!(fs::read(mods.join("usuario.jar")).unwrap(), b"propio");
            assert_eq!(fs::read_to_string(mods.join(".openclient_managed.json")).unwrap(), manifest);
            assert_eq!(fs::read(root.join("fuera.jar")).unwrap(), b"ajeno");
            fs::remove_dir_all(root).unwrap();
        }
    }

    #[tokio::test]
    async fn sources_that_cannot_be_downloaded_automatically_only_produce_warnings() {
        let (root, mods) = mods_dir_with_lock(&[]);
        let curseforge = |url: Option<&str>, file: Option<&str>| LaunchMod {
            download_url: url.map(String::from),
            file_name: file.map(String::from),
            ..item("cf1", "CurseForge", "mod", true, None)
        };
        let content = [
            item("manual", "otra-fuente", "mod", true, None),
            curseforge(Some("https://evil.example/a.jar"), Some("a.jar")),
            curseforge(Some("http://edge.forgecdn.net/a.jar"), Some("a.jar")),
            curseforge(Some("https://edge.forgecdn.net/a.txt"), Some("a.txt")),
            curseforge(None, None),
        ];
        let (result, warnings) = sync(&mods, "fabric", &content).await;
        result.unwrap();
        assert_eq!(warnings.len(), 5);
        assert_eq!(
            warnings[0],
            "'manual' es de otra-fuente y no se descarga automáticamente; colócalo en la carpeta mods."
        );
        assert!(warnings[1..].iter().all(|w| w.starts_with("'cf1' es de CurseForge")));
        assert!(!mods.join("a.jar").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn curseforge_jar_matching_a_local_file_is_neither_downloaded_nor_adopted() {
        let (root, mods) = mods_dir_with_lock(&[]);
        fs::write(mods.join("local.jar"), b"contenido").unwrap();
        let expected = hex(&Sha1::digest(b"contenido"));
        let direct = |sha1: &str, name: &str| LaunchMod {
            download_url: Some("https://edge.forgecdn.net/files/1/local.jar".into()),
            file_name: Some(name.into()),
            sha1: Some(sha1.into()),
            ..item("cf1", "curseforge", "mod", true, None)
        };
        // El nombre se reduce a su último componente: `../local.jar` es `local.jar`.
        let (result, warnings) = sync(&mods, "fabric", &[direct(&expected.to_uppercase(), "../local.jar")]).await;
        result.unwrap();
        assert!(warnings.is_empty());
        assert_eq!(fs::read(mods.join("local.jar")).unwrap(), b"contenido");
        assert_eq!(fs::read_to_string(mods.join(".openclient_managed.json")).unwrap(), "[]");
        // Otro contenido con el mismo nombre no se sobrescribe.
        let (result, _) = sync(&mods, "fabric", &[direct(&"0".repeat(40), "local.jar")]).await;
        assert_eq!(result.unwrap_err(), "El mod local.jar coincide con un archivo local distinto.");
        // Un SHA-1 de longitud incorrecta se ignora y el archivo local tampoco se pisa.
        let (result, _) = sync(&mods, "fabric", &[direct("1234", "local.jar")]).await;
        assert!(result.unwrap_err().contains("coincide con un archivo local distinto"));
        assert_eq!(fs::read(mods.join("local.jar")).unwrap(), b"contenido");
        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn resolved_versions_must_match_game_loader_and_project_rules() {
        let with = |field: &str, value: Value| {
            let mut v = version("a", vec![]);
            v[field] = value;
            v
        };
        // Quilt acepta versiones publicadas para Fabric; Forge no.
        let (root, mods) = mods_dir_with_lock(&[("a", version("a", vec![]))]);
        sync(&mods, "quilt", &[modrinth("a")]).await.0.unwrap();
        let error = sync(&mods, "forge", &[modrinth("a")]).await.0.unwrap_err();
        assert_eq!(error, "La versión fijada de a no es compatible con forge 1.21.4");
        fs::remove_dir_all(root).unwrap();

        let incompatible = "La versión fijada de a no es compatible con fabric 1.21.4";
        for (entry, expected) in [
            (with("game_versions", json!(["1.20.1"])), incompatible),
            (with("loaders", json!(["forge"])), incompatible),
            (with("game_versions", json!("1.21.4")), incompatible),
            (with("project_id", Value::Null), "Mod sin identificador de proyecto"),
            (with("id", json!(3)), "Mod sin identificador de versión"),
            (
                with("dependencies", json!([{"dependency_type": "required"}])),
                "Un mod tiene una dependencia obligatoria sin identificador válido; se conserva la instalación anterior.",
            ),
        ] {
            let (root, mods) = mods_dir_with_lock(&[("a", entry)]);
            assert_eq!(sync(&mods, "fabric", &[modrinth("a")]).await.0.unwrap_err(), expected);
            assert!(!mods.join(".openclient_managed.json").exists());
            fs::remove_dir_all(root).unwrap();
        }
    }

    #[tokio::test]
    async fn two_versions_of_the_same_project_are_reported_as_incompatible() {
        let mut other = version("shared", vec![]);
        other["id"] = json!("otra-version");
        let (root, mods) = mods_dir_with_lock(&[("a", version("shared", vec![])), ("b", other)]);
        let error = sync(&mods, "fabric", &[modrinth("a"), modrinth("b")]).await.0.unwrap_err();
        assert_eq!(error, "Dependencias incompatibles para shared. Revisa las versiones seleccionadas.");
        // La misma versión pedida dos veces por rutas distintas sí es coherente.
        let (root2, mods2) =
            mods_dir_with_lock(&[("a", version("shared", vec![])), ("b", version("shared", vec![]))]);
        sync(&mods2, "fabric", &[modrinth("a"), modrinth("b")]).await.0.unwrap();
        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(root2).unwrap();
    }

    #[tokio::test]
    async fn mod_identifiers_with_invalid_characters_are_never_requested() {
        let (root, mods) = mods_dir_with_lock(&[]);
        let content = [modrinth("../x"), modrinth("a b"), modrinth(""), modrinth(&"x".repeat(65))];
        sync(&mods, "fabric", &content).await.0.unwrap();
        fs::remove_dir_all(root).unwrap();
    }

    // sync_packs

    fn pack_entry(bytes: &[u8]) -> Value {
        json!({"url": "https://cdn.modrinth.com/data/p/versions/v/pack.zip", "sha1": hex(&Sha1::digest(bytes)), "size": bytes.len()})
    }

    async fn sync_game_packs(game: &Path, content: &[LaunchMod]) -> Result<(), String> {
        sync_packs(&context(game), &build_client().unwrap(), game, "1.21.4", content).await
    }

    #[test]
    fn pack_names_and_download_metadata_are_strictly_validated() {
        assert_eq!(pack_name("projA", "ver1").unwrap(), "OpenClient-projA-ver1.zip");
        let too_long = "x".repeat(33);
        for (project, version) in [("", "v"), ("p", ""), ("a-b", "v"), ("../x", "v"), ("p", "v.zip"), (too_long.as_str(), "v")] {
            assert!(pack_name(project, version).is_none(), "{project}/{version}");
        }
        assert!(pack_name(&"x".repeat(32), &"y".repeat(32)).is_some());
        assert!(managed_pack_name("OpenClient-projA-ver1.zip"));
        for name in ["usuario.zip", "OpenClient-solo.zip", "OpenClient-a-b-c.zip", "OpenClient-a-b.jar", "OpenClient-../x-y.zip", "OpenClient--v.zip"] {
            assert!(!managed_pack_name(name), "{name}");
        }
        let valid = |url: &str, sha1: &str, size: u64| PackFile { url: url.into(), sha1: sha1.into(), size }.valid();
        let sha = "a".repeat(40);
        let good = "https://cdn.modrinth.com/data/a/versions/b/c.zip";
        assert!(valid(good, &sha, 1));
        assert!(valid(good, &sha, 128 * 1024 * 1024));
        assert!(!valid(good, &sha, 0));
        assert!(!valid(good, &sha, 128 * 1024 * 1024 + 1));
        assert!(!valid(good, &"g".repeat(40), 1));
        assert!(!valid(good, &"a".repeat(39), 1));
        for url in [
            "http://cdn.modrinth.com/a.zip",
            "https://cdn.modrinth.com.evil.net/a.zip",
            "https://user@cdn.modrinth.com/a.zip",
            "https://cdn.modrinth.com:8443/a.zip",
            "https://github.com/a.zip",
            "no es una url",
        ] {
            assert!(!valid(url, &sha, 1), "{url}");
        }
    }

    #[tokio::test]
    async fn packs_already_on_disk_are_kept_and_stale_or_disabled_ones_are_removed() {
        let game = temp_dir("packs");
        let folder = game.join("resourcepacks");
        fs::create_dir_all(&folder).unwrap();
        let (kept, stale) = ("OpenClient-projA-verA.zip", "OpenClient-projB-verB.zip");
        fs::write(folder.join(kept), b"zip-a").unwrap();
        fs::write(folder.join(stale), b"zip-b").unwrap();
        fs::write(folder.join("usuario.zip"), b"mio").unwrap();
        let manifest = json!({kept: pack_entry(b"zip-a"), stale: pack_entry(b"zip-b")});
        fs::write(folder.join(".openclient-packs.json"), manifest.to_string()).unwrap();
        let content = [
            item("projA", "modrinth", "resourcepack", true, Some("verA")),
            item("projB", "modrinth", "resourcepack", false, Some("verB")),
            item("projC", "curseforge", "resourcepack", true, Some("verC")),
        ];
        sync_game_packs(&game, &content).await.unwrap();
        assert_eq!(fs::read(folder.join(kept)).unwrap(), b"zip-a");
        assert!(!folder.join(stale).exists());
        assert_eq!(fs::read(folder.join("usuario.zip")).unwrap(), b"mio");
        let saved: HashMap<String, PackFile> =
            serde_json::from_str(&fs::read_to_string(folder.join(".openclient-packs.json")).unwrap()).unwrap();
        assert_eq!(saved.len(), 1);
        assert!(saved.contains_key(kept));
        // Sin paquetes ni registro no se crea nada (shaderpacks no existía).
        assert!(!game.join("shaderpacks").exists());
        fs::remove_dir_all(game).unwrap();
    }

    #[tokio::test]
    async fn pack_registry_problems_fail_before_touching_the_network_or_the_folder() {
        let active = |id: &str, version: Option<&str>| item(id, "modrinth", "shader", true, version);
        let occupied = "Ya existe un archivo local con el nombre OpenClient-projA-verA.zip.";
        let tampered = json!({"OpenClient-projA-verA.zip": {"url": "https://evil.example/a.zip", "sha1": "a".repeat(40), "size": 3}});
        let cases = [
            (Some("{roto".to_string()), vec![], "El registro de paquetes está dañado."),
            (
                Some(json!({"evil.zip": pack_entry(b"x")}).to_string()),
                vec![],
                "El registro de paquetes contiene un nombre inválido.",
            ),
            (None, vec![active("projA", None)], "El paquete no tiene una versión asignada."),
            (None, vec![active("proj-A", Some("verA"))], "Identificador de paquete inválido."),
            (None, vec![active("projA", Some("ver A"))], "Identificador de paquete inválido."),
            // Un archivo local con el nombre gestionado no se pisa.
            (None, vec![active("projA", Some("verA"))], occupied),
            // Un registro manipulado (URL ajena) deja de ser fiable y cae en la misma comprobación.
            (Some(tampered.to_string()), vec![active("projA", Some("verA"))], occupied),
        ];
        for (manifest, content, expected) in cases {
            let game = temp_dir("packs-error");
            let folder = game.join("shaderpacks");
            fs::create_dir_all(&folder).unwrap();
            fs::write(folder.join("usuario.zip"), b"mio").unwrap();
            if expected == occupied {
                fs::write(folder.join("OpenClient-projA-verA.zip"), b"local").unwrap();
            }
            if let Some(text) = &manifest {
                fs::write(folder.join(".openclient-packs.json"), text).unwrap();
            }
            assert_eq!(sync_game_packs(&game, &content).await.unwrap_err(), expected);
            assert_eq!(fs::read(folder.join("usuario.zip")).unwrap(), b"mio");
            if let Some(text) = &manifest {
                assert_eq!(&fs::read_to_string(folder.join(".openclient-packs.json")).unwrap(), text);
            }
            assert_eq!(fs::read_dir(&game).unwrap().count(), 1, "sin carpetas temporales");
            fs::remove_dir_all(game).unwrap();
        }
    }

    // Puntos de entrada públicos

    #[tokio::test]
    async fn instance_content_sync_skips_mods_for_vanilla_and_reports_manual_downloads() {
        let root = temp_dir("instance-sync");
        let context = context(&root);
        let manual = [item("manual", "otra", "mod", true, None)];
        // Vanilla no usa carpeta de mods: ni se crea ni se avisa.
        sync_instance_content(&context, "inst", "1.21.4", "vanilla", &manual).await.unwrap();
        assert!(!root.join("instances").join("inst").join("mods").exists());
        // Con cargador, los avisos se convierten en error para la interfaz.
        let error = sync_instance_content(&context, "../inst", "1.21.4", "forge", &manual).await.unwrap_err();
        assert!(error.starts_with("'manual' es de otra"));
        assert!(root.join("instances").join("_inst").join("mods").is_dir());
        sync_instance_content(&context, "../inst", "1.21.4", "neoforge", &[]).await.unwrap();
        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn installing_instance_mods_validates_the_version_and_stays_in_the_instance_folder() {
        let root = temp_dir("install-mods");
        for bad in ["", "1.21/../x", "1.21 4"] {
            let error = install_instance_mods(context(&root), "inst".into(), bad.into(), vec![]).await.unwrap_err();
            assert_eq!(error, "Versión inválida", "{bad}");
        }
        let disabled = vec![item("x", "modrinth", "mod", false, None)];
        install_instance_mods(context(&root), "../../inst".into(), "1.21.4".into(), disabled).await.unwrap();
        let folder = root.join("instances").join(sanitize_component("../../inst")).join("mods");
        assert!(folder.is_dir());
        assert!(folder.starts_with(root.join("instances")));
        fs::remove_dir_all(root).unwrap();
    }

    // Versión fijada de cargadores

    #[test]
    fn pinned_loader_urls_and_cache_names_are_exact() {
        assert_eq!(
            quilt_profile_url("1.21.4", "0.27.1"),
            "https://meta.quiltmc.org/v3/versions/loader/1.21.4/0.27.1/profile/json"
        );
        assert_eq!(
            forge_installer_url_pinned("1.20.1", "47.2.0"),
            "https://maven.minecraftforge.net/net/minecraftforge/forge/1.20.1-47.2.0/forge-1.20.1-47.2.0-installer.jar"
        );
        assert_eq!(
            neoforge_installer_url_pinned("21.1.5"),
            "https://maven.neoforged.net/releases/net/neoforged/neoforge/21.1.5/neoforge-21.1.5-installer.jar"
        );
        assert_eq!(forge_cache_name("forge", "1.20.1", None), "forge-1.20.1.json");
        assert_eq!(forge_cache_name("neoforge", "1.21.1", Some("21.1.5")), "neoforge-1.21.1-21.1.5.json");
        assert_eq!(pinned_loader_version("quilt", None).unwrap(), None);
        assert_eq!(pinned_loader_version("forge", Some("47.2.0")).unwrap(), Some("47.2.0"));
    }

    #[tokio::test]
    async fn invalid_pinned_loader_versions_are_rejected_before_any_request() {
        let client = build_client().unwrap();
        let root = temp_dir("pinned-invalid");
        let too_long = "1".repeat(49);
        for bad in ["", "../../x", "v1", "1 2", "1/2", "1?x=y", too_long.as_str()] {
            let quilt = load_quilt_profile(&client, "1.21.4", Path::new("quilt.json"), Some(bad)).await;
            assert_eq!(quilt.unwrap_err(), "Versión de Quilt inválida.", "{bad}");
            for (loader, name) in [("forge", "Forge"), ("neoforge", "NeoForge")] {
                let error = load_forge_profile(
                    &client,
                    &root,
                    &root.join("java.exe"),
                    "1.21.4",
                    loader,
                    &root.join("perfil.json"),
                    Some(bad),
                    &CancelToken::default(),
                )
                .await
                .unwrap_err();
                assert_eq!(error, format!("Versión de {name} inválida."), "{loader} {bad}");
            }
        }
        // Nada se escribió en disco durante el rechazo.
        assert_eq!(fs::read_dir(&root).unwrap().count(), 0);
        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn launch_rejects_an_invalid_pinned_version_before_the_network_and_accepts_the_old_field_name() {
        let root = temp_dir("launch-pinned");
        let request = |loader: &str, field: &str, version: &str| -> LaunchRequest {
            serde_json::from_value(json!({
                "instance_id": "x", "mc_version": "1.21.4", "loader": loader, field: version,
                "account": {"username": "a", "uuid": "u", "account_type": "offline", "access_token": ""},
                "optimization": {}
            }))
            .unwrap()
        };
        let refresh = |_: String| async { Err::<crate::identity::MinecraftAccount, String>("no".into()) };
        for (loader, name) in [("fabric", "Fabric"), ("quilt", "Quilt"), ("forge", "Forge"), ("neoforge", "NeoForge")] {
            let error = launch_game(context(&root), request(loader, "loader_version", "../x"), refresh)
                .await
                .unwrap_err();
            assert_eq!(error, format!("Versión de {name} inválida."));
        }
        // El nombre anterior del campo sigue aceptándose.
        let legacy = request("fabric", "fabric_version", "0.16.9");
        assert_eq!(legacy.loader_version.as_deref(), Some("0.16.9"));
        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn installed_profile_search_requires_the_pinned_version() {
        let root = temp_dir("pinned-installed");
        let cache = root.join("cache.json");
        let write_profile = |dir: &str, id: &str| {
            fs::create_dir_all(root.join("versions").join(dir)).unwrap();
            fs::write(root.join("versions").join(dir).join(format!("{dir}.json")), format!(r#"{{"id":"{id}"}}"#)).unwrap();
        };
        write_profile("1.20.1-forge-47.1.0", "vieja");
        assert!(read_installed_profile(&root, "forge", "1.20.1", Some("47.2.0"), &cache).await.is_none());
        write_profile("1.20.1-forge-47.2.0", "fijada");
        let found = read_installed_profile(&root, "forge", "1.20.1", Some("47.2.0"), &cache).await.unwrap();
        assert_eq!(found["id"], "fijada");
        fs::remove_dir_all(root).unwrap();
    }

    // Cancelación

    fn build_test_client() -> reqwest::Client {
        reqwest::Client::builder().timeout(Duration::from_secs(30)).build().unwrap()
    }

    /// Servidor local: responde con cabeceras y un trozo de cuerpo y no termina nunca.
    async fn stalled_server() -> u16 {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            while let Ok((mut socket, _)) = listener.accept().await {
                tokio::spawn(async move {
                    use tokio::io::{AsyncReadExt, AsyncWriteExt};
                    let mut buffer = [0u8; 2048];
                    let _ = socket.read(&mut buffer).await;
                    let _ = socket
                        .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 1000000\r\n\r\ncuerpo-parcial")
                        .await;
                    std::future::pending::<()>().await;
                });
            }
        });
        port
    }

    /// Servidor local que acepta conexiones y no responde nunca.
    async fn silent_server() -> u16 {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            while let Ok((socket, _)) = listener.accept().await {
                tokio::spawn(async move {
                    let _socket = socket;
                    std::future::pending::<()>().await;
                });
            }
        });
        port
    }

    /// Servidor local que entrega siempre `body` completo.
    async fn fixed_server(body: &'static [u8]) -> u16 {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            while let Ok((mut socket, _)) = listener.accept().await {
                tokio::spawn(async move {
                    use tokio::io::{AsyncReadExt, AsyncWriteExt};
                    let mut buffer = [0u8; 2048];
                    let _ = socket.read(&mut buffer).await;
                    let head = format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
                    let _ = socket.write_all(head.as_bytes()).await;
                    let _ = socket.write_all(body).await;
                    let _ = socket.shutdown().await;
                });
            }
        });
        port
    }

    fn files_with_suffix(dir: &Path, suffix: &str) -> Vec<PathBuf> {
        fs::read_dir(dir)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| path.to_string_lossy().ends_with(suffix))
            .collect()
    }

    #[tokio::test]
    async fn streamed_download_verifies_size_and_sha1_and_leaves_no_part_file() {
        let dir = temp_dir("download-ok");
        let port = fixed_server(b"hello").await;
        let client = build_test_client();
        let task = |name: &str, sha1: &str, size: u64| DlTask {
            url: format!("http://127.0.0.1:{port}/a"),
            dest: dir.join(name),
            sha1: Some(sha1.into()),
            size: Some(size),
        };
        let good = hex(&Sha1::digest(b"hello"));
        download(&client, &task("ok.bin", &good, 5), &CancelToken::default()).await.unwrap();
        assert_eq!(fs::read(dir.join("ok.bin")).unwrap(), b"hello");
        // SHA-1 o tamaño incorrectos: error y ni destino ni `.part`.
        let bad_sha = download(&client, &task("sha.bin", &"0".repeat(40), 5), &CancelToken::default()).await;
        assert!(bad_sha.unwrap_err().contains("SHA-1 inválido"));
        let bad_size = download(&client, &task("size.bin", &good, 4), &CancelToken::default()).await;
        assert!(bad_size.unwrap_err().contains("Tamaño inválido"));
        assert!(!dir.join("sha.bin").exists() && !dir.join("size.bin").exists());
        assert!(files_with_suffix(&dir, ".part").is_empty());
        fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn cancelling_run_downloads_returns_cancelled_and_leaves_no_part_files() {
        let dir = temp_dir("download-cancel");
        let port = stalled_server().await;
        let token = CancelToken::default();
        let app = context(&dir).with_cancel(token.clone());
        let tasks: Vec<DlTask> = (0..3)
            .map(|n| DlTask {
                url: format!("http://127.0.0.1:{port}/{n}"),
                dest: dir.join(format!("file{n}.bin")),
                sha1: None,
                size: None,
            })
            .collect();
        let worker = tokio::spawn(async move {
            run_downloads(&app, &build_test_client(), tasks, "mods", 0, 100, "Descargando").await
        });
        // Esperar a que haya descargas en curso (archivos `.part` ya creados).
        let started = std::time::Instant::now();
        while files_with_suffix(&dir, ".part").is_empty() {
            assert!(started.elapsed() < Duration::from_secs(10), "no empezó la descarga");
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        token.cancel();
        let result = tokio::time::timeout(Duration::from_secs(2), worker)
            .await
            .expect("la cancelación debe ser inmediata")
            .unwrap();
        assert_eq!(result.unwrap_err(), CANCELLED);
        assert!(files_with_suffix(&dir, ".part").is_empty());
        assert!((0..3).all(|n| !dir.join(format!("file{n}.bin")).exists()));
        fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn an_already_cancelled_token_stops_downloads_before_touching_disk() {
        let dir = temp_dir("download-precancelled");
        let token = CancelToken::default();
        token.cancel();
        let task = DlTask {
            url: "http://127.0.0.1:1/nada".into(),
            dest: dir.join("sub").join("a.bin"),
            sha1: None,
            size: None,
        };
        assert_eq!(download(&build_test_client(), &task, &token).await.unwrap_err(), CANCELLED);
        let app = context(&dir).with_cancel(token);
        let error = run_downloads(&app, &build_test_client(), vec![task], "mods", 0, 100, "x").await;
        assert_eq!(error.unwrap_err(), CANCELLED);
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 0);
        fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn cancelled_sync_mods_leaves_the_mods_folder_unchanged() {
        let snapshot = |mods: &Path| {
            let mut names: Vec<(String, Vec<u8>)> = fs::read_dir(mods)
                .unwrap()
                .map(|entry| {
                    let path = entry.unwrap().path();
                    (path.file_name().unwrap().to_string_lossy().into_owned(), fs::read(&path).unwrap())
                })
                .collect();
            names.sort();
            names
        };
        // Token ya cancelado: ni siquiera se crea el staging.
        let (root, mods) = mods_dir_with_lock(&[("a", version("a", vec![]))]);
        fs::write(mods.join("usuario.jar"), b"propio").unwrap();
        fs::write(mods.join("viejo.jar"), b"gestionado").unwrap();
        fs::write(mods.join(".openclient_managed.json"), r#"["viejo.jar"]"#).unwrap();
        let before = snapshot(&mods);
        let token = CancelToken::default();
        token.cancel();
        let app = context(&root).with_cancel(token);
        let mut warnings = Vec::new();
        let result = sync_mods(&app, &build_client().unwrap(), &mods, "1.21.4", "fabric", &[modrinth("a")], &mut warnings).await;
        assert_eq!(result.unwrap_err(), CANCELLED);
        assert_eq!(snapshot(&mods), before);
        assert_eq!(fs::read_dir(&root).unwrap().count(), 1, "sin carpetas temporales");
        fs::remove_dir_all(root).unwrap();

        // Cancelación en mitad de una descarga: el staging ya existe y se descarta.
        let port = silent_server().await;
        let mut resolved = version("p", vec![]);
        resolved["files"] = json!([{
            "filename": "p.jar", "primary": true, "size": 10,
            "url": format!("https://127.0.0.1:{port}/p.jar"),
            "hashes": {"sha1": "a".repeat(40)}
        }]);
        let (root, mods) = mods_dir_with_lock(&[("p", resolved)]);
        fs::write(mods.join("usuario.jar"), b"propio").unwrap();
        fs::write(mods.join("viejo.jar"), b"gestionado").unwrap();
        fs::write(mods.join(".openclient_managed.json"), r#"["viejo.jar"]"#).unwrap();
        let before = snapshot(&mods);
        let token = CancelToken::default();
        let app = context(&root).with_cancel(token.clone());
        let worker = {
            let mods = mods.clone();
            tokio::spawn(async move {
                let mut warnings = Vec::new();
                sync_mods(&app, &build_client().unwrap(), &mods, "1.21.4", "fabric", &[modrinth("p")], &mut warnings).await
            })
        };
        let started = std::time::Instant::now();
        while fs::read_dir(&root).unwrap().count() < 2 {
            assert!(started.elapsed() < Duration::from_secs(10), "no se creó el staging");
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        token.cancel();
        let result = tokio::time::timeout(Duration::from_secs(2), worker).await.expect("cancelación inmediata").unwrap();
        assert_eq!(result.unwrap_err(), CANCELLED);
        assert_eq!(snapshot(&mods), before);
        assert_eq!(fs::read_dir(&root).unwrap().count(), 1, "el staging se borra");
        fs::remove_dir_all(root).unwrap();
    }
}
