//! Detección e instalación automática de Java.
//!
//! Orden de búsqueda:
//!   1. Runtime aislado del launcher: directorio de datos del launcher / runtimes / java-<major>
//!   2. Java del sistema (JAVA_HOME / PATH) si es compatible con la versión de Minecraft.
//!   3. Descarga automática desde Adoptium o Azul (verificada con SHA-256 y TLS estricto).

use crate::GameContext;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct JavaStatus {
    pub required_major: u8,
    pub found: bool,
    pub installed_major: Option<u8>,
    /// "launcher" | "system" | "downloaded" | "missing"
    pub source: String,
    pub java_path: Option<String>,
}

#[derive(Debug, Serialize, Clone)]
struct JavaProgressEvent {
    stage: String,
    percent: u8,
    message: String,
}

#[derive(Debug, Deserialize)]
struct AdoptiumAsset {
    binary: AdoptiumBinary,
}

#[derive(Debug, Deserialize)]
struct AdoptiumBinary {
    package: JavaPackage,
}

#[derive(Debug, Deserialize)]
struct JavaPackage {
    #[serde(alias = "download_url")]
    link: String,
    #[serde(alias = "sha256_hash")]
    checksum: String,
    name: String,
}

#[cfg(target_os = "windows")]
const JAVA_EXE: &str = "javaw.exe";
#[cfg(not(target_os = "windows"))]
const JAVA_EXE: &str = "java";

#[cfg(windows)]
const JAVA_ARCHIVE_SUFFIX: &str = ".zip";
#[cfg(not(windows))]
const JAVA_ARCHIVE_SUFFIX: &str = ".tar.gz";

/// Versión mayor de Java que exige una versión de Minecraft.
pub fn required_java_major(mc_version: &str) -> u8 {
    let mut parts = mc_version
        .split(|c: char| !c.is_ascii_digit())
        .filter(|s| !s.is_empty())
        .map(|s| s.parse::<u32>().unwrap_or(0));

    let major = parts.next().unwrap_or(1);
    let minor = parts.next().unwrap_or(0);
    let patch = parts.next().unwrap_or(0);

    if major != 1 {
        return 21;
    }
    if minor >= 21 || (minor == 20 && patch >= 5) {
        21
    } else if minor >= 18 {
        17
    } else if minor == 17 {
        16
    } else {
        8
    }
}

fn is_compatible(installed: u8, required: u8) -> bool {
    if required <= 8 {
        installed == 8
    } else {
        installed == required
    }
}

fn launcher_runtimes_dir(root: &Path) -> PathBuf {
    root.join("runtimes")
}

fn runtime_dir_for(root: &Path, major: u8) -> PathBuf {
    launcher_runtimes_dir(root).join(format!("java-{}", major))
}

fn java_exe_in(home: &Path) -> PathBuf {
    home.join("bin").join(JAVA_EXE)
}

fn managed_java_exe(runtime: &Path) -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        let bundle_java = java_exe_in(&runtime.join("Contents/Home"));
        if bundle_java.is_file() {
            return bundle_java;
        }
        // Los paquetes antiguos de Azul incluyen el bundle .jdk dentro de la carpeta raíz.
        if let Ok(entries) = std::fs::read_dir(runtime) {
            for path in entries.filter_map(Result::ok).map(|entry| entry.path()) {
                if path.extension().is_some_and(|ext| ext == "jdk") {
                    let executable = java_exe_in(&path.join("Contents/Home"));
                    if executable.is_file() {
                        return executable;
                    }
                }
            }
        }
    }
    java_exe_in(runtime)
}

fn probe_java(java_path: &Path, args: &[&str]) -> Option<String> {
    let mut cmd = std::process::Command::new(java_path);
    cmd.args(args);

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }

    cmd.stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let mut child = cmd.spawn().ok()?;
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if std::time::Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(50))
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
    let output = child.wait_with_output().ok()?;
    Some(format!(
        "{}{}",
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    ))
}

