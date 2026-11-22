//! ABI C para la interfaz Qt. Las credenciales no forman parte del contrato.

use openclient_core::{
    instance_storage::{delete_at, duplicate_at},
    library_state::{load_state_at, write_state_locked},
    portable_instance,
};
use serde_json::{json, Value};
use std::ffi::{c_char, CString};
use std::path::Path;
mod account;
mod catalog;
mod content;
mod crash;
mod game;
mod instance_settings;
mod modpack_ops;
mod operation;
mod transfer;
mod worlds;

#[no_mangle]
pub extern "C" fn openclient_abi_version() -> u32 {
    3
}

/// Devuelve JSON UTF-8 propiedad de Rust; liberarlo con `openclient_free_string`.
/// Un puntero nulo indica un error interno. Ningún panic cruza la ABI.
#[no_mangle]
pub extern "C" fn openclient_hardware_json() -> *mut c_char {
    std::panic::catch_unwind(|| {
        let hardware = openclient_core::performance::detect_system_hardware();
        let json = serde_json::json!({ "ok": true, "abi_version": 3, "hardware": hardware });
        CString::new(json.to_string())
            .map(CString::into_raw)
            .unwrap_or(std::ptr::null_mut())
    })
    .unwrap_or(std::ptr::null_mut())
}

/// # Safety
/// The caller must supply a readable buffer of exactly `length` bytes.
#[no_mangle]
pub unsafe extern "C" fn openclient_request_json(input: *const u8, length: usize) -> *mut c_char {
    if input.is_null() || length > 32768 {
        return std::ptr::null_mut();
    }
    std::panic::catch_unwind(|| {
        let response = match serde_json::from_slice(std::slice::from_raw_parts(input, length)) {
            Ok(request) => launcher_operation(&request),
            Err(_) => Err("Solicitud inválida.".into()),
        };
        let response = match response {
            Ok(result) => json!({"ok": true, "result": result}),
            Err(error) => json!({"error": error}),
        };
        CString::new(response.to_string()).map_or(std::ptr::null_mut(), CString::into_raw)
    })
    .unwrap_or(std::ptr::null_mut())
}

fn launcher_operation(request: &Value) -> Result<Value, String> {
    if let Some(count) = request["downloadConcurrency"].as_u64() {
        openclient_core::set_download_concurrency(usize::try_from(count).unwrap_or(1));
    }
    match request["action"].as_str() {
        Some("launch") => return game::launch(request),
        Some("game_status") => return Ok(game::status()),
        Some("game_log") => return game::read_log(request),
        Some("crash_analyze") => return crash::analyze_crash(request),
        Some("world_list") => return worlds::list(request),
        Some("world_backup") => return operation::tracked("world_backup", request, worlds::backup),
        Some("world_restore") => return operation::tracked("world_restore", request, worlds::restore),
        Some("world_backup_delete") => return worlds::delete(request),
        Some(action @ ("java_check" | "java_install")) => return game::java_operation(action, request),
        Some(
            action @ ("auth_prepare" | "auth_login" | "auth_prompt" | "auth_cancel"
            | "auth_profile" | "auth_accounts" | "auth_diagnose" | "auth_forget" | "auth_refresh" | "auth_equip_cape"),
        ) => return account::operation(action, request),
        Some("catalog_search" | "catalog_discover") => return catalog::search(request),
        Some("catalog_project") => return catalog::project(request),
        Some(kind @ ("modpack_install" | "modpack_import")) => {
            return operation::tracked(kind, request, |request, _| catalog::install_pack(request))
        }
        Some("modpack_inspect") => return catalog::inspect_pack(request),
        Some("content_sync") => return operation::tracked("content_sync", request, game::sync_content_with),
        Some("operation_status") => return Ok(operation::status()),
        Some("operation_cancel") => return operation::cancel(request),
        Some("modpack_check") => return modpack_ops::check(request),
        Some("modpack_link_candidates") => return modpack_ops::link_candidates(request),
        Some("modpack_link") => return operation::tracked("modpack_link", request, modpack_ops::link),
        Some("modpack_update") => {
            return operation::tracked("modpack_update", request, modpack_ops::update)
        }
        Some("modpack_snapshots") => return modpack_ops::snapshots(request),
        Some("modpack_restore") => {
            return operation::tracked("modpack_restore", request, modpack_ops::restore)
        }
        Some("content_conflicts") => return catalog::content_conflicts(request),
        Some("content_list") => return content::list(request),
        Some("content_resolve") => return content::resolve(request),
        Some("modpack_export") => {
            return operation::tracked("modpack_export", request, transfer::export)
        }
        Some("import_curseforge") => {
            // Sin `confirm` solo inspecciona: no registra operación.
            return if request["confirm"].as_bool() == Some(true) {
                operation::tracked("import_curseforge", request, transfer::import_curseforge)
            } else {
                transfer::import_curseforge(request, &operation::detached_context(request)?)
            };
        }
        Some("import_mmc") => return operation::tracked("import_mmc", request, transfer::import_mmc),
        Some(kind @ ("duplicate" | "import" | "export")) => {
            return operation::tracked(kind, request, |request, _| library_operation(request))
        }
        Some("minecraft_versions") => {
            let root = Path::new(request["root"].as_str().ok_or("Falta la carpeta de datos.")?);
            if !root.is_absolute() || !root.is_dir() {
                return Err("Carpeta de datos inválida.".into());
            }
            return openclient_core::versions::catalog(root, request["refresh"].as_bool() == Some(true));
        }
        Some(action @ ("storage_usage" | "storage_clear")) => {
            return instance_settings::storage_operation(action, request)
        }
        Some("content_update") => {
            return operation::tracked("content_update", request, |request, _| {
                catalog::update_content(request)
            })
        }
        Some("content_add") => {
            return operation::tracked("content_add", request, |request, _| catalog::change(request))
        }
        Some("content_toggle" | "content_remove") => return catalog::change(request),
        _ => {}
    }
    if request["action"] == "offline" {
        let username = request["username"].as_str().unwrap_or_default();
        let account = openclient_core::identity::login_offline(username.to_owned())?;
        return Ok(json!({"username": account.username, "uuid": account.uuid}));
    }
    library_operation(request)
}

