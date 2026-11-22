//! Exportación e importación entre launchers: `.mrpack`, CurseForge (zip) y MultiMC/Prism.
//!
//! La lógica y los límites viven en `openclient_core::{mrpack_export, foreign_import}`; aquí solo
//! se valida la petición, se arma la entrada de `launcher_state.json` (siempre vía
//! `portable_metadata`, sin ajustes locales del autor) y se da forma a la respuesta.

use openclient_core::{
    foreign_import::{self, ForeignPack},
    library_state::load_state_at,
    modpack::Loader,
    mrpack_export::{self, Candidate, ExportOptions},
    GameContext,
};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

/// Tope de jars embebidos y de ids sin resolver que se devuelven a la interfaz.
const LISTED: usize = 50;
const DEFAULT_VERSION: &str = "1.0.0";

// ------------------------------------------------------------------ validación

fn text_valid(text: &str) -> bool {
    !text.trim().is_empty() && text.chars().count() <= 64 && !text.chars().any(char::is_control)
}

/// Id de instancia: un UUID en su forma canónica (también es un componente de ruta seguro).
fn instance_id(request: &Value) -> Result<&str, String> {
    let id = request["id"].as_str().ok_or("Falta el identificador.")?;
    if !uuid::Uuid::parse_str(id).is_ok_and(|uuid| uuid.to_string() == id) {
        return Err("Identificador de instancia inválido.".into());
    }
    Ok(id)
}

fn absolute_path(request: &Value, key: &str, missing: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(request[key].as_str().filter(|text| !text.is_empty()).ok_or(missing)?);
    if !path.is_absolute() {
        return Err(missing.to_owned());
    }
    Ok(path)
}

fn has_extension(path: &Path, wanted: &str) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case(wanted))
}

fn read_state(root: &Path) -> Result<Value, String> {
    match load_state_at(&root.join("launcher_state.json"))? {
        Some(raw) => serde_json::from_str(&raw).map_err(|_| "Biblioteca inválida.".to_owned()),
        None => Ok(json!({"instances": []})),
    }
}

// ----------------------------------------------------------------------- export

/// Comprobación temprana del destino (antes de gastar red): absoluto, `.mrpack`, carpeta
/// existente y fuera de `root`. `mrpack_export::write` repite y completa estas reglas.
fn check_destination(dest: &Path, root: &Path) -> Result<(), String> {
    if !has_extension(dest, "mrpack") {
        return Err("Elige un archivo .mrpack con una ruta absoluta válida.".into());
    }
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
    Ok(())
}

fn export_version(request: &Value, entry: &Value) -> Result<String, String> {
    match &request["versionNumber"] {
        Value::Null => Ok(entry["modpack"]["versionNumber"]
            .as_str()
            .filter(|version| text_valid(version))
            .unwrap_or(DEFAULT_VERSION)
            .to_owned()),
        Value::String(version) if text_valid(version) => Ok(version.clone()),
        _ => Err("La versión del modpack debe tener de 1 a 64 caracteres.".into()),
    }
}

/// Identifica los mods en Modrinth (red). Sin red no hay degradación: el error llega a `export`.
fn resolve_online(cands: &mut [Candidate], ctx: &GameContext) -> Result<(), String> {
    const NETWORK: &str =
        "No se pudo consultar Modrinth para identificar los mods; no se generó el archivo.";
    let client = reqwest::Client::builder()
        .https_only(true)
        .timeout(std::time::Duration::from_secs(15))
        .user_agent("OpenClient/0.1 (Minecraft desktop launcher)")
        .build()
        .map_err(|_| NETWORK.to_owned())?;
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| NETWORK.to_owned())?
        .block_on(mrpack_export::resolve_modrinth(cands, &client, ctx))
}

/// `modpack_export` `{root,id,file,includeSaves,versionNumber?}`. Operación registrada.
pub fn export(request: &Value, context: &GameContext) -> Result<Value, String> {
    export_with(request, context, resolve_online)
}