fn probe_candidate(exe: &Path) -> Option<u8> {
    let text = probe_java(&console_executable(exe), &["-version"])?;
    let start = text.find("version \"")? + "version \"".len();
    let rest = &text[start..];
    let end = rest.find('"')?;
    let raw = &rest[..end]; // "21.0.5" | "17.0.13" | "1.8.0_432"

    let mut it = raw.split(['.', '_', '-', '+']);
    let first: u8 = it.next()?.parse().ok()?;
    if first == 1 {
        it.next()?.parse().ok()
    } else {
        Some(first)
    }
}

/// `javaw.exe` no imprime en consola; se usa el `java.exe` hermano cuando existe.
pub(crate) fn console_executable(exe: &Path) -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        let console = exe.with_file_name("java.exe");
        if console.exists() {
            return console;
        }
    }
    exe.to_path_buf()
}

pub(crate) fn architecture(exe: &Path) -> Option<&'static str> {
    let output = probe_java(exe, &["-XshowSettings:properties", "-version"])?;
    let arch = output.lines().find_map(|line| {
        let (key, value) = line.trim().split_once('=')?;
        (key.trim() == "os.arch").then_some(value.trim())
    })?;
    match arch {
        "amd64" | "x64" | "x86_64" => Some("x86_64"),
        "arm64" | "aarch64" => Some("aarch64"),
        "x86" | "i386" | "i686" => Some("x86"),
        "arm" => Some("arm"),
        _ => None,
    }
}

fn find_launcher_java(root: &Path, required: u8) -> Option<(PathBuf, u8)> {
    let exe = managed_java_exe(&runtime_dir_for(root, required));
    if exe.exists() {
        let major = probe_candidate(&exe)?;
        if is_compatible(major, required) {
            return Some((exe, major));
        }
    }
    None
}

fn find_system_java(required: u8) -> Option<(PathBuf, u8)> {
    let mut candidates: Vec<PathBuf> = Vec::new();

    if let Ok(home) = std::env::var("JAVA_HOME") {
        candidates.push(java_exe_in(Path::new(&home)));
    }
    #[cfg(target_os = "macos")]
    {
        let version = if required == 8 { "1.8".to_owned() } else { required.to_string() };
        if let Ok(output) = std::process::Command::new("/usr/libexec/java_home").args(["-F", "-v", &version]).output() {
            if output.status.success() {
                candidates.push(java_exe_in(Path::new(String::from_utf8_lossy(&output.stdout).trim())));
            }
        }
    }
    #[cfg(target_os = "linux")]
    if let Ok(homes) = std::fs::read_dir("/usr/lib/jvm") {
        candidates.extend(homes.filter_map(Result::ok).map(|home| java_exe_in(&home.path())));
    }

    if let Ok(path_var) = std::env::var("PATH") {
        for dir in std::env::split_paths(&path_var) {
            let exe = dir.join(JAVA_EXE);
            // macOS incluye un ejecutable que solo abre un aviso si Java no está instalado.
            if exe.exists() && !(cfg!(target_os = "macos") && exe == Path::new("/usr/bin/java")) {
                candidates.push(exe);
            }
        }
    }

    for exe in candidates {
        if !exe.exists() {
            continue;
        }
        if let Some(major) = probe_candidate(&exe) {
            if is_compatible(major, required) {
                return Some((exe, major));
            }
        }
    }
    None
}

fn status_missing(required: u8) -> JavaStatus {
    JavaStatus {
        required_major: required,
        found: false,
        installed_major: None,
        source: "missing".into(),
        java_path: None,
    }
}

fn emit_progress(app: &GameContext, stage: &str, percent: u8, message: &str) {
    let _ = app.emit(
        "java-progress",
        JavaProgressEvent {
            stage: stage.to_string(),
            percent,
            message: message.to_string(),
        },
    );
}