fn library_operation(request: &Value) -> Result<Value, String> {
    let root = Path::new(
        request["root"]
            .as_str()
            .ok_or("Falta la carpeta de datos.")?,
    );
    if !root.is_absolute() || !root.is_dir() {
        return Err("Carpeta de datos inválida.".into());
    }
    let path = root.join("launcher_state.json");
    let mut state = match load_state_at(&path)? {
        Some(raw) => serde_json::from_str::<Value>(&raw).map_err(|_| "Biblioteca inválida.")?,
        None => json!({"instances": []}),
    };
    let instances = state["instances"]
        .as_array_mut()
        .ok_or("Biblioteca inválida.")?;
    match request["action"].as_str() {
        Some("list") => {}
        Some("export") => {
            let id = request["id"].as_str().ok_or("Falta el identificador.")?;
            let entry = instances
                .iter()
                .find(|entry| entry["id"].as_str() == Some(id))
                .ok_or("La instancia ya no existe.")?;
            let directory = Path::new(
                request["directory"]
                    .as_str()
                    .ok_or("Elige una carpeta de destino.")?,
            );
            let folder =
                portable_instance::export(root, id, directory, &portable_metadata(entry)?)?;
            return Ok(json!({"folder": folder.to_string_lossy()}));
        }
        Some("import") => {
            let directory = Path::new(
                request["directory"]
                    .as_str()
                    .ok_or("Elige una carpeta de instancia.")?,
            );
            let (game, metadata) = portable_instance::read(root, directory)?;
            let mut entry = portable_metadata(&metadata)?;
            let id = uuid::Uuid::new_v4();
            entry["id"] = json!(id.to_string());
            instances.push(entry);
            portable_instance::import(root, &game, id, &state.to_string())?;
        }
        Some("create" | "edit") => {
            let name = request["name"].as_str().unwrap_or_default().trim();
            let version = request["version"].as_str().unwrap_or_default().trim();
            let loader = request["loader"].as_str().unwrap_or_default();
            if name.is_empty() || name.chars().count() > 64 || name.chars().any(char::is_control) {
                return Err("Escribe un nombre de 1 a 64 caracteres.".into());
            }
            if version.is_empty()
                || version.len() > 40
                || !version.as_bytes()[0].is_ascii_digit()
                || !version
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b".-_".contains(&c))
            {
                return Err("Escribe una versión de Minecraft válida, por ejemplo 1.21.4.".into());
            }
            let label = match loader {
                "vanilla" => "Vanilla",
                "fabric" => "Fabric",
                "quilt" => "Quilt",
                "forge" => "Forge",
                "neoforge" => "NeoForge",
                _ => return Err("Selecciona Vanilla, Fabric, Quilt, Forge o NeoForge.".into()),
            };
            if request["action"] == "create" {
                let initial_content = if loader == "fabric" && request["autoSodium"].as_bool() == Some(true) {
                    json!([{
                        "id": "AANobbMI",
                        "versionId": "auto",
                        "type": "mod",
                        "source": "modrinth",
                        "title": "Sodium",
                        "version": "Auto",
                        "enabled": true
                    }])
                } else {
                    json!([])
                };
                instances.push(json!({
                    "id": uuid::Uuid::new_v4().to_string(), "name": name, "mcVersion": version,
                    "category": loader, "loaderLabel": label, "installedContent": initial_content,
                    "lastPlayed": "", "playtimeHours": 0, "accentColor": "#8494ac"
                }));
            } else {
                let id = request["id"].as_str().ok_or("Falta el identificador.")?;
                let instance = instances
                    .iter_mut()
                    .find(|item| item["id"].as_str() == Some(id))
                    .ok_or("La instancia ya no existe.")?;
                if (instance["modpack"].is_object() || instance["modpackVersion"].is_string())
                    && (instance["mcVersion"] != version || instance["category"] != loader)
                {
                    return Err("Este modpack usa la versión de Minecraft y el cargador indicados por su autor.".into());
                }
                if (instance["mcVersion"] != version || instance["category"] != loader)
                    && instance["installedContent"]
                        .as_array()
                        .is_some_and(|items| !items.is_empty())
                {
                    return Err(
                        "Quita el contenido de la instancia antes de cambiar su versión o cargador."
                            .into(),
                    );
                }
                instance["name"] = json!(name);
                instance["mcVersion"] = json!(version);
                instance["category"] = json!(loader);
                instance["loaderLabel"] = json!(label);
            }
            write_state_locked(&path, &state.to_string())?;
        }
        Some("duplicate") => {
            let id = request["id"].as_str().ok_or("Falta el identificador.")?;
            let mut copy = instances
                .iter()
                .find(|entry| entry["id"].as_str() == Some(id))
                .cloned()
                .ok_or("La instancia ya no existe.")?;
            let target_id = uuid::Uuid::new_v4().to_string();
            let name = copy["name"]
                .as_str()
                .ok_or("La instancia no tiene nombre.")?
                .chars()
                .take(56)
                .collect::<String>();
            copy["id"] = json!(target_id);
            copy["name"] = json!(format!("{name} (copia)"));
            copy["favorite"] = json!(false);
            if let Some(fields) = copy.as_object_mut() {
                fields.remove("official");
            }
            copy["lastPlayed"] = json!("");
            copy["playtimeHours"] = json!(0);
            instances.push(copy);
            duplicate_at(root, id, &target_id, &state.to_string())?;
        }
        Some("favorite") => {
            let id = request["id"].as_str().ok_or("Falta el identificador.")?;
            let entry = instances
                .iter_mut()
                .find(|instance| instance["id"].as_str() == Some(id))
                .ok_or("La instancia ya no existe.")?;
            entry["favorite"] = json!(!entry["favorite"].as_bool().unwrap_or(false));
            write_state_locked(&path, &state.to_string())?;
        }
        Some("instance_ram") => {
            let id = request["id"].as_str().ok_or("Falta el identificador.")?;
            let entry = instances
                .iter_mut()
                .find(|item| item["id"].as_str() == Some(id))
                .ok_or("La instancia ya no existe.")?;
            let ram = request["ram"]
                .as_u64()
                .ok_or("Asignación de RAM inválida.")?;
            if ram != 0 {
                let installed =
                    openclient_core::performance::detect_system_hardware().installed_ram_mb;
                openclient_core::performance::validate_ram(ram, installed)?;
            }
            entry["ramMb"] = json!(ram);
            write_state_locked(&path, &state.to_string())?;
        }
        Some("instance_settings") => {
            let id = request["id"].as_str().ok_or("Falta el identificador.")?;
            let entry = instances
                .iter_mut()
                .find(|item| item["id"].as_str() == Some(id))
                .ok_or("La instancia ya no existe.")?;
            instance_settings::apply(entry, request)?;
            write_state_locked(&path, &state.to_string())?;
        }
        Some("delete") => {
            let id = request["id"].as_str().ok_or("Falta el identificador.")?;
            instances.retain(|item| item["id"].as_str() != Some(id));
            delete_at(root, id, &state.to_string())?;
        }
        _ => return Err("Operación desconocida.".into()),
    }
    Ok(state["instances"].take())
}

