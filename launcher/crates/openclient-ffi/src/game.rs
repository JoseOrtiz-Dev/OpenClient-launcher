use openclient_core::{
    game_launcher::{self, LaunchAccount, LaunchHooks, LaunchRequest},
    hooks,
    identity,
    library_state::{load_state_at, write_state_locked},
    performance::GodOptimizationConfig,
    CancelToken, GameContext,
};
use serde_json::{json, Value};
use std::{path::Path, sync::Mutex};

static STATE: Mutex<Option<Value>> = Mutex::new(None);
/// Token del lanzamiento en fase `preparing`; se limpia en `game-started`, al salir o en error.
static GAME_CANCEL: Mutex<Option<CancelToken>> = Mutex::new(None);

/// Cancela el lanzamiento solo mientras prepara (descargas); nunca con el proceso ya lanzado.
pub fn cancel_launch() -> bool {
    let state = STATE.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if !state.as_ref().is_some_and(|s| s["phase"] == "preparing") {
        return false;
    }
    match GAME_CANCEL.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).as_ref() {
        Some(token) => {
            token.cancel();
            true
        }
        None => false,
    }
}

fn clear_cancel() {
    *GAME_CANCEL.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
}

/// Revierte una actualización de modpack interrumpida antes de lanzar (barato: solo mira
/// `snapshots/<id>/.applying`).
fn recover_pending(root: &Path, id: &str) -> Result<(), String> {
    openclient_core::modpack_tx::recover(root, id).map(|_| ())
}

pub fn status() -> Value {
    STATE
        .lock()
        .unwrap()
        .clone()
        .unwrap_or_else(|| json!({"phase": "idle"}))
}

pub(crate) fn selection(request: &Value) -> Result<(&Path, Value), String> {
    let root = Path::new(
        request["root"]
            .as_str()
            .ok_or("Falta la carpeta de datos.")?,
    );
    if !root.is_absolute() || !root.is_dir() {
        return Err("Carpeta de datos inválida.".into());
    }
    let id = request["id"].as_str().ok_or("Selecciona una instancia.")?;
    if uuid::Uuid::parse_str(id).is_err() {
        return Err("Identificador de instancia inválido.".into());
    }
    let raw =
        load_state_at(&root.join("launcher_state.json"))?.ok_or("La biblioteca está vacía.")?;
    let state: Value = serde_json::from_str(&raw).map_err(|_| "Biblioteca inválida.")?;
    let instance = state["instances"]
        .as_array()
        .and_then(|items| items.iter().find(|item| item["id"].as_str() == Some(id)))
        .ok_or("La instancia ya no existe.")?;
    Ok((root, instance.clone()))
}

pub fn read_log(request: &Value) -> Result<Value, String> {
    let (root, instance) = selection(request)?;
    game_launcher::read_instance_log(root, instance["id"].as_str().unwrap().to_owned())
        .map(Value::String)
}

pub fn java_operation(action: &str, request: &Value) -> Result<Value, String> {
    let root = Path::new(
        request["root"]
            .as_str()
            .ok_or("Falta la carpeta de datos.")?,
    );
    if !root.is_absolute() || !root.is_dir() {
        return Err("Carpeta de datos inválida.".into());
    }
    let major = request["major"]
        .as_u64()
        .and_then(|v| u8::try_from(v).ok())
        .filter(|m| matches!(m, 8 | 16 | 17 | 21 | 25))
        .ok_or("Versión de Java no admitida.")?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| format!("No se pudo iniciar la verificación de Java: {error}"))?;
    let status = if action == "java_install" {
        let context = GameContext::new(root.to_owned(), "0.1.0-preview.25", |_, _| Ok(()));
        runtime.block_on(openclient_core::java_runtime::install_java_major(context, major))?
    } else {
        let custom = request["path"].as_str().map(str::to_owned);
        runtime.block_on(openclient_core::java_runtime::check_java_major(
            root.to_owned(),
            major,
            custom,
        ))?
    };
    serde_json::to_value(status).map_err(|error| error.to_string())
}

/// Sincroniza el contenido con el contexto de la operación en curso en este hilo (si la hay).
pub fn sync_content(request: &Value) -> Result<Value, String> {
    let context = match crate::operation::current() {
        Some(context) => context,
        None => crate::operation::detached_context(request)?,
    };
    sync_content_with(request, &context)
}