fn locate(root: &Path, required: u8) -> JavaStatus {
    if let Some((path, major)) = find_launcher_java(root, required) {
        return JavaStatus {
            required_major: required,
            found: true,
            installed_major: Some(major),
            source: "launcher".into(),
            java_path: Some(path.to_string_lossy().to_string()),
        };
    }
    if let Some((path, major)) = find_system_java(required) {
        return JavaStatus {
            required_major: required,
            found: true,
            installed_major: Some(major),
            source: "system".into(),
            java_path: Some(path.to_string_lossy().to_string()),
        };
    }
    status_missing(required)
}

/// Verifica si hay un Java compatible con `mc_version` sin descargar nada.
pub async fn check_java_runtime(root: PathBuf, mc_version: String) -> Result<JavaStatus, String> {
    let required = required_java_major(&mc_version);
    tokio::task::spawn_blocking(move || locate(&root, required))
        .await
        .map_err(|e| format!("Error verificando Java: {}", e))
}

async fn resolve_adoptium_package(
    client: &reqwest::Client,
    required: u8,
) -> Result<JavaPackage, String> {
    // Some supported Minecraft versions need an archived Java for which Adoptium
    // publishes a JDK but no standalone JRE (notably Java 16 / Minecraft 1.17).
    let os = if cfg!(target_os = "macos") { "mac" } else { std::env::consts::OS };
    let arch = match std::env::consts::ARCH {
        "x86_64" => "x64",
        "aarch64" => "aarch64",
        _ => return Err("Adoptium no ofrece Java para esta arquitectura. Selecciona un Java instalado.".into()),
    };
    for image_type in ["jre", "jdk"] {
        let url = format!("https://api.adoptium.net/v3/assets/latest/{required}/hotspot?architecture={arch}&image_type={image_type}&os={os}&vendor=eclipse");
        let response = client
            .get(url)
            .send()
            .await
            .map_err(|e| format!("No se pudo contactar a Adoptium: {}", e))?;
        if matches!(
            response.status(),
            reqwest::StatusCode::NO_CONTENT | reqwest::StatusCode::NOT_FOUND
        ) {
            continue;
        }
        let assets: Vec<AdoptiumAsset> = response
            .error_for_status()
            .map_err(|e| format!("Adoptium respondió con error: {}", e))?
            .json()
            .await
            .map_err(|e| format!("Respuesta inválida de Adoptium: {}", e))?;
        if let Some(package) = assets
            .into_iter()
            .map(|asset| asset.binary.package)
            .find(|package| package.name.to_lowercase().ends_with(JAVA_ARCHIVE_SUFFIX))
        {
            return Ok(package);
        }
    }
    Err(format!(
        "Adoptium no ofrece Java {required} para {os} {arch}. Selecciona un Java compatible en Ajustes."
    ))
}

async fn resolve_zulu_mac_package(
    client: &reqwest::Client,
    required: u8,
) -> Result<JavaPackage, String> {
    #[derive(Deserialize)]
    struct ZuluBuild {
        package_uuid: String,
        name: String,
    }

    let (arch, suffix) = if cfg!(target_arch = "aarch64") {
        ("arm", "-macosx_aarch64.tar.gz")
    } else {
        ("x86", "-macosx_x64.tar.gz")
    };
    let api_url = "https://api.azul.com/metadata/v1/zulu/packages/";
    let url = format!("{api_url}?java_version={required}&os=macos&arch={arch}&hw_bitness=64&archive_type=tar.gz&java_package_type=jre&javafx_bundled=false&availability_types=ca&release_status=ga&latest=true");
    let builds: Vec<ZuluBuild> = client
        .get(url)
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(|error| format!("No se pudo consultar Java en Azul: {error}"))?
        .json()
        .await
        .map_err(|error| format!("Respuesta inválida de Azul: {error}"))?;
    let build = builds
        .into_iter()
        .find(|build| build.name.ends_with(suffix))
        .ok_or_else(|| format!("Azul no ofrece Java {required} para este Mac. Selecciona un Java compatible en Ajustes."))?;
    let id = uuid::Uuid::parse_str(&build.package_uuid)
        .map_err(|_| "Identificador de paquete de Azul inválido")?;
    let package: JavaPackage = client
        .get(format!("{api_url}{id}"))
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(|error| format!("No se pudo consultar el paquete de Azul: {error}"))?
        .json()
        .await
        .map_err(|error| format!("Paquete de Azul inválido: {error}"))?;
    if package.name != build.name || !package.link.starts_with("https://cdn.azul.com/zulu/bin/") {
        return Err("El archivo de Java indicado por Azul no coincide con el paquete solicitado.".into());
    }
    Ok(package)
}