fn portable_metadata(entry: &Value) -> Result<Value, String> {
    let name = entry["name"].as_str().unwrap_or_default().trim();
    if name.is_empty() || name.chars().count() > 64 || name.chars().any(char::is_control) {
        return Err("El nombre de la instancia no es válido.".into());
    }
    let version = entry["mcVersion"].as_str().unwrap_or_default();
    if version.is_empty()
        || version.len() > 40
        || !version.as_bytes()[0].is_ascii_digit()
        || !version
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b".-_".contains(&byte))
    {
        return Err("La versión de Minecraft no es válida.".into());
    }
    let loader = entry["category"].as_str().unwrap_or_default();
    let label = match loader {
        "vanilla" => "Vanilla",
        "fabric" => "Fabric",
        "quilt" => "Quilt",
        "forge" => "Forge",
        "neoforge" => "NeoForge",
        _ => return Err("La instancia debe usar Vanilla, Fabric, Quilt, Forge o NeoForge.".into()),
    };
    let fabric = entry["fabricVersion"].as_str();
    if fabric.is_some_and(|version| {
        loader != "fabric" || !openclient_core::modpack::fabric_version_valid(version)
    }) {
        return Err("La versión de Fabric no es válida.".into());
    }
    let valid_id = |value: &Value| {
        value.as_str().is_some_and(|id| {
            !id.is_empty() && id.len() <= 32 && id.bytes().all(|byte| byte.is_ascii_alphanumeric())
        })
    };
    let content = entry["installedContent"]
        .as_array()
        .ok_or("La lista de contenido no es válida.")?;
    if content.len() > 256 {
        return Err("La lista de contenido tiene demasiados proyectos.".into());
    }
    let mut mods = Vec::with_capacity(content.len());
    let mut seen = std::collections::HashSet::new();
    for item in content {
        let kind = item["type"].as_str().unwrap_or("mod");
        let modded = matches!(loader, "fabric" | "quilt" | "forge" | "neoforge");
        if kind != "resourcepack" && !(modded && matches!(kind, "mod" | "shader")) {
            return Err("La lista de contenido no es compatible con esta instancia.".into());
        }
        if !valid_id(&item["id"])
            || !valid_id(&item["versionId"])
            || !seen.insert(item["id"].as_str())
        {
            return Err("La exportación contiene identificadores de contenido inválidos.".into());
        }
        let mut mod_entry = json!({
            "id": item["id"], "versionId": item["versionId"], "type": kind, "source": "modrinth",
            "title": item["title"].as_str().unwrap_or_default().chars().take(150).collect::<String>(),
            "version": item["version"].as_str().unwrap_or_default().chars().take(100).collect::<String>(),
            "enabled": item["enabled"].as_bool().unwrap_or(false)
        });
        match (&item["addedAs"], &item["parent"]) {
            (Value::Null, Value::Null) => {}
            (Value::String(added), parent)
                if added == "optional" && valid_id(parent) && parent != &item["id"] =>
            {
                mod_entry["addedAs"] = json!("optional");
                mod_entry["parent"] = parent.clone();
            }
            _ => return Err("La exportación contiene un origen de contenido inválido.".into()),
        }
        mods.push(mod_entry);
    }
    let modpack = portable_modpack(&entry["modpack"], &valid_id)?;
    let modpack_version = match &modpack {
        Some(pack) => pack["versionNumber"].as_str().map(str::to_owned),
        None => entry["modpackVersion"].as_str().map(|version| {
            version.chars().filter(|character| !character.is_control()).take(100).collect::<String>()
        }),
    };
    let mut metadata = json!({
        "name": name, "mcVersion": version, "category": loader, "loaderLabel": label,
        "installedContent": mods, "favorite": false, "lastPlayed": "", "playtimeHours": 0,
        "accentColor": "#8494ac", "fabricVersion": fabric,
        "modpackVersion": modpack_version
    });
    if let Some(pack) = modpack {
        metadata["modpack"] = pack;
    }
    Ok(metadata)
}