pub fn sync_content_with(request: &Value, context: &GameContext) -> Result<Value, String> {
    let (_, instance) = selection(request)?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| format!("No se pudo iniciar el motor: {error}"))?;
    runtime.block_on(game_launcher::sync_instance_content(
        context,
        instance["id"].as_str().unwrap(),
        instance["mcVersion"]
            .as_str()
            .ok_or("Versión de Minecraft inválida.")?,
        instance["category"].as_str().ok_or("Cargador inválido.")?,
        &catalog_mods(&instance),
    ))?;
    Ok(Value::Null)
}

pub fn launch(request: &Value) -> Result<Value, String> {
    let (root, instance) = selection(request)?;
    recover_pending(root, instance["id"].as_str().unwrap())?;
    let microsoft = request["accountType"] == "microsoft";
    let session = if microsoft {
        crate::account::session_path(
            root,
            request["accountId"]
                .as_str()
                .ok_or("SESSION_REAUTH: Elige una cuenta de Microsoft desde Cuenta.")?,
        )?
    } else {
        std::path::PathBuf::new()
    };
    let user = if microsoft {
        openclient_core::microsoft::saved_profile(&session)?
            .filter(|account| request["accountId"] == account.id)
            .ok_or("SESSION_REAUTH: Inicia sesión con Microsoft desde Cuenta.")?
    } else {
        identity::login_offline(request["username"].as_str().unwrap_or_default().to_owned())?
    };
    let effective = |instance_key: &str, global_key: &str| -> Result<String, String> {
        let own = instance[instance_key].as_str().unwrap_or_default().trim();
        let chosen = if own.is_empty() {
            request[global_key].as_str().unwrap_or_default()
        } else {
            own
        };
        hooks::validate(chosen)
    };
    let launch_hooks = LaunchHooks {
        pre_launch: effective("preLaunch", "preLaunchHook")?,
        post_exit: effective("postExit", "postExitHook")?,
        wrapper: effective("wrapper", "wrapperCommand")?,
    };
    let global_args: String = request["customJvmArgs"].as_str().unwrap_or_default().chars().take(512).collect();
    let own_args: String = instance["jvmArgs"].as_str().unwrap_or_default().chars().take(512).collect();
    let jvm_args = if instance["useGlobalJvmArgs"].as_bool() == Some(false) {
        own_args
    } else {
        format!("{global_args} {own_args}")
    };
    let own_window = match (instance["windowWidth"].as_u64(), instance["windowHeight"].as_u64()) {
        (Some(width @ 320..=7680), Some(height @ 240..=4320)) => Some((width, height)),
        _ => None,
    };
    let launch = LaunchRequest {
        instance_id: instance["id"].as_str().unwrap().into(),
        mc_version: instance["mcVersion"].as_str().unwrap_or_default().into(),
        loader: instance["category"].as_str().unwrap_or_default().into(),
        loader_version: loader_version(&instance),
        account: LaunchAccount {
            username: user.username,
            uuid: user.uuid,
            account_type: user.account_type,
            access_token: user.access_token,
        },
        optimization: GodOptimizationConfig {
            allocated_ram_mb: instance["ramMb"]
                .as_u64()
                .filter(|ram| *ram != 0)
                .or_else(|| request["ram"].as_u64())
                .ok_or("Asignación de RAM inválida.")?,
            gc_preset: match request["gcPreset"].as_str().unwrap_or("automatic") {
                preset @ ("automatic" | "g1gc" | "generational_zgc" | "low_memory") => {
                    preset.to_owned()
                }
                _ => return Err("Perfil de memoria JVM inválido.".into()),
            },
            pre_touch_memory: request["preTouchMemory"].as_bool().unwrap_or(false),
            high_process_priority: request["highPriority"].as_bool().unwrap_or(false),
            minimize_on_launch: request["minimizeOnLaunch"].as_bool().unwrap_or(false),
            refocus_on_exit: request["refocusOnExit"].as_bool().unwrap_or(false),
            window_width: own_window
                .map(|(width, _)| width)
                .or_else(|| request["windowWidth"].as_u64())
                .and_then(|v| u32::try_from(v).ok())
                .unwrap_or(0),
            window_height: own_window
                .map(|(_, height)| height)
                .or_else(|| request["windowHeight"].as_u64())
                .and_then(|v| u32::try_from(v).ok())
                .unwrap_or(0),
            fullscreen: request["fullscreen"].as_bool().unwrap_or(false),
            custom_jvm_args: jvm_args,
            java_paths: match request.get("javaPaths") {
                Some(value) => serde_json::from_value(value.clone())
                    .map_err(|_| "Configuración de Java inválida.")?,
                None => Default::default(),
            },
            ..Default::default()
        },
        mods: catalog_mods(&instance),
        java_override: instance["javaPath"].as_str().map(str::to_owned),
        hooks: launch_hooks,
        join_server: game_launcher::server_address(instance["joinServer"].as_str().unwrap_or_default())?,
    };
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|error| format!("No se pudo iniciar el motor: {error}"))?;
    {
        let mut state = STATE.lock().unwrap();
        if state
            .as_ref()
            .is_some_and(|s| matches!(s["phase"].as_str(), Some("preparing" | "running")))
        {
            return Err("Ya hay un lanzamiento activo.".into());
        }
        *state = Some(json!({"phase": "preparing", "message": "Preparando Minecraft…"}));
    }
    let cancel = CancelToken::default();
    *GAME_CANCEL.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(cancel.clone());
    let root_for_exit = root.to_owned();
    let app = GameContext::new(root.to_owned(), "0.1.0-preview.25", move |event, data| {
        let mut state = STATE.lock().unwrap();
        match event {
            "game-started" => {
                clear_cancel();
                *state = Some(json!({"phase":"running", "message":"Minecraft iniciado."}))
            }
            "game-exited" => {
                if let Some(id) = data["instance_id"].as_str() {
                    let secs = data["elapsed_seconds"].as_u64().unwrap_or(0);
                    let path = root_for_exit.join("launcher_state.json");
                    if let Ok(Some(raw)) = load_state_at(&path) {
                        if let Ok(mut lib) = serde_json::from_str::<Value>(&raw) {
                            if let Some(entry) = lib["instances"]
                                .as_array_mut()
                                .and_then(|arr| arr.iter_mut().find(|i| i["id"].as_str() == Some(id)))
                            {
                                let prev = entry["playtimeHours"].as_f64().unwrap_or(0.0);
                                let added = (secs as f64 / 3600.0).max(0.1);
                                entry["playtimeHours"] = json!(((prev + added) * 10.0).round() / 10.0);
                                entry["lastPlayed"] = json!("Reciente");
                                let _ = write_state_locked(&path, &lib.to_string());
                            }
                        }
                    }
                }
                let code = data["exit_code"].as_i64();
                let message = match code {
                    Some(0) => "Minecraft se cerró.".to_owned(),
                    Some(code) => {
                        format!("Minecraft terminó con código {code}. Consulta el registro.")
                    }
                    None => "Minecraft terminó. Consulta el registro.".to_owned(),
                };
                *state = Some(json!({"phase":"idle", "message":message, "exitCode": code, "instanceId": data["instance_id"]}));
            }
            "launch-progress" | "java-progress"
                if state.as_ref().is_some_and(|s| s["phase"] == "preparing") =>
            {
                *state = Some(
                    json!({"phase":"preparing", "message":data["message"], "percent":data["percent"]}),
                );
            }
            _ => {}
        }
        Ok(())
    })
    .with_cancel(cancel);
    match runtime.block_on(game_launcher::launch_game(app, launch, |id| async move {
        openclient_core::microsoft::ensure_microsoft_session(&session, &id, false).await
    })) {
        Ok(result) => Ok(json!({"pid":result.pid, "warnings":result.warnings})),
        Err(error) => {
            clear_cancel();
            let message = if error.starts_with("CANCELLED:") { "Lanzamiento cancelado." } else { error.as_str() };
            *STATE.lock().unwrap() = Some(json!({"phase":"idle", "message":message}));
            Err(error)
        }
    }
}

/// Versión fijada del cargador: `fabricVersion`, o la del modpack si declara el mismo cargador.
fn loader_version(instance: &Value) -> Option<String> {
    instance["fabricVersion"]
        .as_str()
        .or_else(|| {
            let pack = &instance["modpack"];
            (pack["loader"] == instance["category"])
                .then(|| pack["loaderVersion"].as_str())
                .flatten()
        })
        .map(str::to_owned)
}

fn catalog_mods(instance: &Value) -> Vec<game_launcher::LaunchMod> {
    instance["installedContent"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|item| {
            Some(game_launcher::LaunchMod {
                id: item["id"].as_str()?.to_owned(),
                version_id: item["versionId"]
                    .as_str()
                    .filter(|s| !s.is_empty() && *s != "auto")
                    .map(str::to_owned),
                source: item["source"].as_str().unwrap_or("modrinth").to_owned(),
                content_type: item["type"].as_str().unwrap_or("mod").into(),
                enabled: item["enabled"].as_bool().unwrap_or(false),
                download_url: item["downloadUrl"].as_str().map(str::to_owned),
                file_name: item["fileName"].as_str().map(str::to_owned),
                sha1: item["sha1"].as_str().map(str::to_owned),
            })
        })
        .collect()
}