/// Igual que `export`, con la identificación de archivos inyectada (las pruebas no usan red).
fn export_with(
    request: &Value,
    ctx: &GameContext,
    resolve: impl FnOnce(&mut [Candidate], &GameContext) -> Result<(), String>,
) -> Result<Value, String> {
    let root = crate::operation::root_of(request)?;
    let id = instance_id(request)?;
    let dest = absolute_path(request, "file", "Elige un archivo .mrpack con una ruta absoluta válida.")?;
    check_destination(&dest, &root)?;
    let state = read_state(&root)?;
    let entry = state["instances"]
        .as_array()
        .and_then(|items| items.iter().find(|entry| entry["id"].as_str() == Some(id)))
        .ok_or("La instancia ya no existe.")?;
    let name: String = entry["name"]
        .as_str()
        .unwrap_or_default()
        .chars()
        .filter(|character| !character.is_control())
        .take(64)
        .collect();
    let opts = ExportOptions {
        include_saves: request["includeSaves"].as_bool() == Some(true),
        version_number: export_version(request, entry)?,
        name: name.trim().to_owned(),
    };
    let deps = mrpack_export::loader_dependencies(&root, entry)?;
    let canonical = root.canonicalize().map_err(|error| error.to_string())?;
    let game = canonical.join("instances").join(id);
    if game.canonicalize().ok().as_deref() != Some(game.as_path()) {
        return Err("La instancia no tiene una carpeta de datos válida.".into());
    }
    ctx.progress("collecting", 0, "Leyendo los archivos de la instancia");
    let mut cands = mrpack_export::collect(&game, &opts)?;
    resolve(&mut cands, ctx)?;
    let result = mrpack_export::write(&dest, &root, &deps, &cands, &opts, ctx)?;
    Ok(json!({
        "file": result.file.to_string_lossy(),
        "files": result.files,
        "overrides": result.overrides,
        "embeddedJars": result.embedded_jars.iter().take(LISTED).collect::<Vec<_>>(),
        "embeddedCount": result.embedded_jars.len(),
        "bytes": result.bytes,
    }))
}

// --------------------------------------------------------------------- importar

/// Entrada de estado de una instancia importada, sin ajustes locales y validada.
fn imported_entry(pack: &ForeignPack, modpack: Option<Value>, id: uuid::Uuid) -> Result<Value, String> {
    let fabric = (pack.loader == Loader::Fabric).then(|| json!(pack.loader_version));
    let mut source = json!({
        "name": pack.name, "mcVersion": pack.minecraft, "category": pack.loader.category(),
        "fabricVersion": fabric.unwrap_or(Value::Null), "installedContent": [],
    });
    if let Some(modpack) = modpack {
        source["modpack"] = modpack;
    }
    let mut entry = crate::portable_metadata(&source)?;
    entry["id"] = json!(id.to_string());
    Ok(entry)
}

/// `import_curseforge` `{root,file,confirm}`. Con `confirm:true` es operación registrada; sin
/// `confirm` solo inspecciona (el contexto no tiene operación asociada y no se toca el disco).
pub fn import_curseforge(request: &Value, context: &GameContext) -> Result<Value, String> {
    let root = crate::operation::root_of(request)?;
    let file = absolute_path(request, "file", "Selecciona un archivo .zip de CurseForge.")?;
    if !has_extension(&file, "zip") {
        return Err("Selecciona un archivo .zip de CurseForge.".into());
    }
    let pack = foreign_import::inspect_curseforge(&file)?;
    let unresolved = pack.unresolved.len();
    let message = pack.curseforge_message();
    if request["confirm"].as_bool() != Some(true) {
        return Ok(json!({
            "needsConfirmation": true, "name": pack.name, "version": pack.version_number,
            "minecraft": pack.minecraft, "loader": pack.loader.category(),
            "loaderVersion": pack.loader_version, "unresolved": unresolved,
            "overrideFiles": pack.override_files, "message": message, "warnings": pack.warnings,
        }));
    }
    let id = uuid::Uuid::new_v4();
    let version = if pack.version_number.is_empty() { "Sin versión" } else { &pack.version_number };
    let entry = imported_entry(
        &pack,
        Some(json!({
            "source": "curseforge", "projectId": null, "versionId": null,
            "versionNumber": version, "name": pack.name,
            "loader": pack.loader.category(), "loaderVersion": pack.loader_version,
        })),
        id,
    )?;
    let mut state = read_state(&root)?;
    state["instances"]
        .as_array_mut()
        .ok_or("Biblioteca inválida.")?
        .push(entry);
    foreign_import::publish_curseforge_with(&file, &root, id, &state.to_string(), context)?;
    let mut warnings = pack.warnings.clone();
    if unresolved > 0 {
        warnings.push(message);
    }
    Ok(json!({
        "id": id.to_string(), "instances": state["instances"].take(), "unresolved": unresolved,
        "unresolvedSample": pack.unresolved.iter().take(LISTED)
            .map(|(project, file)| json!({"projectID": project, "fileID": file})).collect::<Vec<_>>(),
        "warnings": warnings,
    }))
}