/// Revalida el vínculo con un modpack; cualquier clave desconocida se descarta.
fn portable_modpack(pack: &Value, valid_id: &dyn Fn(&Value) -> bool) -> Result<Option<Value>, String> {
    use openclient_core::modpack::loader_version_valid;
    if pack.is_null() {
        return Ok(None);
    }
    let invalid = || "El vínculo con el modpack no es válido.".to_string();
    let text = |key: &str, max: usize| {
        pack[key]
            .as_str()
            .filter(|value| {
                !value.is_empty() && value.chars().count() <= max && !value.chars().any(char::is_control)
            })
            .ok_or_else(invalid)
    };
    let optional_id = |key: &str| match &pack[key] {
        Value::Null => Ok(Value::Null),
        value if valid_id(value) => Ok(value.clone()),
        _ => Err(invalid()),
    };
    let source = pack["source"].as_str().unwrap_or_default();
    let loader = pack["loader"].as_str().unwrap_or_default();
    let loader_version = match &pack["loaderVersion"] {
        Value::Null => Value::Null,
        Value::String(version) if loader != "vanilla" && loader_version_valid(version) => json!(version),
        _ => return Err(invalid()),
    };
    if !pack.is_object()
        || !matches!(source, "modrinth" | "official" | "file" | "curseforge")
        || !matches!(loader, "vanilla" | "fabric" | "quilt" | "forge" | "neoforge")
    {
        return Err(invalid());
    }
    Ok(Some(json!({
        "source": source, "projectId": optional_id("projectId")?, "versionId": optional_id("versionId")?,
        "versionNumber": text("versionNumber", 100)?, "name": text("name", 64)?,
        "loader": loader, "loaderVersion": loader_version
    })))
}

