use openclient_core::{
    game_launcher::{self, LaunchAccount, LaunchHooks, LaunchRequest},
    hooks,
    identity,
    library_state::{load_state_at, write_state_locked},
    performance::GodOptimizationConfig,
    CancelToken, GameContext,
};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    path::Path,
    sync::{LazyLock, Mutex},
};

#[derive(Default)]
struct Games {
    states: HashMap<String, Value>,
    preparing: Option<(String, CancelToken)>,
    last_id: String,
}

impl Games {
    fn busy(&self) -> bool {
        self.preparing.is_some() || self.states.values().any(|state| state["phase"] == "running")
    }

    fn begin(&mut self, id: &str) -> Result<CancelToken, String> {
        if self.preparing.is_some() {
            return Err("Ya hay un lanzamiento activo.".into());
        }
        if self.states.get(id).is_some_and(|state| state["phase"] == "running") {
            return Err("Esta instancia ya está en juego.".into());
        }
        let cancel = CancelToken::default();
        self.preparing = Some((id.to_owned(), cancel.clone()));
        self.last_id = id.to_owned();
        self.states.insert(id.to_owned(), json!({
            "phase": "preparing", "message": "Preparando Minecraft…", "instanceId": id,
        }));
        Ok(cancel)
    }

    fn status(&mut self, request: &Value) -> Value {
        let id = request["id"].as_str().unwrap_or(&self.last_id);
        let Some(state) = self.states.get(id) else {
            return json!({"phase": "idle"});
        };
        if request["consume"].as_bool() == Some(true) && state["phase"] == "idle" {
            return self.states.remove(id).unwrap();
        }
        state.clone()
    }

    fn cancel_preparation(&self) -> bool {
        let Some((id, token)) = &self.preparing else {
            return false;
        };
        if !self.states.get(id).is_some_and(|state| state["phase"] == "preparing") {
            return false;
        }
        token.cancel();
        true
    }

    fn end_preparation(&mut self, id: &str, error: Option<&str>) {
        self.preparing = None;
        if let Some(error) = error {
            let message = if error.starts_with("CANCELLED:") { "Lanzamiento cancelado." } else { error };
            self.states.insert(id.to_owned(), json!({"phase":"idle", "message":message, "instanceId": id}));
        }
    }

    fn record_event(&mut self, root: &Path, id: &str, event: &str, data: Value) {
        let Some(state) = self.states.get_mut(id) else {
            return;
        };
        match event {
            "game-started" => {
                *state = json!({"phase":"running", "message":"Minecraft iniciado.", "instanceId": id});
            }
            "game-exited" => {
                record_playtime(root, id, data["elapsed_seconds"].as_u64().unwrap_or(0));
                let code = data["exit_code"].as_i64();
                let message = match code {
                    Some(0) => "Minecraft se cerró.".to_owned(),
                    Some(code) => format!("Minecraft terminó con código {code}. Consulta el registro."),
                    None => "Minecraft terminó. Consulta el registro.".to_owned(),
                };
                *state = json!({"phase":"idle", "message":message, "exitCode":code, "instanceId":id});
            }
            "launch-progress" | "java-progress" if state["phase"] == "preparing" => {
                *state = json!({
                    "phase":"preparing", "message":data["message"], "percent":data["percent"], "instanceId":id,
                });
            }
            _ => {}
        }
    }
}

static GAMES: LazyLock<Mutex<Games>> = LazyLock::new(|| Mutex::new(Games::default()));

fn record_playtime(root: &Path, id: &str, seconds: u64) {
    let path = root.join("launcher_state.json");
    let Ok(Some(raw)) = load_state_at(&path) else {
        return;
    };
    let Ok(mut library) = serde_json::from_str::<Value>(&raw) else {
        return;
    };
    let Some(entry) = library["instances"]
        .as_array_mut()
        .and_then(|instances| instances.iter_mut().find(|instance| instance["id"] == id))
    else {
        return;
    };
    let previous = entry["playtimeHours"].as_f64().unwrap_or(0.0);
    let added = (seconds as f64 / 3600.0).max(0.1);
    entry["playtimeHours"] = json!(((previous + added) * 10.0).round() / 10.0);
    entry["lastPlayed"] = json!("Reciente");
    let _ = write_state_locked(&path, &library.to_string());
}

/// Cancela el lanzamiento solo mientras prepara (descargas); nunca con el proceso ya lanzado.
pub fn cancel_launch() -> bool {
    GAMES.lock().unwrap().cancel_preparation()
}

/// Revierte una actualización de modpack interrumpida antes de lanzar (barato: solo mira
/// `snapshots/<id>/.applying`).
fn recover_pending(root: &Path, id: &str) -> Result<(), String> {
    openclient_core::modpack_tx::recover(root, id).map(|_| ())
}

pub fn status(request: &Value) -> Value {
    GAMES.lock().unwrap().status(request)
}

pub fn busy() -> bool {
    GAMES.lock().unwrap().busy()
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
    let id = launch.instance_id.clone();
    let cancel = GAMES.lock().unwrap().begin(&id)?;
    let event_id = id.clone();
    let root_for_exit = root.to_owned();
    let app = GameContext::new(root.to_owned(), "0.1.0-preview.25", move |event, data| {
        GAMES.lock().unwrap().record_event(&root_for_exit, &event_id, event, data);
        Ok(())
    })
    .with_cancel(cancel);
    let result = recover_pending(root, &id).and_then(|()| {
        runtime.block_on(game_launcher::launch_game(app, launch, |id| async move {
            openclient_core::microsoft::ensure_microsoft_session(&session, &id, false).await
        }))
    });
    GAMES.lock().unwrap().end_preparation(&id, result.as_ref().err().map(String::as_str));
    result.map(|result| json!({"pid":result.pid, "warnings":result.warnings}))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_games_keep_their_own_status_and_playtime() {
        let root = std::env::temp_dir().join(format!("openclient-games-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let library = root.join("launcher_state.json");
        std::fs::write(&library, r#"{"instances":[{"id":"first"},{"id":"second"}]}"#).unwrap();
        let mut games = Games::default();

        games.begin("first").unwrap();
        assert!(games.begin("second").is_err());
        games.record_event(&root, "first", "game-started", Value::Null);
        games.end_preparation("first", None);
        assert!(games.begin("first").is_err());

        let second = games.begin("second").unwrap();
        assert!(games.cancel_preparation());
        assert!(second.is_cancelled());
        games.end_preparation("second", Some("CANCELLED: test"));
        assert_eq!(games.status(&json!({"id":"first"}))["phase"], "running");

        games.begin("second").unwrap();
        games.record_event(&root, "second", "game-started", Value::Null);
        games.end_preparation("second", None);
        games.record_event(&root, "first", "game-exited", json!({"exit_code":0,"elapsed_seconds":3600}));
        let finished = games.status(&json!({"id":"first","consume":true}));
        assert_eq!(finished["exitCode"], 0);
        assert_eq!(games.status(&json!({"id":"second"}))["phase"], "running");
        assert!(games.busy());

        games.record_event(&root, "second", "game-exited", json!({"exit_code":7,"elapsed_seconds":7200}));
        assert_eq!(games.status(&json!({"id":"second","consume":true}))["exitCode"], 7);
        assert!(!games.busy());
        let stored: Value = serde_json::from_str(&load_state_at(&library).unwrap().unwrap()).unwrap();
        assert_eq!(stored["instances"][0]["playtimeHours"], 1.0);
        assert_eq!(stored["instances"][1]["playtimeHours"], 2.0);
        std::fs::remove_dir_all(root).unwrap();
    }
}