async fn download_and_install(app: &GameContext, required: u8) -> Result<PathBuf, String> {
    let client = reqwest::Client::builder()
        .https_only(true)
        .timeout(Duration::from_secs(600))
        .connect_timeout(Duration::from_secs(20))
        .user_agent("OpenClient/2.0")
        .build()
        .map_err(|e| e.to_string())?;

    app.cancel.check()?;
    emit_progress(
        app,
        "java-lookup",
        2,
        &format!("Buscando Java {required} para este sistema..."),
    );

    // Temurin no publica Java 16 para Mac ni Java 8 nativo para Apple Silicon.
    let package = if cfg!(target_os = "macos")
        && (required == 16 || (required == 8 && cfg!(target_arch = "aarch64")))
    {
        resolve_zulu_mac_package(&client, required).await?
    } else {
        resolve_adoptium_package(&client, required).await?
    };

    // Solo aceptamos paquetes oficiales con una suma SHA-256 publicada.
    let link_ok = package.link.starts_with("https://github.com/adoptium/")
        || package.link.starts_with("https://api.adoptium.net/")
        || package.link.starts_with("https://cdn.azul.com/zulu/bin/");
    if !link_ok || package.checksum.len() != 64 || !package.checksum.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("El paquete de Java no tiene una URL oficial y una suma SHA-256 válidas.".into());
    }

    let root = &app.root;
    let runtimes = launcher_runtimes_dir(root);
    std::fs::create_dir_all(&runtimes)
        .map_err(|e| format!("No se pudo crear carpeta de runtimes: {}", e))?;
    let archive_path = runtimes.join(format!("java-{required}{JAVA_ARCHIVE_SUFFIX}.part"));
    struct Download(PathBuf);
    impl Drop for Download {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }
    let _download = Download(archive_path.clone());

    let resp = client
        .get(&package.link)
        .send()
        .await
        .map_err(|e| format!("Error descargando Java: {}", e))?
        .error_for_status()
        .map_err(|e| format!("Descarga de Java rechazada: {}", e))?;

    let total = resp.content_length().unwrap_or(0);
    let mut downloaded: u64 = 0;
    let mut hasher = Sha256::new();
    let mut file = std::fs::File::create(&archive_path)
        .map_err(|e| format!("No se pudo crear archivo temporal: {}", e))?;
    let mut stream = resp.bytes_stream();
    let mut last_pct: u8 = 0;

    while let Some(chunk) = stream.next().await {
        app.cancel.check()?;
        let bytes =
            chunk.map_err(|e| format!("Conexión interrumpida durante la descarga: {}", e))?;
        hasher.update(&bytes);
        file.write_all(&bytes)
            .map_err(|e| format!("Error escribiendo Java en disco: {}", e))?;
        downloaded += bytes.len() as u64;

        if total > 0 {
            let pct = 5 + ((downloaded as f64 / total as f64) * 70.0) as u8; // 5..75
            if pct != last_pct {
                last_pct = pct;
                emit_progress(
                    app,
                    "java-download",
                    pct,
                    &format!(
                        "Descargando Java {} ({:.1} / {:.1} MB)...",
                        required,
                        downloaded as f64 / 1_048_576.0,
                        total as f64 / 1_048_576.0
                    ),
                );
            }
        }
    }
    file.flush().map_err(|e| e.to_string())?;
    drop(file);

    emit_progress(
        app,
        "java-verify",
        78,
        "Verificando integridad (SHA-256)...",
    );
    let digest = hasher.finalize();
    let actual: String = digest.iter().map(|b| format!("{:02x}", b)).collect();
    if !actual.eq_ignore_ascii_case(package.checksum.trim()) {
        return Err(
            "La descarga de Java está corrupta (SHA-256 no coincide). Intenta de nuevo.".into(),
        );
    }

    emit_progress(app, "java-extract", 82, "Instalando Java en el launcher...");
    let target = runtime_dir_for(root, required);
    let staged = crate::directory_install::PreparedDirectory::new(&target, false)?;
    app.cancel.check()?;
    let archive_for_task = archive_path.clone();
    let target_for_task = staged.path.clone();
    tokio::task::spawn_blocking(move || extract_java_archive(&archive_for_task, &target_for_task))
        .await
        .map_err(|e| format!("Error en extracción: {}", e))??;

    let staged_exe = managed_java_exe(&staged.path);
    if !staged_exe.exists() {
        return Err("Java se extrajo pero no se encontró su ejecutable.".into());
    }
    let probe_path = staged_exe.clone();
    let major = tokio::task::spawn_blocking(move || probe_candidate(&probe_path))
        .await
        .map_err(|e| e.to_string())?;
    if !major.map(|m| is_compatible(m, required)).unwrap_or(false) {
        return Err(
            "El Java descargado no pasó la verificación; se conserva la instalación anterior."
                .into(),
        );
    }
    app.cancel.check()?;
    staged.commit()?;
    Ok(managed_java_exe(&target))
}