/// # Safety
/// `value` debe ser nulo o un puntero devuelto por esta biblioteca, aún no liberado.
/// No modificarlo, desplazarlo ni liberarlo con el asignador de C++.
#[no_mangle]
pub unsafe extern "C" fn openclient_free_string(value: *mut c_char) {
    if !value.is_null() {
        drop(CString::from_raw(value));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CStr;

    #[test]
    fn request_sets_download_concurrency_within_limits() {
        let _ = launcher_operation(&json!({"action": "unknown", "downloadConcurrency": 99}));
        assert_eq!(openclient_core::download_concurrency(), 20);
        let _ = launcher_operation(&json!({"action": "unknown", "downloadConcurrency": 0}));
        assert_eq!(openclient_core::download_concurrency(), 1);
        openclient_core::set_download_concurrency(openclient_core::DEFAULT_DOWNLOADS);
    }

    #[test]
    fn hardware_abi_returns_owned_json_without_credentials() {
        assert_eq!(openclient_abi_version(), 3);
        let pointer = openclient_hardware_json();
        assert!(!pointer.is_null());
        let text = unsafe { CStr::from_ptr(pointer) }
            .to_str()
            .unwrap()
            .to_owned();
        unsafe { openclient_free_string(pointer) };
        let value: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(value["ok"], true);
        assert!(value["hardware"]["installed_ram_mb"].as_u64().unwrap() > 0);
        assert!(!text.contains("token"));
        assert!(!text.contains("username"));
    }

    #[test]
    fn inspect_rejects_missing_and_non_pack_files() {
        assert!(launcher_operation(&json!({"action": "modpack_inspect"})).is_err());
        let result =
            launcher_operation(&json!({"action": "modpack_inspect", "file": "relativo.mrpack"}));
        assert!(result.is_err());
    }

    /// Manual: OPENCLIENT_TEST_MRPACK=<ruta> cargo test inspect_real -- --ignored --nocapture
    #[test]
    #[ignore = "Requires a local .mrpack"]
    fn inspect_real_pack() {
        let file = std::env::var("OPENCLIENT_TEST_MRPACK").unwrap();
        let summary =
            launcher_operation(&json!({"action": "modpack_inspect", "file": file})).unwrap();
        println!(
            "{} {} mc {} fabric {} | mods {} shaders {} resourcepacks {}",
            summary["name"],
            summary["version"],
            summary["minecraft"],
            summary["fabric"],
            summary["mods"].as_array().unwrap().len(),
            summary["shaders"].as_array().unwrap().len(),
            summary["resourcepacks"].as_array().unwrap().len()
        );
        println!("{}", summary["resourcepacks"]);
        assert_eq!(summary["minecraft"], "1.21.11");
    }

    /// Manual y con red: OPENCLIENT_TEST_MRPACK=<ruta> cargo test install_real -- --ignored --nocapture
    #[test]
    #[ignore = "Requires a local .mrpack and network access"]
    fn install_real_pack_into_temporary_library() {
        let file = std::env::var("OPENCLIENT_TEST_MRPACK").unwrap();
        let root = std::env::temp_dir().join(format!("openclient-real-install-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let instances = launcher_operation(&json!({
            "action": "modpack_import", "root": root.to_string_lossy(), "file": file, "official": "diosesmc"
        }))
        .unwrap();
        let entry = &instances[0];
        assert_eq!(entry["official"], "diosesmc");
        assert_eq!(entry["modpackVersion"], "1.0.0");
        let game = root.join("instances").join(entry["id"].as_str().unwrap());
        let mods = std::fs::read_dir(game.join("mods")).unwrap().count();
        println!("mods instalados: {mods}; options.txt: {}", game.join("options.txt").is_file());
        assert!(mods >= 60 && game.join("options.txt").is_file());
        std::fs::remove_dir_all(root).unwrap();
    }

    fn temp_library() -> (std::path::PathBuf, std::path::PathBuf) {
        let base = std::env::temp_dir().join(format!("openclient-ffi-library-{}", uuid::Uuid::new_v4()));
        let root = base.join("datos");
        std::fs::create_dir_all(&root).unwrap();
        (base, root)
    }

    #[test]
    fn exported_instance_travels_without_local_settings_and_imports_as_a_new_instance() {
        let (base, root) = temp_library();
        let id = uuid::Uuid::new_v4().to_string();
        let entry = json!({
            "id": id, "name": "Mi instancia", "mcVersion": "1.21.4", "category": "fabric", "favorite": true,
            "playtimeHours": 12.5, "ramMb": 4096, "official": "oficial",
            "installedContent": [{"id": "AANobbMI", "versionId": "abc123", "type": "mod", "source": "curseforge",
                "title": "Sodium", "version": "1.0", "enabled": true, "downloadUrl": "https://evil.example/x.jar"}],
            "preLaunch": "evil.exe", "postExit": "evil.exe", "wrapper": "evil.exe", "javaPath": "C:\\x\\java.exe",
            "jvmArgs": "-Dx=1", "notes": "privado", "joinServer": "evil.example"
        });
        std::fs::write(root.join("launcher_state.json"), json!({"instances": [entry]}).to_string()).unwrap();
        let game = root.join("instances").join(&id);
        std::fs::create_dir_all(game.join("mods")).unwrap();
        std::fs::create_dir_all(game.join("logs")).unwrap();
        std::fs::write(game.join("mods/sodium.jar"), b"jar").unwrap();
        std::fs::write(game.join("options.txt"), b"fov:90").unwrap();
        std::fs::write(game.join("logs/openclient-latest.log"), b"log").unwrap();
        let destination = base.join("destino");
        std::fs::create_dir_all(&destination).unwrap();

        let exported = launcher_operation(&json!({
            "action": "export", "root": root, "id": id, "directory": destination
        }))
        .unwrap();
        let folder = std::path::PathBuf::from(exported["folder"].as_str().unwrap());
        let manifest: Value =
            serde_json::from_slice(&std::fs::read(folder.join("openclient-instance.json")).unwrap()).unwrap();
        let shared = &manifest["instance"];
        for key in [
            "id", "preLaunch", "postExit", "wrapper", "javaPath", "jvmArgs", "notes", "joinServer",
            "ramMb", "official",
        ] {
            assert!(shared.get(key).is_none(), "{key} viaja en la exportación");
        }
        assert!(!manifest.to_string().contains("evil"));
        assert_eq!(shared["name"], "Mi instancia");
        assert_eq!(shared["favorite"], false);
        assert_eq!(shared["playtimeHours"], 0);
        assert_eq!(shared["installedContent"][0]["source"], "modrinth");
        assert!(shared["installedContent"][0].get("downloadUrl").is_none());
        assert!(folder.join("game/mods/sodium.jar").is_file());
        assert!(!folder.join("game/logs").exists());

        let instances = launcher_operation(&json!({"action": "import", "root": root, "directory": folder})).unwrap();
        let instances = instances.as_array().unwrap();
        assert_eq!(instances.len(), 2);
        let copy = &instances[1];
        let copy_id = copy["id"].as_str().unwrap();
        assert!(uuid::Uuid::parse_str(copy_id).is_ok() && copy_id != id);
        assert_eq!(copy["name"], "Mi instancia");
        for key in ["preLaunch", "postExit", "wrapper", "javaPath", "jvmArgs", "notes", "joinServer", "ramMb"] {
            assert!(copy.get(key).is_none(), "{key} aparece en la instancia importada");
        }
        let imported = root.join("instances").join(copy_id);
        assert_eq!(std::fs::read(imported.join("options.txt")).unwrap(), b"fov:90");
        assert_eq!(std::fs::read(imported.join("mods/sodium.jar")).unwrap(), b"jar");
        assert!(!imported.join("logs").exists());
        let saved: Value =
            serde_json::from_str(&std::fs::read_to_string(root.join("launcher_state.json")).unwrap()).unwrap();
        assert_eq!(saved["instances"].as_array().unwrap().len(), 2);
        // La instancia original conserva sus ajustes locales.
        assert_eq!(saved["instances"][0]["preLaunch"], "evil.exe");
        std::fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn export_and_import_requests_are_validated_before_touching_files() {
        let (base, root) = temp_library();
        let id = uuid::Uuid::new_v4().to_string();
        let state = json!({"instances": [{"id": id, "name": "n", "mcVersion": "1.21.4", "category": "vanilla", "installedContent": []}]});
        std::fs::write(root.join("launcher_state.json"), state.to_string()).unwrap();
        let destination = base.join("destino");
        std::fs::create_dir_all(&destination).unwrap();
        let fail = |request: Value| launcher_operation(&request).unwrap_err();
        assert_eq!(fail(json!({"action": "export", "root": root, "directory": destination})), "Falta el identificador.");
        assert_eq!(fail(json!({"action": "export", "root": root, "id": id})), "Elige una carpeta de destino.");
        assert_eq!(
            fail(json!({"action": "export", "root": root, "id": uuid::Uuid::new_v4().to_string(), "directory": destination})),
            "La instancia ya no existe."
        );
        // Destino dentro de la biblioteca de datos.
        assert!(fail(json!({"action": "export", "root": root, "id": id, "directory": root})).contains("fuera del directorio"));
        assert_eq!(fail(json!({"action": "import", "root": root})), "Elige una carpeta de instancia.");
        assert!(fail(json!({"action": "import", "root": root, "directory": destination})).contains("openclient-instance.json"));
        assert_eq!(fail(json!({"action": "export", "root": "relativa", "id": id})), "Carpeta de datos inválida.");
        assert_eq!(fail(json!({"action": "borrar_todo", "root": root})), "Operación desconocida.");
        assert_eq!(std::fs::read_dir(&destination).unwrap().count(), 0);
        assert_eq!(std::fs::read_to_string(root.join("launcher_state.json")).unwrap(), state.to_string());
        std::fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn portable_metadata_validates_name_loader_and_content_before_export() {
        let base = json!({"name": " Mi mundo ", "mcVersion": "1.21.4", "category": "forge", "installedContent": []});
        let metadata = portable_metadata(&base).unwrap();
        assert_eq!(metadata["name"], "Mi mundo");
        assert_eq!(metadata["loaderLabel"], "Forge");
        let with = |field: &str, value: Value| {
            let mut entry = base.clone();
            entry[field] = value;
            entry
        };
        let item = |id: &str| json!({"id": id, "versionId": "v1", "type": "mod"});
        let mut vanilla_mod = with("category", json!("vanilla"));
        vanilla_mod["installedContent"] = json!([item("mod1")]);
        let bad = [
            with("name", json!("")),
            with("name", json!("a".repeat(65))),
            with("name", json!("con\ncontrol")),
            with("mcVersion", json!("../1.21")),
            with("mcVersion", json!("snapshot")),
            with("category", json!("rift")),
            with("fabricVersion", json!("0.16.9")),
            with("installedContent", Value::Null),
            with("installedContent", json!([item("a b")])),
            with("installedContent", json!([item("dup1"), item("dup1")])),
            with("installedContent", json!([{"id": "x1", "type": "mod"}])),
            with("installedContent", json!([{"id": "x1", "versionId": "v1", "type": "modpack"}])),
            with("installedContent", json!((0..257).map(|n| item(&format!("m{n}"))).collect::<Vec<_>>())),
            vanilla_mod,
        ];
        for entry in bad {
            assert!(portable_metadata(&entry).is_err(), "{entry}");
        }
        // Un paquete de recursos sí es válido en Vanilla; `enabled` ausente significa desactivado.
        let vanilla = json!({"name": "v", "mcVersion": "1.21.4", "category": "vanilla",
            "installedContent": [{"id": "pack1", "versionId": "v1", "type": "resourcepack", "source": "otra"}]});
        let metadata = portable_metadata(&vanilla).unwrap();
        assert_eq!(metadata["installedContent"][0]["enabled"], false);
        assert_eq!(metadata["installedContent"][0]["source"], "modrinth");
        // Fabric admite una versión de cargador válida.
        let fabric = json!({"name": "f", "mcVersion": "1.21.4", "category": "fabric", "fabricVersion": "0.16.9", "installedContent": []});
        assert_eq!(portable_metadata(&fabric).unwrap()["fabricVersion"], "0.16.9");
    }

    fn pack_link() -> Value {
        json!({"source": "modrinth", "projectId": "AbC123", "versionId": "XyZ789", "versionNumber": "1.4.2",
            "name": "Mi Pack", "loader": "fabric", "loaderVersion": "0.16.14"})
    }

    #[test]
    fn portable_metadata_keeps_a_valid_modpack_link_and_drops_extra_keys() {
        let mut entry = json!({"name": "p", "mcVersion": "1.21.4", "category": "fabric", "installedContent": []});
        assert!(portable_metadata(&entry).unwrap().get("modpack").is_none());
        let mut link = pack_link();
        link["downloadUrl"] = json!("https://evil.example/x");
        link["files"] = json!([{"path": "mods/a.jar"}]);
        entry["modpack"] = link;
        entry["modpackVersion"] = json!("viejo");
        let metadata = portable_metadata(&entry).unwrap();
        assert_eq!(metadata["modpack"], pack_link());
        assert_eq!(metadata["modpackVersion"], "1.4.2");
        assert!(!metadata.to_string().contains("evil"));
        entry["modpack"]["projectId"] = Value::Null;
        entry["modpack"]["loaderVersion"] = Value::Null;
        assert!(portable_metadata(&entry).is_ok());
        for (key, value) in [
            ("projectId", json!("a b")), ("versionId", json!("x".repeat(33))), ("source", json!("evil")),
            ("loader", json!("rift")), ("loaderVersion", json!("../1")), ("versionNumber", json!("a".repeat(101))),
            ("versionNumber", json!("a
b")), ("name", json!("a".repeat(65))), ("name", json!("")),
        ] {
            let mut bad = entry.clone();
            bad["modpack"] = pack_link();
            bad["modpack"][key] = value;
            assert!(portable_metadata(&bad).is_err(), "{key}");
        }
        entry["modpack"] = json!("texto");
        assert!(portable_metadata(&entry).is_err());
    }

    #[test]
    fn portable_metadata_validates_optional_content_origin() {
        let with = |extra: Value| {
            let mut item = json!({"id": "opt1", "versionId": "v1", "type": "mod"});
            for (key, value) in extra.as_object().unwrap() {
                item[key] = value.clone();
            }
            portable_metadata(&json!({"name": "p", "mcVersion": "1.21.4", "category": "fabric", "installedContent": [item]}))
        };
        let ok = with(json!({"addedAs": "optional", "parent": "par1"})).unwrap();
        assert_eq!(ok["installedContent"][0]["addedAs"], "optional");
        assert_eq!(ok["installedContent"][0]["parent"], "par1");
        assert!(with(json!({})).unwrap()["installedContent"][0].get("addedAs").is_none());
        for bad in [
            json!({"addedAs": "x", "parent": "par1"}),
            json!({"addedAs": "optional"}),
            json!({"parent": "par1"}),
            json!({"addedAs": "optional", "parent": "a b"}),
            json!({"addedAs": "optional", "parent": "opt1"}),
        ] {
            assert!(with(bad.clone()).is_err(), "{bad}");
        }
    }

    #[test]
    fn edit_refuses_version_or_loader_changes_on_linked_modpacks_and_new_actions_respond() {
        let (base, root) = temp_library();
        let id = uuid::Uuid::new_v4().to_string();
        let state = json!({"instances": [{"id": id, "name": "n", "mcVersion": "1.21.4", "category": "fabric",
            "installedContent": [], "modpack": pack_link()}]});
        std::fs::write(root.join("launcher_state.json"), state.to_string()).unwrap();
        let edit = |version: &str, loader: &str| {
            launcher_operation(&json!({"action": "edit", "root": root, "id": id, "name": "n", "version": version, "loader": loader}))
        };
        assert!(edit("1.20.1", "fabric").unwrap_err().contains("autor"));
        assert!(edit("1.21.4", "quilt").unwrap_err().contains("autor"));
        assert!(edit("1.21.4", "fabric").is_ok());
        // El estado global de operación lo comparten las pruebas de `operation`: hay que serializar su lectura.
        let _serial = operation::test_lock();
        assert_eq!(launcher_operation(&json!({"action": "operation_status"})).unwrap()["active"], false);
        assert_eq!(launcher_operation(&json!({"action": "operation_cancel", "target": "content"})).unwrap()["cancelled"], false);
        assert_eq!(launcher_operation(&json!({"action": "operation_cancel", "target": "game"})).unwrap()["cancelled"], false);
        std::fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn null_can_be_freed() {
        unsafe { openclient_free_string(std::ptr::null_mut()) };
    }
}