/// `import_mmc` `{root,path}`. Operación registrada. Solo se importan los datos del jugador y
/// el nombre: Java, argumentos, comandos y servidor de `instance.cfg` se ignoran.
pub fn import_mmc(request: &Value, context: &GameContext) -> Result<Value, String> {
    let root = crate::operation::root_of(request)?;
    let path = absolute_path(request, "path", "Selecciona una carpeta o un .zip de MultiMC/Prism.")?;
    let pack = foreign_import::inspect_mmc(&path)?;
    let id = uuid::Uuid::new_v4();
    let entry = imported_entry(&pack, None, id)?;
    let mut state = read_state(&root)?;
    state["instances"]
        .as_array_mut()
        .ok_or("Biblioteca inválida.")?
        .push(entry);
    foreign_import::publish_mmc_with(&path, &root, id, &state.to_string(), context)?;
    let mut warnings = pack.warnings.clone();
    if pack.loader != Loader::Vanilla && pack.loader != Loader::Fabric && pack.loader_version.is_some() {
        warnings.push(format!(
            "OpenClient usará la versión recomendada de {} para {}.",
            pack.loader.category(),
            pack.minecraft
        ));
    }
    Ok(json!({"id": id.to_string(), "instances": state["instances"].take(), "warnings": warnings}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use openclient_core::modpack::Modpack;
    use std::fs;

    fn crc32(data: &[u8]) -> u32 {
        let mut crc = !0u32;
        for &byte in data {
            crc ^= u32::from(byte);
            for _ in 0..8 {
                crc = if crc & 1 != 0 { (crc >> 1) ^ 0xEDB8_8320 } else { crc >> 1 };
            }
        }
        !crc
    }

    /// Zip sin compresión (ffi no depende de `zip`).
    fn stored_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut central = Vec::new();
        for (name, data) in entries {
            let offset = out.len() as u32;
            let mut header = Vec::new();
            header.extend_from_slice(&20u16.to_le_bytes());
            header.extend_from_slice(&0u16.to_le_bytes());
            header.extend_from_slice(&0u16.to_le_bytes());
            header.extend_from_slice(&0u16.to_le_bytes());
            header.extend_from_slice(&0x21u16.to_le_bytes());
            header.extend_from_slice(&crc32(data).to_le_bytes());
            header.extend_from_slice(&(data.len() as u32).to_le_bytes());
            header.extend_from_slice(&(data.len() as u32).to_le_bytes());
            header.extend_from_slice(&(name.len() as u16).to_le_bytes());
            header.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
            out.extend_from_slice(&header);
            out.extend_from_slice(name.as_bytes());
            out.extend_from_slice(data);
            central.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
            central.extend_from_slice(&20u16.to_le_bytes());
            central.extend_from_slice(&header);
            central.extend_from_slice(&[0; 10]);
            central.extend_from_slice(&offset.to_le_bytes());
            central.extend_from_slice(name.as_bytes());
        }
        let start = out.len() as u32;
        out.extend_from_slice(&central);
        out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
        out.extend_from_slice(&[0, 0, 0, 0]);
        out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
        out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
        out.extend_from_slice(&(central.len() as u32).to_le_bytes());
        out.extend_from_slice(&start.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out
    }

    struct Library {
        base: PathBuf,
        root: PathBuf,
        out: PathBuf,
    }

    impl Library {
        fn new() -> Self {
            let base = std::env::temp_dir().join(format!("openclient-transfer-{}", uuid::Uuid::new_v4()));
            let (root, out) = (base.join("datos"), base.join("salida"));
            fs::create_dir_all(&root).unwrap();
            fs::create_dir_all(&out).unwrap();
            Self { base, root, out }
        }

        fn write_state(&self, instances: Value) {
            fs::write(self.root.join("launcher_state.json"), json!({"instances": instances}).to_string()).unwrap();
        }

        fn put(&self, relative: &str, data: &[u8]) {
            let path = self.base.join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, data).unwrap();
        }

        fn state(&self) -> String {
            fs::read_to_string(self.root.join("launcher_state.json")).unwrap()
        }

        fn out_files(&self) -> usize {
            fs::read_dir(&self.out).unwrap().count()
        }
    }

    impl Drop for Library {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.base);
        }
    }

    fn ctx(library: &Library) -> GameContext {
        GameContext::new(library.root.clone(), "test", |_, _| Ok(()))
    }

    /// Las acciones de importación sin pasar por `tracked` (el estado de operación es global y
    /// otras pruebas lo leen); el enrutado de `lib.rs` es una línea por acción.
    fn run(request: &Value) -> Result<Value, String> {
        let context = crate::operation::detached_context(request)?;
        match request["action"].as_str() {
            Some("import_curseforge") => import_curseforge(request, &context),
            Some("import_mmc") => import_mmc(request, &context),
            _ => Err("acción".into()),
        }
    }

    fn contains(haystack: &[u8], needle: &str) -> bool {
        haystack.windows(needle.len()).any(|window| window == needle.as_bytes())
    }

    /// Respuesta simulada de `POST /version_files` para `mods/sodium.jar`.
    fn simulated_modrinth(cands: &mut [Candidate]) -> Result<(), String> {
        let mut response = serde_json::Map::new();
        for candidate in cands.iter().filter(|c| c.path == "mods/sodium.jar") {
            let (sha1, sha512) = (candidate.sha1.clone().unwrap(), candidate.sha512.clone().unwrap());
            response.insert(
                sha512.clone(),
                json!({"project_id": "AANobbMI", "files": [{
                    "hashes": {"sha1": sha1, "sha512": sha512}, "size": candidate.size,
                    "url": "https://cdn.modrinth.com/data/AANobbMI/versions/v1/sodium.jar"}]}),
            );
        }
        mrpack_export::resolve_from_response(cands, &Value::Object(response));
        mrpack_export::apply_projects(
            cands,
            &json!([{"id": "AANobbMI", "client_side": "required", "server_side": "optional"}]),
        );
        Ok(())
    }

    fn trap_instance(library: &Library, id: &str) {
        library.write_state(json!([{
            "id": id, "name": "Mi instancia", "mcVersion": "1.21.4", "category": "fabric",
            "fabricVersion": "0.16.14", "installedContent": [], "ramMb": 4096,
            "preLaunch": "evil.exe", "postExit": "evil.exe", "wrapper": "evil.exe",
            "javaPath": "C:\\evil\\java.exe", "jvmArgs": "-Devil=1", "notes": "privado-evil",
            "joinServer": "evil.example"
        }]));
        let game = format!("datos/instances/{id}");
        for (path, data) in [
            ("mods/sodium.jar", "jar-sodium"),
            ("mods/custom.jar", "jar-custom"),
            ("mods/.openclient_managed.json", "{}"),
            ("config/a.toml", "a=1"),
            ("config/.oculto", "x"),
            ("options.txt", "fov:70"),
            ("servers.dat", "secreto"),
            ("token.txt", "secreto"),
            ("logs/latest.log", "log"),
            ("crash-reports/c.txt", "c"),
            ("screenshots/s.png", "s"),
            (".openclient/modpack.json", "{}"),
            ("saves/Mundo/level.dat", "mundo"),
            ("resourcepacks/Mio.zip", "z"),
            ("resourcepacks/OpenClient-Base.zip", "z"),
        ] {
            library.put(&format!("{game}/{path}"), data.as_bytes());
        }
        // Fuera de la instancia: sesión y estado.
        library.put("datos/session.json", b"token-secreto");
    }

    #[test]
    fn export_of_a_trap_instance_only_ships_allowed_files_and_round_trips() {
        let library = Library::new();
        let id = uuid::Uuid::new_v4().to_string();
        trap_instance(&library, &id);
        let dest = library.out.join("Pack.mrpack");
        let request = json!({"root": library.root, "id": id, "file": dest, "includeSaves": false});
        let result = export_with(&request, &ctx(&library), |cands, _| simulated_modrinth(cands)).unwrap();
        assert_eq!(
            Path::new(result["file"].as_str().unwrap()).canonicalize().unwrap(),
            dest.canonicalize().unwrap()
        );
        assert_eq!(result["files"], 1);
        assert_eq!(result["overrides"], 4);
        assert_eq!(result["embeddedJars"], json!(["mods/custom.jar"]));
        assert_eq!(result["embeddedCount"], 1);
        assert!(result["bytes"].as_u64().unwrap() > 0);
        assert_eq!(library.out_files(), 1, "no debe quedar el temporal");

        let bytes = fs::read(&dest).unwrap();
        for forbidden in [
            "servers.dat", "logs/", "crash-reports", "screenshots", ".openclient", "token.txt",
            "saves/", "launcher_state", "session.json", "OpenClient-", "evil", ".oculto",
        ] {
            assert!(!contains(&bytes, forbidden), "{forbidden} viaja en el .mrpack");
        }
        for allowed in ["modrinth.index.json", "overrides/config/a.toml", "overrides/options.txt",
            "overrides/mods/custom.jar", "overrides/resourcepacks/Mio.zip"] {
            assert!(contains(&bytes, allowed), "falta {allowed}");
        }
        // Ida y vuelta: lo exportado lo acepta el importador de OpenClient.
        let mut pack = Modpack::open_local(&dest).unwrap();
        assert_eq!(pack.identity().unwrap(), ("Mi instancia", "1.0.0"));
        let summary = pack.summary().unwrap();
        assert_eq!(summary["minecraft"], "1.21.4");
        assert_eq!(summary["fabric"], "0.16.14");
        assert_eq!(summary["mods"], json!(["custom", "sodium"]));

        // Con `includeSaves` los mundos entran y nada más cambia.
        let with_saves = library.out.join("Con mundos.mrpack");
        let request = json!({"root": library.root, "id": id, "file": with_saves, "includeSaves": true, "versionNumber": "2.5"});
        let result = export_with(&request, &ctx(&library), |cands, _| simulated_modrinth(cands)).unwrap();
        assert_eq!(result["overrides"], 5);
        assert!(contains(&fs::read(&with_saves).unwrap(), "overrides/saves/Mundo/level.dat"));
        assert_eq!(Modpack::open_local(&with_saves).unwrap().identity().unwrap().1, "2.5");

        // Sin identificar nada (todo a overrides) el paquete se reimporta con los archivos propios.
        let plain = library.out.join("Plano.mrpack");
        let request = json!({"root": library.root, "id": id, "file": plain});
        export_with(&request, &ctx(&library), |_, _| Ok(())).unwrap();
        let target = Library::new();
        let imported = crate::catalog::install_pack(&json!({
            "action": "modpack_import", "root": target.root, "file": plain
        }))
        .unwrap();
        let copy = target.root.join("instances").join(imported[0]["id"].as_str().unwrap());
        assert_eq!(fs::read(copy.join("mods/sodium.jar")).unwrap(), b"jar-sodium");
        assert_eq!(fs::read(copy.join("config/a.toml")).unwrap(), b"a=1");
        assert!(!copy.join("servers.dat").exists() && !copy.join("saves").exists());
        assert!(!target.state().contains("evil"));
    }

    #[test]
    fn export_rejects_bad_requests_without_creating_files() {
        let library = Library::new();
        let id = uuid::Uuid::new_v4().to_string();
        trap_instance(&library, &id);
        let run = |request: Value| export_with(&request, &ctx(&library), |_, _| Ok(()));
        let base = |file: Value| json!({"root": library.root, "id": id, "file": file});
        let inside = library.root.join("Pack.mrpack");
        for (request, wanted) in [
            (base(json!(inside)), "fuera del directorio"),
            (base(json!("Pack.mrpack")), "ruta absoluta"),
            (base(json!(library.out.join("Pack.zip"))), ".mrpack"),
            (base(json!(library.out.join("no-existe").join("Pack.mrpack"))), "no existe"),
            (base(Value::Null), "ruta absoluta"),
            (json!({"root": library.root, "id": "../x", "file": library.out.join("P.mrpack")}), "inválido"),
            (json!({"root": library.root, "file": library.out.join("P.mrpack")}), "identificador"),
            (json!({"root": library.root, "id": uuid::Uuid::new_v4().to_string(), "file": library.out.join("P.mrpack")}), "ya no existe"),
            (json!({"root": library.root, "id": id, "file": library.out.join("P.mrpack"), "versionNumber": "a\nb"}), "versión"),
            (json!({"root": library.root, "id": id, "file": library.out.join("P.mrpack"), "versionNumber": "v".repeat(65)}), "versión"),
        ] {
            let error = run(request.clone()).unwrap_err();
            assert!(error.contains(wanted), "{request}: {error}");
        }
        assert_eq!(library.out_files(), 0);
        assert!(!inside.exists());
        // Si la identificación en red falla no se genera nada, ni siquiera un temporal.
        let failing = export_with(&base(json!(library.out.join("P.mrpack"))), &ctx(&library), |_, _| {
            Err("No se pudo consultar Modrinth para identificar los mods; no se generó el archivo.".into())
        });
        assert!(failing.unwrap_err().contains("no se generó"));
        assert_eq!(library.out_files(), 0);
    }

    #[test]
    fn export_lists_at_most_fifty_embedded_jars_but_counts_all() {
        let library = Library::new();
        let id = uuid::Uuid::new_v4().to_string();
        library.write_state(json!([{"id": id, "name": "n", "mcVersion": "1.21.4", "category": "vanilla", "installedContent": []}]));
        for number in 0..60 {
            library.put(&format!("datos/instances/{id}/mods/m{number}.jar"), format!("jar{number}").as_bytes());
        }
        let request = json!({"root": library.root, "id": id, "file": library.out.join("P.mrpack")});
        let result = export_with(&request, &ctx(&library), |_, _| Ok(())).unwrap();
        assert_eq!(result["embeddedJars"].as_array().unwrap().len(), 50);
        assert_eq!(result["embeddedCount"], 60);
        assert_eq!(Modpack::open_local(&library.out.join("P.mrpack")).unwrap().pack_dependencies().unwrap().loader, Loader::Vanilla);
    }

    fn curseforge_zip(library: &Library, manifest: Value, extra: &[(&str, &[u8])]) -> PathBuf {
        let manifest = manifest.to_string();
        let mut entries: Vec<(&str, &[u8])> = vec![("manifest.json", manifest.as_bytes())];
        entries.extend_from_slice(extra);
        let path = library.base.join(format!("{}.zip", uuid::Uuid::new_v4()));
        fs::write(&path, stored_zip(&entries)).unwrap();
        path
    }

    fn curse_manifest() -> Value {
        json!({
            "manifestType": "minecraftModpack", "manifestVersion": 1, "name": "Pack CF", "version": "3.1",
            "minecraft": {"version": "1.20.1", "modLoaders": [{"id": "forge-47.2.0", "primary": true}]},
            "overrides": "overrides",
            "files": [{"projectID": 1, "fileID": 11, "required": true}, {"projectID": 2, "fileID": 22},
                {"projectID": 3, "fileID": 33, "required": false}]
        })
    }

    #[test]
    fn curseforge_import_inspects_first_and_only_creates_the_instance_on_confirmation() {
        let library = Library::new();
        let file = curseforge_zip(&library, curse_manifest(), &[
            ("overrides/config/a.toml", b"a=1"), ("overrides/options.txt", b"fov:80"),
            ("overrides/.hidden/x", b"x"),
        ]);
        let before = fs::read_dir(&library.root).unwrap().count();
        let inspected = run(&json!({"action": "import_curseforge", "root": library.root, "file": file})).unwrap();
        let inspected_false = run(&json!({"action": "import_curseforge", "root": library.root, "file": file, "confirm": false})).unwrap();
        assert_eq!(inspected, inspected_false);
        assert_eq!(inspected["needsConfirmation"], true);
        assert_eq!(inspected["name"], "Pack CF");
        assert_eq!(inspected["version"], "3.1");
        assert_eq!(inspected["minecraft"], "1.20.1");
        assert_eq!(inspected["loader"], "forge");
        assert_eq!(inspected["loaderVersion"], "47.2.0");
        assert_eq!(inspected["unresolved"], 3);
        assert_eq!(inspected["overrideFiles"], 2);
        let message = inspected["message"].as_str().unwrap();
        assert!(message.contains("3 mods") && message.contains("clave de API") && message.contains("no arrancará"));
        assert_eq!(fs::read_dir(&library.root).unwrap().count(), before, "la inspección no toca el disco");

        let done = run(&json!({"action": "import_curseforge", "root": library.root, "file": file, "confirm": true})).unwrap();
        assert_eq!(done["unresolved"], 3);
        assert_eq!(done["unresolvedSample"].as_array().unwrap().len(), 3);
        assert_eq!(done["unresolvedSample"][0], json!({"projectID": 1, "fileID": 11}));
        assert!(done["warnings"][0].as_str().unwrap().contains("3 mods"));
        let entry = &done["instances"][0];
        assert_eq!(entry["name"], "Pack CF");
        assert_eq!(entry["installedContent"], json!([]));
        assert_eq!(entry["modpack"], json!({
            "source": "curseforge", "projectId": null, "versionId": null, "versionNumber": "3.1",
            "name": "Pack CF", "loader": "forge", "loaderVersion": "47.2.0"
        }));
        assert_eq!(entry["modpackVersion"], "3.1");
        let game = library.root.join("instances").join(entry["id"].as_str().unwrap());
        assert_eq!(fs::read(game.join("config/a.toml")).unwrap(), b"a=1");
        assert!(!game.join(".hidden").exists());
        let sidecar: Value = serde_json::from_slice(&fs::read(game.join(".openclient/curseforge-unresolved.json")).unwrap()).unwrap();
        assert_eq!(sidecar.as_array().unwrap().len(), 3);
        assert_eq!(sidecar[2], json!({"projectID": 3, "fileID": 33, "required": false}));
        let saved: Value = serde_json::from_str(&library.state()).unwrap();
        assert_eq!(saved["instances"][0]["modpack"]["source"], "curseforge");
        // Importar otra vez crea otra instancia, nunca sobrescribe la anterior.
        let again = run(&json!({"action": "import_curseforge", "root": library.root, "file": file, "confirm": true})).unwrap();
        assert_eq!(again["instances"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn curseforge_import_rejects_hostile_packs_and_bad_requests_without_touching_the_library() {
        let library = Library::new();
        let before = fs::read_dir(&library.root).unwrap().count();
        let mut wrong_type = curse_manifest();
        wrong_type["manifestType"] = json!("otro");
        let mut escaping = curse_manifest();
        escaping["overrides"] = json!("../x");
        let mut rift = curse_manifest();
        rift["minecraft"]["modLoaders"] = json!([{"id": "rift-1", "primary": true}]);
        let mut two = curse_manifest();
        two["minecraft"]["modLoaders"] = json!([{"id": "forge-1"}, {"id": "fabric-0.16.0"}]);
        let mut many = curse_manifest();
        many["files"] = json!((0..2501).map(|n| json!({"projectID": n, "fileID": n})).collect::<Vec<_>>());
        for (manifest, label) in [(wrong_type, "type"), (escaping, "overrides"), (rift, "rift"), (two, "dos"), (many, "files")] {
            let file = curseforge_zip(&library, manifest, &[("overrides/a", b"x")]);
            for confirm in [false, true] {
                let result = run(&json!({"action": "import_curseforge", "root": library.root, "file": file, "confirm": confirm}));
                assert!(result.is_err(), "{label} confirm={confirm}");
            }
        }
        let good = curseforge_zip(&library, curse_manifest(), &[]);
        for request in [
            json!({"action": "import_curseforge", "root": library.root, "file": "relativo.zip", "confirm": true}),
            json!({"action": "import_curseforge", "root": library.root, "file": library.base.join("x.mrpack"), "confirm": true}),
            json!({"action": "import_curseforge", "root": library.root, "confirm": true}),
            json!({"action": "import_curseforge", "root": "relativa", "file": good}),
        ] {
            assert!(run(&request).is_err(), "{request}");
        }
        assert_eq!(fs::read_dir(&library.root).unwrap().count(), before);
        assert!(!library.root.join("launcher_state.json").exists());
    }

    fn mmc_folder(library: &Library, loader: (&str, &str)) -> PathBuf {
        let folder = library.base.join("origen").join("Mi MMC");
        fs::create_dir_all(&folder).unwrap();
        fs::write(folder.join("instance.cfg"), "name=Instancia MMC\nPreLaunchCommand=evil.exe\nPostExitCommand=evil.exe\nWrapperCommand=evil\nJvmArgs=-Devil\nJavaPath=C:\\evil\\java.exe\nJoinServerOnLaunchAddress=evil.example\nMaxMemAlloc=99999\n").unwrap();
        fs::write(folder.join("mmc-pack.json"), json!({"formatVersion": 1, "components": [
            {"uid": "net.minecraft", "version": "1.20.1"}, {"uid": loader.0, "version": loader.1}]}).to_string()).unwrap();
        for (path, data) in [
            (".minecraft/mods/a.jar", "jar"), (".minecraft/mods/.index/a.pw.toml", "x"),
            (".minecraft/config/c.toml", "c"), (".minecraft/options.txt", "fov:60"),
            (".minecraft/saves/W/level.dat", "w"), (".minecraft/logs/latest.log", "log"),
            (".minecraft/servers.dat", "s"), (".minecraft/crash-reports/c.txt", "c"),
        ] {
            let target = folder.join(path);
            fs::create_dir_all(target.parent().unwrap()).unwrap();
            fs::write(target, data).unwrap();
        }
        folder
    }

    #[test]
    fn mmc_import_ignores_commands_java_and_arguments() {
        let library = Library::new();
        let folder = mmc_folder(&library, ("net.minecraftforge", "47.2.0"));
        let result = run(&json!({"action": "import_mmc", "root": library.root, "path": folder})).unwrap();
        let entry = &result["instances"][0];
        assert_eq!(entry["name"], "Instancia MMC");
        assert_eq!(entry["mcVersion"], "1.20.1");
        assert_eq!(entry["category"], "forge");
        assert_eq!(entry["installedContent"], json!([]));
        for key in ["preLaunch", "postExit", "wrapper", "jvmArgs", "javaPath", "joinServer", "ramMb", "modpack", "notes"] {
            assert!(entry.get(key).is_none(), "{key} aparece en la instancia");
        }
        assert!(!library.state().contains("evil"));
        let warnings = result["warnings"].as_array().unwrap();
        assert!(warnings.iter().any(|w| w.as_str().unwrap().contains("Java")));
        assert!(warnings.iter().any(|w| w.as_str().unwrap().contains("versión recomendada de forge")));
        let game = library.root.join("instances").join(entry["id"].as_str().unwrap());
        assert_eq!(fs::read(game.join("mods/a.jar")).unwrap(), b"jar");
        assert_eq!(fs::read(game.join("saves/W/level.dat")).unwrap(), b"w");
        assert_eq!(fs::read(game.join("options.txt")).unwrap(), b"fov:60");
        // `servers.dat` y las capturas son datos del jugador (lista blanca portable); lo demás no.
        assert_eq!(fs::read(game.join("servers.dat")).unwrap(), b"s");
        for absent in ["logs", "crash-reports", "mods/.index", "instance.cfg"] {
            assert!(!game.join(absent).exists(), "{absent} se copió");
        }
        let saved: Value = serde_json::from_str(&library.state()).unwrap();
        assert_eq!(saved["instances"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn mmc_import_keeps_fabric_version_and_rejects_hostile_sources() {
        let library = Library::new();
        let folder = mmc_folder(&library, ("net.fabricmc.fabric-loader", "0.16.14"));
        let result = run(&json!({"action": "import_mmc", "root": library.root, "path": folder})).unwrap();
        assert_eq!(result["instances"][0]["fabricVersion"], "0.16.14");
        assert_eq!(result["warnings"].as_array().unwrap().len(), 1);

        let before = library.state();
        let instances = fs::read_dir(library.root.join("instances")).unwrap().count();
        // `.minecraft` y `minecraft` a la vez.
        fs::create_dir_all(folder.join("minecraft")).unwrap();
        // Componente no compatible y carpeta dentro de la biblioteca.
        let unsupported = library.base.join("origen").join("Liteloader");
        fs::create_dir_all(&unsupported).unwrap();
        fs::write(unsupported.join("instance.cfg"), "name=x\n").unwrap();
        fs::write(unsupported.join("mmc-pack.json"), json!({"formatVersion": 1, "components": [
            {"uid": "net.minecraft", "version": "1.12.2"}, {"uid": "com.mumfrey.liteloader", "version": "1"}]}).to_string()).unwrap();
        let inside = library.root.join("copia");
        fs::create_dir_all(&inside).unwrap();
        fs::copy(unsupported.join("instance.cfg"), inside.join("instance.cfg")).unwrap();
        fs::copy(unsupported.join("mmc-pack.json"), inside.join("mmc-pack.json")).unwrap();
        for path in [
            json!(folder), json!(unsupported), json!(inside), json!("relativa"),
            json!(library.base.join("no-existe")), json!(library.base.join("origen").join("x.zip")),
        ] {
            let result = run(&json!({"action": "import_mmc", "root": library.root, "path": path}));
            assert!(result.is_err(), "{path}");
        }
        assert!(run(&json!({"action": "import_mmc", "root": library.root})).is_err());
        assert_eq!(library.state(), before);
        assert_eq!(fs::read_dir(library.root.join("instances")).unwrap().count(), instances);
    }

    #[test]
    fn mmc_zip_is_imported_one_level_deep_only() {
        let library = Library::new();
        let pack = json!({"formatVersion": 1, "components": [{"uid": "net.minecraft", "version": "1.21.4"}]}).to_string();
        let write = |name: &str, entries: &[(&str, &[u8])]| {
            let path = library.base.join(name);
            fs::write(&path, stored_zip(entries)).unwrap();
            path
        };
        let ok = write("ok.zip", &[
            ("Mi MMC/instance.cfg", b"name=Desde zip\nJvmArgs=-Devil\n"), ("Mi MMC/mmc-pack.json", pack.as_bytes()),
            ("Mi MMC/minecraft/options.txt", b"fov:50"), ("Mi MMC/minecraft/logs/x.log", b"log"),
        ]);
        let deep = write("deep.zip", &[
            ("a/b/instance.cfg", b"name=x\n"), ("a/b/mmc-pack.json", pack.as_bytes()), ("a/b/minecraft/options.txt", b"x"),
        ]);
        assert!(run(&json!({"action": "import_mmc", "root": library.root, "path": deep})).is_err());
        assert!(!library.root.join("launcher_state.json").exists());
        let result = run(&json!({"action": "import_mmc", "root": library.root, "path": ok})).unwrap();
        assert_eq!(result["instances"][0]["name"], "Desde zip");
        assert_eq!(result["instances"][0]["category"], "vanilla");
        let game = library.root.join("instances").join(result["instances"][0]["id"].as_str().unwrap());
        assert_eq!(fs::read(game.join("options.txt")).unwrap(), b"fov:50");
        assert!(!game.join("logs").exists() && !library.state().contains("evil"));
    }
}