#[cfg(unix)]
fn extract_java_archive(archive: &Path, target: &Path) -> Result<(), String> {
    // Solo recibe el paquete oficial después de comprobar su SHA-256.
    // tar conserva los permisos de ejecución y enlaces relativos del runtime.
    std::fs::create_dir_all(target).map_err(|error| error.to_string())?;
    let status = std::process::Command::new("/usr/bin/tar")
        .arg("-xzf")
        .arg(archive)
        .arg("--strip-components=1")
        .arg("-C")
        .arg(target)
        .status()
        .map_err(|error| format!("No se pudo ejecutar tar: {error}"))?;
    if !status.success() {
        return Err("No se pudo extraer el paquete de Java.".into());
    }
    Ok(())
}

/// Extrae el .zip quitando la carpeta raíz (p. ej. `jdk-21.0.5+11-jre/`) y
/// bloqueando rutas maliciosas (zip-slip).
#[cfg(target_os = "windows")]
fn extract_java_archive(zip_path: &Path, target: &Path) -> Result<(), String> {
    let file = std::fs::File::open(zip_path).map_err(|e| e.to_string())?;
    let mut archive =
        zip::ZipArchive::new(file).map_err(|e| format!("ZIP de Java inválido: {}", e))?;

    if target.exists() {
        std::fs::remove_dir_all(target)
            .map_err(|e| format!("No se pudo limpiar instalación previa: {}", e))?;
    }
    std::fs::create_dir_all(target).map_err(|e| e.to_string())?;

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
        let enclosed = match entry.enclosed_name() {
            Some(p) => p.to_path_buf(),
            None => continue, // ruta insegura: se ignora
        };

        let mut comps = enclosed.components();
        comps.next(); // quitar carpeta raíz
        let relative: PathBuf = comps.collect();
        if relative.as_os_str().is_empty() {
            continue;
        }

        let out_path = target.join(&relative);
        if !out_path.starts_with(target) {
            continue;
        }

        if entry.is_dir() {
            std::fs::create_dir_all(&out_path).map_err(|e| e.to_string())?;
        } else {
            if let Some(parent) = out_path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            let mut out = std::fs::File::create(&out_path).map_err(|e| e.to_string())?;
            std::io::copy(&mut entry, &mut out).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// Garantiza que exista un Java compatible; si no, lo descarga e instala.
/// Emite eventos `java-progress` { stage, percent, message } al frontend.
pub async fn ensure_java_runtime(
    app: GameContext,
    mc_version: String,
) -> Result<JavaStatus, String> {
    ensure_java_major(app, required_java_major(&mc_version), None).await
}

pub async fn ensure_java_major(
    app: GameContext,
    required: u8,
    custom_path: Option<String>,
) -> Result<JavaStatus, String> {
    static INSTALL_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
    let _guard = INSTALL_LOCK.lock().await;
    crate::directory_install::recover(&runtime_dir_for(&app.root, required))?;
    if let Some(path) = custom_path.filter(|p| !p.trim().is_empty()) {
        return check_java_major(app.root.clone(), required, Some(path)).await;
    }
    emit_progress(
        &app,
        "java-check",
        1,
        &format!("Verificando Java {}...", required),
    );
    let root = app.root.clone();
    let existing = tokio::task::spawn_blocking(move || locate(&root, required))
        .await
        .map_err(|e| format!("Error verificando Java: {}", e))?;

    if existing.found {
        emit_progress(
            &app,
            "java-ready",
            100,
            &format!("Java {} listo.", required),
        );
        return Ok(existing);
    }

    let exe = download_and_install(&app, required).await?;

    let exe_for_probe = exe.clone();
    let installed_major = tokio::task::spawn_blocking(move || probe_candidate(&exe_for_probe))
        .await
        .ok()
        .flatten();

    if !installed_major
        .map(|m| is_compatible(m, required))
        .unwrap_or(false)
    {
        return Err("Java se instaló pero no pasó la verificación de versión.".into());
    }

    emit_progress(
        &app,
        "java-ready",
        100,
        &format!("Java {} instalado correctamente.", required),
    );
    Ok(JavaStatus {
        required_major: required,
        found: true,
        installed_major,
        source: "downloaded".into(),
        java_path: Some(exe.to_string_lossy().to_string()),
    })
}

pub async fn check_java_major(
    root: PathBuf,
    major: u8,
    custom_path: Option<String>,
) -> Result<JavaStatus, String> {
    if !matches!(major, 8 | 16 | 17 | 21 | 25) {
        return Err("Versión de Java no admitida en este panel".into());
    }
    tokio::task::spawn_blocking(move || {
        if let Some(raw) = custom_path.filter(|p| !p.trim().is_empty()) {
            let path = PathBuf::from(raw.trim())
                .canonicalize()
                .map_err(|_| "No se encontró el ejecutable Java")?;
            let name = path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            if !matches!(name.as_str(), "java.exe" | "javaw.exe" | "java") {
                return Err("Selecciona java.exe, javaw.exe o java".into());
            }
            let installed =
                probe_candidate(&path).ok_or("No se pudo obtener la versión de Java")?;
            if !is_compatible(installed, major) {
                return Err(format!(
                    "Se necesita Java {}, pero el ejecutable es Java {}",
                    major, installed
                ));
            }
            Ok(JavaStatus {
                required_major: major,
                found: true,
                installed_major: Some(installed),
                source: "manual".into(),
                java_path: Some(path.to_string_lossy().into_owned()),
            })
        } else {
            Ok(locate(&root, major))
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

pub async fn install_java_major(app: GameContext, major: u8) -> Result<JavaStatus, String> {
    if !matches!(major, 8 | 16 | 17 | 21 | 25) {
        return Err("Versión de Java no admitida".into());
    }
    ensure_java_major(app, major, None).await
}

#[cfg(test)]
mod settings_tests {
    use super::*;
    #[cfg(unix)]
    #[test]
    fn tar_extraction_preserves_java_executable_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let root = std::env::temp_dir().join(format!("openclient-java-tar-test-{}", uuid::Uuid::new_v4()));
        let runtime = root.join("runtime");
        let package = root.join("jdk");
        let executable = package.join("bin/java");
        std::fs::create_dir_all(executable.parent().unwrap()).unwrap();
        std::fs::write(&executable, b"runtime-fixture").unwrap();
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o755)).unwrap();
        let archive = root.join("runtime.tar.gz");
        assert!(std::process::Command::new("/usr/bin/tar")
            .arg("-czf").arg(&archive).arg("-C").arg(&root).arg("jdk")
            .status().unwrap().success());
        extract_java_archive(&archive, &runtime).unwrap();
        let installed = managed_java_exe(&runtime);
        assert_eq!(std::fs::read(&installed).unwrap(), b"runtime-fixture");
        assert_ne!(std::fs::metadata(installed).unwrap().permissions().mode() & 0o111, 0);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(windows)]
    #[tokio::test]
    #[ignore = "Requires official Adoptium and GitHub services"]
    async fn fresh_pc_can_resolve_java_including_java16_jdk_fallback() {
        let client = reqwest::Client::builder()
            .https_only(true)
            .timeout(Duration::from_secs(30))
            .build()
            .unwrap();
        for major in [8, 16, 17, 21, 25] {
            let package = resolve_adoptium_package(&client, major).await.unwrap();
            assert!(package.name.ends_with(".zip"));
            assert_eq!(package.checksum.len(), 64);
            assert!(package.checksum.bytes().all(|c| c.is_ascii_hexdigit()));
            assert!(package.link.starts_with("https://github.com/adoptium/"));
            if major == 16 {
                assert!(package.name.contains("-jdk_"));
                assert!(client
                    .head(&package.link)
                    .send()
                    .await
                    .unwrap()
                    .status()
                    .is_success());
            }
            println!("Java {major}: {} disponible con SHA-256", package.name);
        }
    }
    #[cfg(windows)]
    #[test]
    fn zip_extraction_rejects_traversal_and_keeps_runtime_layout() {
        use std::io::Write;
        let root =
            std::env::temp_dir().join(format!("openclient-java-zip-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let archive = root.join("runtime.zip");
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&archive).unwrap());
        let options = zip::write::SimpleFileOptions::default();
        zip.start_file("root/bin/javaw.exe", options).unwrap();
        zip.write_all(b"runtime-fixture").unwrap();
        zip.start_file("root/escape.dll", options).unwrap();
        zip.write_all(b"must-not-escape").unwrap();
        zip.finish().unwrap();
        // Mutate both header names without changing their length or file CRC.
        let mut bytes = std::fs::read(&archive).unwrap();
        let original = b"root/escape.dll";
        for i in 0..=bytes.len() - original.len() {
            if &bytes[i..i + original.len()] == original {
                bytes[i..i + original.len()].copy_from_slice(b"../escaped1.dll");
            }
        }
        std::fs::write(&archive, bytes).unwrap();
        let destination = root.join("runtime");
        extract_java_archive(&archive, &destination).unwrap();
        assert_eq!(
            std::fs::read(destination.join("bin/javaw.exe")).unwrap(),
            b"runtime-fixture"
        );
        assert!(!root.join("escaped1.dll").exists());
        assert!(!destination.join("escaped1.dll").exists());
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn compatibility_does_not_silently_replace_pinned_major() {
        assert!(is_compatible(25, 25));
        assert!(!is_compatible(25, 17));
        assert!(!is_compatible(21, 25));
        assert!(!is_compatible(17, 8));
        assert_eq!(required_java_major("1.17.1"), 16);
        assert_eq!(required_java_major("1.18.2"), 17);
    }
}
