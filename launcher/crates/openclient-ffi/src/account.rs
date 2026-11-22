use openclient_core::{identity::MinecraftAccount, microsoft};
use serde_json::{json, Value};
use std::{
    path::{Path, PathBuf},
    sync::Mutex,
};

static PROMPT: Mutex<(u64, Option<microsoft::DevicePrompt>)> = Mutex::new((0, None));

/// Archivo de sesión de la cuenta `accountId` (`sessions/<id>.dpapi`).
pub fn session_path(root: &Path, account_id: &str) -> Result<PathBuf, String> {
    microsoft::account_path(root, account_id)
}

fn public(account: MinecraftAccount) -> Value {
    json!({
        "id": account.id,
        "username": account.username,
        "uuid": account.uuid,
        "skinUrl": account.skin_url,
        "skinVariant": account.skin_variant,
        "capeId": account.cape_id,
        "capes": account.capes,
        "expiresAt": account.expires_at
    })
}

fn summary(entry: microsoft::AccountSummary) -> Value {
    let mut value = entry.account.map(public).unwrap_or_else(|| json!({"id": entry.id}));
    value["status"] = json!(entry.status);
    value
}

pub fn operation(action: &str, request: &Value) -> Result<Value, String> {
    let attempt = request["attempt"].as_u64().unwrap_or_default();
    match action {
        "auth_prepare" => {
            let mut state = PROMPT.lock().unwrap();
            let id = microsoft::begin_attempt();
            *state = (id, None);
            return Ok(json!(id));
        }
        "auth_prompt" | "auth_cancel" => {
            let mut state = PROMPT.lock().unwrap();
            if attempt == 0 || state.0 != attempt {
                return Ok(Value::Null);
            }
            if action == "auth_cancel" {
                microsoft::cancel_attempt();
                *state = (0, None);
                return Ok(Value::Null);
            }
            return Ok(json!(state.1));
        }
        _ => {}
    }

    let root = Path::new(
        request["root"]
            .as_str()
            .ok_or("Falta la carpeta de datos.")?,
    );
    if !root.is_absolute() || !root.is_dir() {
        return Err("Carpeta de datos inválida.".into());
    }
    if action == "auth_accounts" {
        let list = microsoft::list_accounts(root)?;
        return Ok(Value::Array(list.into_iter().map(summary).collect()));
    }
    if action == "auth_profile" {
        // Sin `accountId` solo se responde si hay una única cuenta.
        let list = microsoft::list_accounts(root)?;
        let only = list.len() == 1;
        let wanted = request["accountId"].as_str();
        let chosen = list
            .into_iter()
            .find(|entry| entry.status == "ok" && wanted.map_or(only, |id| entry.id == id));
        return Ok(chosen.and_then(|entry| entry.account).map(public).unwrap_or(Value::Null));
    }
    if action == "auth_login" {
        microsoft::list_accounts(root)?; // migra la sesión única antigua
    }
    let account = |request: &Value| -> Result<PathBuf, String> {
        session_path(
            root,
            request["accountId"].as_str().ok_or("Elige una cuenta de Microsoft.")?,
        )
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| "No se pudo iniciar la operación de cuenta.")?;
    if action == "auth_diagnose" {
        let id = request["accountId"].as_str().ok_or("Elige una cuenta de Microsoft.")?;
        let found = runtime.block_on(microsoft::diagnose(root, id))?;
        return Ok(json!({
            "code": found.code, "action": found.action, "detail": found.detail,
            "steps": found.steps.iter().map(|step| json!({"name": step.name, "state": step.state})).collect::<Vec<_>>(),
            "profile": found.account.map(public),
        }));
    }
    if action == "auth_forget" {
        runtime.block_on(microsoft::forget_session(&account(request)?))?;
        return Ok(Value::Null);
    }
    if action == "auth_refresh" {
        return runtime.block_on(microsoft::refresh_profile(&account(request)?)).map(public);
    }
    if action == "auth_equip_cape" {
        return runtime
            .block_on(microsoft::equip_cape(&account(request)?, request["capeId"].as_str()))
            .map(public);
    }
    if attempt == 0 || PROMPT.lock().unwrap().0 != attempt {
        return Err("Inicio de sesión cancelado.".into());
    }
    let loopback = request["mode"].as_str() == Some("loopback");
    let result = runtime.block_on(microsoft::login_with_mode(root, attempt, loopback, |prompt| {
        let mut state = PROMPT.lock().unwrap();
        if state.0 != attempt {
            return Err("Inicio de sesión cancelado.".into());
        }
        state.1 = Some(prompt.clone());
        Ok(())
    }));
    let mut state = PROMPT.lock().unwrap();
    if state.0 == attempt {
        *state = (0, None);
    }
    result.map(public)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn account_actions_validate_the_account_and_work_on_an_empty_library() {
        let root = std::env::temp_dir().join(format!("openclient-ffi-accounts-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let request = |extra: Value| {
            let mut value = json!({"root": root});
            value.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
            value
        };
        assert_eq!(operation("auth_accounts", &request(json!({}))).unwrap(), json!([]));
        assert_eq!(operation("auth_profile", &request(json!({}))).unwrap(), Value::Null);
        assert_eq!(operation("auth_profile", &request(json!({"accountId": "ms-aaaa"}))).unwrap(), Value::Null);
        for action in ["auth_forget", "auth_refresh", "auth_equip_cape", "auth_diagnose"] {
            assert!(operation(action, &request(json!({}))).is_err(), "{action}");
            assert!(operation(action, &request(json!({"accountId": "../x"}))).is_err(), "{action}");
        }
        assert!(session_path(&root, "ms-aaaa").unwrap().starts_with(root.join("sessions")));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn profile_response_has_no_credential_fields() {
        let mut account = openclient_core::identity::login_offline("Prueba".into()).unwrap();
        account.access_token = "private-test-token".into();
        let response = public(account);
        assert_eq!(response["username"], "Prueba");
        assert!(!response.to_string().contains("private-test-token"));
        assert!(response.get("access_token").is_none());
    }
}
