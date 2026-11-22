use crate::session_store::{self, SavedSession};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
static AUTH_ATTEMPT: AtomicU64 = AtomicU64::new(0);
static SESSION_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

// Public application identifier supplied by the owner; never bundle a secret.
const MINECRAFT_CLIENT_ID: &str = "072ee81c-f9ab-4fb8-87a6-17e0589fabd3";
const MINECRAFT_SCOPE: &str = "XboxLive.signin offline_access";
const TOKEN_URL: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0/token";

fn active_client_id() -> String {
    std::env::var("OPENCLIENT_AZURE_CLIENT_ID")
        .ok()
        .and_then(|s| uuid::Uuid::parse_str(s.trim()).ok().map(|u| u.to_string()))
        .unwrap_or_else(|| MINECRAFT_CLIENT_ID.to_owned())
}

use crate::identity::MinecraftAccount;
use crate::identity::{MojangCape, MojangSkin};

// The webview only needs the profile. Game credentials stay in the native process.
pub fn account_for_interface(mut account: MinecraftAccount) -> MinecraftAccount {
    account.access_token.clear();
    account
}

#[derive(Debug, Deserialize)]
struct MsTokenResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
}

#[derive(Debug, Deserialize)]
struct XblDisplayClaims {
    xui: Vec<XblXui>,
}

#[derive(Debug, Deserialize)]
struct XblXui {
    uhs: String,
}

#[derive(Debug, Deserialize)]
struct XblAuthResponse {
    #[serde(rename = "Token")]
    token: String,
    #[serde(rename = "DisplayClaims")]
    display_claims: XblDisplayClaims,
}

#[derive(Debug, Deserialize)]
struct MinecraftXboxLoginResponse {
    access_token: String,
    expires_in: u64,
}

#[derive(Debug, Deserialize)]
struct MinecraftProfileResponse {
    id: String,
    name: String,
    #[serde(default)]
    skins: Vec<MojangSkin>,
    #[serde(default)]
    capes: Vec<MojangCape>,
}

fn url_encode(input: &str) -> String {
    let mut out = String::with_capacity(input.len() * 2);
    for b in input.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => {
                out.push_str(&format!("%{:02X}", b));
            }
        }
    }
    out
}

async fn http_post_form(client: &reqwest::Client, url: &str, body: &str) -> Result<String, String> {
    client.post(url).header("Content-Type", "application/x-www-form-urlencoded").body(body.to_owned()).send().await
        .map_err(|_| "SESSION_NETWORK: No se pudo conectar con Microsoft. Comprueba tu conexión e inténtalo otra vez.".to_string())?
        .text().await.map_err(|_| "SESSION_NETWORK: Respuesta incompleta de Microsoft.".into())
}
async fn http_post_json(client: &reqwest::Client, url: &str, body: &str) -> Result<String, String> {
    client
        .post(url)
        .header("Content-Type", "application/json")
        .body(body.to_owned())
        .send()
        .await
        .map_err(|_| {
            "SESSION_NETWORK: No se pudo conectar con Xbox/Minecraft. Inténtalo otra vez."
                .to_string()
        })?
        .text()
        .await
        .map_err(|_| "SESSION_NETWORK: Respuesta incompleta de Minecraft.".into())
}
async fn http_get_bearer(
    client: &reqwest::Client,
    url: &str,
    bearer: &str,
) -> Result<String, String> {
    client
        .get(url)
        .bearer_auth(bearer)
        .send()
        .await
        .map_err(|_| "SESSION_NETWORK: No se pudo cargar el perfil de Minecraft.".to_string())?
        .text()
        .await
        .map_err(|_| "SESSION_NETWORK: Respuesta incompleta del perfil.".into())
}
fn auth_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .https_only(true)
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(30))
        .user_agent("OpenClient/2.0")
        .build()
        .map_err(|_| "No se pudo preparar la conexión segura".into())
}
fn parse_ms_token(raw: &str) -> Result<MsTokenResponse, String> {
    serde_json::from_str(raw).map_err(|_| {
        let error = serde_json::from_str::<serde_json::Value>(raw).ok();
        match error.as_ref().and_then(|v| v["error"].as_str()) {
            Some(
                "invalid_grant" | "interaction_required" | "login_required" | "consent_required",
            ) => "SESSION_REAUTH: Microsoft requiere que vuelvas a iniciar sesión.".into(),
            Some("invalid_client" | "unauthorized_client") => {
                "La configuración de acceso Microsoft fue rechazada por el proveedor.".into()
            }
            _ => "SESSION_NETWORK: Microsoft no pudo completar la solicitud. Inténtalo más tarde."
                .into(),
        }
    })
}

#[derive(Deserialize)]
struct DeviceAuthorization {
    device_code: String,
    user_code: String,
    verification_uri: String,
    expires_in: u64,
    #[serde(default = "default_device_interval")]
    interval: u64,
}
fn default_device_interval() -> u64 {
    5
}
#[derive(Clone, Serialize)]
pub struct DevicePrompt {
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
}
fn parse_device_authorization(raw: &str) -> Result<DeviceAuthorization, String> {
    let authorization: DeviceAuthorization = serde_json::from_str(raw).map_err(|_| "Microsoft no pudo iniciar el acceso. En Azure, habilita los flujos de cliente público y las cuentas personales para OpenClient.".to_string())?;
    let url = reqwest::Url::parse(&authorization.verification_uri)
        .map_err(|_| "Dirección de Microsoft inválida")?;
    if url.scheme() != "https"
        || !matches!(url.host_str(), Some("microsoft.com" | "www.microsoft.com"))
        || !matches!(url.path(), "/devicelogin" | "/link")
        || url.query().is_some()
        || url.fragment().is_some()
        || url.port().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
        || authorization.device_code.is_empty()
        || authorization.user_code.is_empty()
        || authorization.user_code.len() > 32
        || !authorization
            .user_code
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-')
        || !(1..=900).contains(&authorization.expires_in)
        || !(1..=60).contains(&authorization.interval)
    {
        return Err("Microsoft devolvió una solicitud de acceso inválida.".into());
    }
    Ok(authorization)
}

enum DevicePoll {
    Pending,
    SlowDown,
    Authorized(MsTokenResponse),
}
fn parse_device_poll(raw: &str) -> Result<DevicePoll, String> {
    let value: serde_json::Value = serde_json::from_str(raw)
        .map_err(|_| "SESSION_NETWORK: Respuesta incompleta de Microsoft.")?;
    match value["error"].as_str() {
        Some("authorization_pending") => Ok(DevicePoll::Pending),
        Some("slow_down") => Ok(DevicePoll::SlowDown),
        Some("authorization_declined" | "access_denied") => Err("Inicio de sesión cancelado en Microsoft.".into()),
        Some("expired_token" | "code_expired") => Err("El código de Microsoft caducó. Vuelve a iniciar sesión.".into()),
        Some("invalid_client" | "unauthorized_client" | "invalid_grant" | "bad_verification_code") => Err("Microsoft rechazó el acceso. Revisa que Azure permita flujos de cliente público para OpenClient.".into()),
        Some(_) => Err("SESSION_NETWORK: Microsoft no pudo completar el acceso. Inténtalo de nuevo.".into()),
        None => Ok(DevicePoll::Authorized(parse_ms_token(raw)?)),
    }
}

async fn wait_device_interval(seconds: u64, attempt: u64) -> Result<(), String> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(seconds);
    while tokio::time::Instant::now() < deadline {
        if AUTH_ATTEMPT.load(Ordering::SeqCst) != attempt {
            return Err("Inicio de sesión cancelado.".into());
        }
        tokio::time::sleep_until(
            deadline.min(tokio::time::Instant::now() + Duration::from_millis(250)),
        )
        .await;
    }
    if AUTH_ATTEMPT.load(Ordering::SeqCst) != attempt {
        return Err("Inicio de sesión cancelado.".into());
    }
    Ok(())
}

async fn account_from_ms(
    client: &reqwest::Client,
    ms_access_token: &str,
    refresh_token: String,
) -> Result<SavedSession, String> {
    // Autenticar el token v2 de Microsoft en Xbox Live.
    let ticket_candidates = [format!("d={}", ms_access_token)];

    let mut xbl_parsed: Option<XblAuthResponse> = None;

    for rps_ticket in &ticket_candidates {
        let xbl_payload = serde_json::json!({
            "Properties": {
                "AuthMethod": "RPS",
                "SiteName": "user.auth.xboxlive.com",
                "RpsTicket": rps_ticket
            },
            "RelyingParty": "http://auth.xboxlive.com",
            "TokenType": "JWT"
        })
        .to_string();

        if let Ok(xbl_raw) = http_post_json(
            client,
            "https://user.auth.xboxlive.com/user/authenticate",
            &xbl_payload,
        )
        .await
        {
            if let Ok(parsed) = serde_json::from_str::<XblAuthResponse>(&xbl_raw) {
                xbl_parsed = Some(parsed);
                break;
            }
        }
    }

    let xbl_data = xbl_parsed
        .ok_or("No se pudo autenticar en Xbox Live. Revisa tu conexión y la configuración de tu cuenta Xbox.")?;
    let user_hash = xbl_data
        .display_claims
        .xui
        .first()
        .ok_or("Respuesta de Xbox Live sin UserHash")?
        .uhs
        .clone();

    // Token XSTS para Minecraft Services
    let xsts_payload = serde_json::json!({
        "Properties": {
            "SandboxId": "RETAIL",
            "UserTokens": [xbl_data.token]
        },
        "RelyingParty": "rp://api.minecraftservices.com/",
        "TokenType": "JWT"
    })
    .to_string();

    let xsts_raw = http_post_json(
        client,
        "https://xsts.auth.xboxlive.com/xsts/authorize",
        &xsts_payload,
    )
    .await?;

    let xsts_data: XblAuthResponse = serde_json::from_str(&xsts_raw)
        .map_err(|_| "Xbox no autorizó esta cuenta para Minecraft. Revisa que tenga un perfil Xbox y los permisos familiares necesarios.".to_string())?;

    // Inicio de sesión en Minecraft Services (API de Mojang)
    let mc_login_payload = serde_json::json!({
        "identityToken": format!("XBL3.0 x={};{}", user_hash, xsts_data.token)
    })
    .to_string();

    let mc_login_raw = http_post_json(
        client,
        "https://api.minecraftservices.com/authentication/login_with_xbox",
        &mc_login_payload,
    )
    .await?;

    let mc_login: MinecraftXboxLoginResponse = serde_json::from_str(&mc_login_raw)
        .map_err(|_| "Minecraft rechazó el acceso. Vuelve a intentarlo más tarde.".to_string())?;

    // Perfil de Minecraft Java Edition (`name`, `id`, `skins`, `capes`)
    let profile_raw = http_get_bearer(
        client,
        "https://api.minecraftservices.com/minecraft/profile",
        &mc_login.access_token,
    )
    .await?;

    let profile: MinecraftProfileResponse = serde_json::from_str(&profile_raw).map_err(|_| {
        "Esta cuenta de Microsoft no tiene un perfil de Minecraft: Java Edition.".to_string()
    })?;

    let active_skin = profile
        .skins
        .iter()
        .find(|s| s.state.eq_ignore_ascii_case("ACTIVE"))
        .or_else(|| profile.skins.first());

    let active_skin_url = active_skin
        .map(|s| s.url.replace("http://", "https://"))
        .filter(|u| !u.is_empty())
        .unwrap_or_else(|| format!("https://mc-heads.net/skin/{}", profile.name));

    let skin_variant = if active_skin
        .map(|s| s.variant.eq_ignore_ascii_case("SLIM"))
        .unwrap_or(false)
    {
        "slim".to_string()
    } else {
        "classic".to_string()
    };

    let mut cleaned_capes: Vec<MojangCape> = profile
        .capes
        .into_iter()
        .map(|mut c| {
            c.url = c.url.replace("http://", "https://");
            c
        })
        .collect();
    cleaned_capes.sort_by_key(|c| !c.state.eq_ignore_ascii_case("ACTIVE"));

    let first_cape = cleaned_capes
        .iter()
        .find(|c| c.state.eq_ignore_ascii_case("ACTIVE"))
        .or_else(|| cleaned_capes.first())
        .map(|c| c.id.clone());

    let expires_at = session_store::now().saturating_add(mc_login.expires_in);
    Ok(SavedSession {
        oauth_client_id: active_client_id(),
        refresh_token,
        expires_at,
        account: MinecraftAccount {
            id: format!("ms-{}", profile.id),
            username: profile.name,
            uuid: profile.id,
            account_type: "microsoft".to_string(),
            access_token: mc_login.access_token,
            skin_url: active_skin_url,
            skin_variant,
            cape_id: first_cape,
            capes: cleaned_capes,
            expires_at,
        },
    })
}

pub(crate) fn session_is_fresh(expires_at: u64, now: u64) -> bool {
    expires_at > now.saturating_add(120)
}

pub async fn ensure_microsoft_session(
    path: &Path,
    account_id: &str,
    force: bool,
) -> Result<MinecraftAccount, String> {
    let _guard = SESSION_LOCK.lock().await;
    let mut session = session_store::load(path)?.filter(|s| s.account.id == account_id)
        .ok_or("SESSION_REAUTH: Inicia sesión con Microsoft una vez para activar la renovación automática.")?;
    let cid = active_client_id();
    if session.oauth_client_id != cid {
        return Err("SESSION_REAUTH: OpenClient actualizó su acceso Microsoft. Inicia sesión una vez para activar la nueva sesión protegida.".into());
    }
    if !force && session_is_fresh(session.expires_at, session_store::now()) {
        return Ok(session.account);
    }
    let client = auth_client()?;
    let body = format!(
        "client_id={}&grant_type=refresh_token&refresh_token={}&scope={}",
        cid,
        url_encode(&session.refresh_token),
        url_encode(MINECRAFT_SCOPE)
    );
    let raw = http_post_form(&client, TOKEN_URL, &body).await?;
    let tokens = parse_ms_token(&raw)?;
    if let Some(rotated) = tokens.refresh_token {
        session.refresh_token = rotated;
    }
    // Save rotation before contacting Xbox; a temporary downstream failure must not lose it.
    session_store::save(path, &session)?;
    let next = account_from_ms(&client, &tokens.access_token, session.refresh_token).await?;
    if next.account.id != account_id {
        return Err(
            "SESSION_REAUTH: La sesión recibida pertenece a otra cuenta. Inicia sesión de nuevo."
                .into(),
        );
    }
    session_store::save(path, &next)?;
    Ok(next.account)
}
async fn finish_login(
    root: &Path,
    tokens: MsTokenResponse,
    attempt: u64,
) -> Result<MinecraftAccount, String> {
    let _guard = SESSION_LOCK.lock().await;
    if AUTH_ATTEMPT.load(Ordering::SeqCst) != attempt {
        return Err("Inicio de sesión cancelado.".into());
    }
    let refresh = tokens
        .refresh_token
        .ok_or("Microsoft no entregó una sesión renovable. Vuelve a iniciar sesión.")?;
    let session = account_from_ms(&auth_client()?, &tokens.access_token, refresh).await?;
    if AUTH_ATTEMPT.load(Ordering::SeqCst) != attempt {
        return Err("Inicio de sesión cancelado.".into());
    }
    let path = session_store::account_file(root, &session.account.id)?;
    if !path.exists() && session_store::account_ids(root).len() >= session_store::MAX_ACCOUNTS {
        return Err(format!(
            "Ya hay {} cuentas guardadas. Quita una antes de añadir otra.",
            session_store::MAX_ACCOUNTS
        ));
    }
    session_store::save(&path, &session)?;
    Ok(session.account)
}

pub use crate::session_store::{valid_account_id, MAX_ACCOUNTS};

/// Archivo de sesión de una cuenta (migra antes la sesión única antigua).
pub fn account_path(root: &Path, account_id: &str) -> Result<std::path::PathBuf, String> {
    session_store::migrate_legacy(root)?;
    session_store::account_file(root, account_id)
}

/// Una cuenta guardada tal como la ve la interfaz (sin tokens).
pub struct AccountSummary {
    pub id: String,
    pub account: Option<MinecraftAccount>,
    /// `ok`, `reauth` (hay que iniciar sesión otra vez) o `broken` (archivo ilegible).
    pub status: &'static str,
}

/// Cuentas de `<root>/sessions`, por nombre. Migra antes la sesión única antigua.
pub fn list_accounts(root: &Path) -> Result<Vec<AccountSummary>, String> {
    session_store::migrate_legacy(root)?;
    let cid = active_client_id();
    let mut list: Vec<AccountSummary> = session_store::account_ids(root)
        .into_iter()
        .map(|id| {
            let loaded = session_store::account_file(root, &id)
                .and_then(|path| session_store::load(&path))
                .ok()
                .flatten()
                .filter(|session| session.account.id == id);
            match loaded {
                Some(session) => AccountSummary {
                    status: if session.oauth_client_id == cid { "ok" } else { "reauth" },
                    account: Some(account_for_interface(session.account)),
                    id,
                },
                None => AccountSummary { id, account: None, status: "broken" },
            }
        })
        .collect();
    // Las ilegibles van al final.
    list.sort_by_key(|entry| {
        (
            entry.account.is_none(),
            entry.account.as_ref().map(|a| a.username.to_lowercase()).unwrap_or_default(),
            entry.id.clone(),
        )
    });
    Ok(list)
}

/// Un paso de la comprobación de cuenta: `ok`, `fail` u `skip` (no se llegó a probar).
pub struct DiagStep {
    pub name: &'static str,
    pub state: &'static str,
}

/// Resultado de `diagnose`. `code` identifica la causa y `action` lo que puede hacer la interfaz:
/// `none`, `reconnect`, `retry`, `open_xbox` o `open_minecraft`.
pub struct Diagnosis {
    pub code: &'static str,
    pub action: &'static str,
    pub detail: String,
    pub steps: Vec<DiagStep>,
    pub account: Option<MinecraftAccount>,
}

const STEPS: [&str; 5] = [
    "Sesión guardada",
    "Acceso de OpenClient",
    "Renovación con Microsoft",
    "Xbox y Minecraft",
    "Perfil de Java",
];

fn steps_with(failed: Option<usize>) -> Vec<DiagStep> {
    STEPS
        .iter()
        .enumerate()
        .map(|(index, name)| DiagStep {
            name,
            state: match failed {
                Some(at) if index == at => "fail",
                Some(at) if index > at => "skip",
                _ => "ok",
            },
        })
        .collect()
}

fn strip_prefix(error: &str) -> String {
    error
        .strip_prefix("SESSION_REAUTH:")
        .or_else(|| error.strip_prefix("SESSION_NETWORK:"))
        .unwrap_or(error)
        .trim()
        .to_owned()
}

/// Causa, acción y paso que falló (0..=4) según el mensaje de error del flujo de sesión.
fn classify(error: &str) -> (&'static str, &'static str, usize) {
    if error.starts_with("SESSION_REAUTH:") {
        ("reauth", "reconnect", 2)
    } else if error.starts_with("SESSION_NETWORK:") {
        let later = error.contains("Xbox") || error.contains("Minecraft");
        ("network", "retry", if later { 3 } else { 2 })
    } else if error.starts_with("No se pudo autenticar en Xbox Live") {
        ("xbox", "open_xbox", 3)
    } else if error.starts_with("Xbox no autorizó") {
        ("xsts", "open_xbox", 3)
    } else if error.starts_with("Minecraft rechazó") {
        ("minecraft", "retry", 3)
    } else if error.contains("no tiene un perfil de Minecraft") {
        ("no_java", "open_minecraft", 4)
    } else if error.starts_with("La configuración de acceso Microsoft") {
        ("client", "none", 2)
    } else {
        ("unknown", "retry", 2)
    }
}

/// Comprueba una cuenta guardada: archivo, acceso de OpenClient y una renovación completa
/// (Microsoft, Xbox, Minecraft y perfil de Java). Renueva la sesión como lo haría Jugar.
pub async fn diagnose(root: &Path, account_id: &str) -> Result<Diagnosis, String> {
    let path = account_path(root, account_id)?;
    let broken = |code: &'static str, detail: &str| Diagnosis {
        code,
        action: "reconnect",
        detail: detail.to_owned(),
        steps: steps_with(Some(0)),
        account: None,
    };
    let saved = match session_store::load(&path) {
        Ok(Some(saved)) if saved.account.id == account_id => saved,
        Ok(None) => return Ok(broken("missing", "No hay sesión guardada para esta cuenta.")),
        _ => {
            return Ok(broken(
                "broken",
                "La sesión guardada no se puede leer. Vuelve a conectar la cuenta.",
            ))
        }
    };
    if saved.oauth_client_id != active_client_id() {
        return Ok(Diagnosis {
            code: "app",
            action: "reconnect",
            detail: "OpenClient actualizó su acceso a Microsoft. Conecta la cuenta otra vez.".into(),
            steps: steps_with(Some(1)),
            account: None,
        });
    }
    Ok(match ensure_microsoft_session(&path, account_id, true).await {
        Ok(account) => Diagnosis {
            code: "ok",
            action: "none",
            detail: format!("La cuenta de {} funciona y la sesión se renovó.", account.username),
            steps: steps_with(None),
            account: Some(account_for_interface(account)),
        },
        Err(error) => {
            let (code, action, failed) = classify(&error);
            Diagnosis {
                code,
                action,
                detail: strip_prefix(&error),
                steps: steps_with(Some(failed)),
                account: None,
            }
        }
    })
}

pub fn begin_attempt() -> u64 {
    AUTH_ATTEMPT.fetch_add(1, Ordering::SeqCst).wrapping_add(1)
}

pub fn cancel_attempt() {
    begin_attempt();
}

pub fn saved_profile(path: &Path) -> Result<Option<MinecraftAccount>, String> {
    let cid = active_client_id();
    Ok(session_store::load(path)?
        .filter(|session| session.oauth_client_id == cid)
        .map(|session| account_for_interface(session.account)))
}

pub async fn forget_session(path: &Path) -> Result<(), String> {
    let _guard = SESSION_LOCK.lock().await;
    session_store::clear(path)
}

pub async fn refresh_profile(path: &Path) -> Result<MinecraftAccount, String> {
    let cid = active_client_id();
    let saved = session_store::load(path)?
        .filter(|s| s.oauth_client_id == cid)
        .ok_or("No hay una sesión de Microsoft activa.")?;
    let mut account = ensure_microsoft_session(path, &saved.account.id, false).await?;
    let client = auth_client()?;
    if let Ok(profile_raw) = http_get_bearer(
        &client,
        "https://api.minecraftservices.com/minecraft/profile",
        &account.access_token,
    )
    .await
    {
        if let Ok(profile) = serde_json::from_str::<MinecraftProfileResponse>(&profile_raw) {
            let active_skin = profile
                .skins
                .iter()
                .find(|s| s.state.eq_ignore_ascii_case("ACTIVE"))
                .or_else(|| profile.skins.first());
            account.skin_url = active_skin
                .map(|s| s.url.replace("http://", "https://"))
                .filter(|u| !u.is_empty())
                .unwrap_or_else(|| format!("https://mc-heads.net/skin/{}", profile.name));
            account.skin_variant = if active_skin
                .map(|s| s.variant.eq_ignore_ascii_case("SLIM"))
                .unwrap_or(false)
            {
                "slim".to_string()
            } else {
                "classic".to_string()
            };
            let mut cleaned_capes: Vec<MojangCape> = profile
                .capes
                .into_iter()
                .map(|mut c| {
                    c.url = c.url.replace("http://", "https://");
                    c
                })
                .collect();
            cleaned_capes.sort_by_key(|c| !c.state.eq_ignore_ascii_case("ACTIVE"));
            account.cape_id = cleaned_capes
                .iter()
                .find(|c| c.state.eq_ignore_ascii_case("ACTIVE"))
                .or_else(|| cleaned_capes.first())
                .map(|c| c.id.clone());
            account.capes = cleaned_capes;
            let _guard = SESSION_LOCK.lock().await;
            if let Ok(Some(mut current)) = session_store::load(path) {
                current.account = account.clone();
                let _ = session_store::save(path, &current);
            }
        }
    }
    Ok(account_for_interface(account))
}

pub async fn equip_cape(path: &Path, cape_id: Option<&str>) -> Result<MinecraftAccount, String> {
    let cid = active_client_id();
    let saved = session_store::load(path)?
        .filter(|s| s.oauth_client_id == cid)
        .ok_or("Inicia sesión con Microsoft para cambiar tu capa oficial.")?;
    let account = ensure_microsoft_session(path, &saved.account.id, false).await?;
    let client = auth_client()?;
    let url = "https://api.minecraftservices.com/minecraft/profile/capes/active";
    let response = match cape_id.filter(|id| !id.trim().is_empty()) {
        Some(id) => client
            .put(url)
            .bearer_auth(&account.access_token)
            .header("Content-Type", "application/json")
            .body(serde_json::json!({ "capeId": id.trim() }).to_string())
            .send()
            .await,
        None => client.delete(url).bearer_auth(&account.access_token).send().await,
    }
    .map_err(|_| "No se pudo conectar con Minecraft Services para cambiar la capa.")?;
    if !response.status().is_success() {
        return Err("Minecraft rechazó el cambio de capa para esta cuenta.".into());
    }
    refresh_profile(path).await
}

fn base64url(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::with_capacity((bytes.len() * 4).div_ceil(3));
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(TABLE[((triple >> 18) & 0x3F) as usize] as char);
        out.push(TABLE[((triple >> 12) & 0x3F) as usize] as char);
        if chunk.len() > 1 {
            out.push(TABLE[((triple >> 6) & 0x3F) as usize] as char);
        }
        if chunk.len() > 2 {
            out.push(TABLE[(triple & 0x3F) as usize] as char);
        }
    }
    out
}

pub async fn login_with_mode(
    root: &Path,
    attempt: u64,
    loopback: bool,
    show_prompt: impl FnOnce(&DevicePrompt) -> Result<(), String>,
) -> Result<MinecraftAccount, String> {
    let cancelled = async {
        while AUTH_ATTEMPT.load(Ordering::SeqCst) == attempt {
            tokio::time::sleep(Duration::from_millis(150)).await;
        }
    };
    tokio::select! {
        biased;
        _ = cancelled => Err("Inicio de sesión cancelado.".into()),
        result = async {
            if loopback {
                login_loopback(root, attempt, show_prompt).await
            } else {
                login_inner(root, attempt, show_prompt).await
            }
        } => result,
    }
}

pub async fn login(
    root: &Path,
    attempt: u64,
    show_prompt: impl FnOnce(&DevicePrompt) -> Result<(), String>,
) -> Result<MinecraftAccount, String> {
    login_with_mode(root, attempt, false, show_prompt).await
}

async fn login_loopback(
    root: &Path,
    attempt: u64,
    show_prompt: impl FnOnce(&DevicePrompt) -> Result<(), String>,
) -> Result<MinecraftAccount, String> {
    use sha2::{Digest, Sha256};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    if AUTH_ATTEMPT.load(Ordering::SeqCst) != attempt {
        return Err("Inicio de sesión cancelado.".into());
    }
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|_| "No se pudo abrir el puerto local para conectar con Microsoft.")?;
    let port = listener
        .local_addr()
        .map_err(|_| "Puerto local inválido.")?
        .port();
    let redirect_uri = format!("http://localhost:{}", port);
    let verifier = format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    );
    let challenge = base64url(&Sha256::digest(verifier.as_bytes()));
    let state_token = uuid::Uuid::new_v4().simple().to_string();
    let cid = active_client_id();
    let authorize_url = format!(
        "https://login.microsoftonline.com/consumers/oauth2/v2.0/authorize?client_id={}&response_type=code&redirect_uri={}&response_mode=query&scope={}&state={}&code_challenge={}&code_challenge_method=S256&prompt=select_account",
        url_encode(&cid),
        url_encode(&redirect_uri),
        url_encode(MINECRAFT_SCOPE),
        url_encode(&state_token),
        url_encode(&challenge)
    );
    show_prompt(&DevicePrompt {
        user_code: String::new(),
        verification_uri: authorize_url,
        expires_in: 300,
    })?;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(300);
    let (mut stream, code, returned_state, oauth_err) = loop {
        let (mut stream, _) = tokio::time::timeout_at(deadline, listener.accept())
            .await
            .map_err(|_| "El tiempo de espera en el navegador caducó. Inténtalo de nuevo.")?
            .map_err(|_| "No se recibió respuesta del navegador.")?;
        let mut buf = vec![0u8; 8192];
        let n = tokio::time::timeout(Duration::from_secs(5), stream.read(&mut buf))
            .await
            .ok()
            .and_then(|r| r.ok())
            .unwrap_or(0);
        let req = String::from_utf8_lossy(&buf[..n]);
        let path_query = req
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .unwrap_or("/");
        let Ok(parsed_url) = reqwest::Url::parse(&format!("http://localhost{}", path_query)) else {
            continue;
        };
        let (mut code, mut returned_state, mut oauth_err) = (None, None, None);
        for (k, v) in parsed_url.query_pairs() {
            match k.as_ref() {
                "code" => code = Some(v.into_owned()),
                "state" => returned_state = Some(v.into_owned()),
                "error" => oauth_err = Some(v.into_owned()),
                _ => {}
            }
        }
        if code.is_none() && oauth_err.is_none() {
            let _ = stream
                .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                .await;
            continue;
        }
        break (stream, code, returned_state, oauth_err);
    };
    let html = if oauth_err.is_none() && code.is_some() {
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nConnection: close\r\n\r\n<!DOCTYPE html><html><body style='background:#0c0a0e;color:#f5f2f8;font-family:Segoe UI,sans-serif;display:flex;align-items:center;justify-content:center;height:100vh;margin:0'><div style='text-align:center;padding:32px;border:1px solid #2a2433;border-radius:14px;background:#15121a'><h2>Cuenta Microsoft conectada</h2><p style='color:#9e96ab'>Ya puedes cerrar esta pestaña y volver a OpenClient.</p></div></body></html>"
    } else {
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nConnection: close\r\n\r\n<!DOCTYPE html><html><body style='background:#0c0a0e;color:#f5f2f8;font-family:Segoe UI,sans-serif;display:flex;align-items:center;justify-content:center;height:100vh;margin:0'><div style='text-align:center;padding:32px;border:1px solid #2a2433;border-radius:14px;background:#15121a'><h2>No se completó el acceso</h2><p style='color:#9e96ab'>Vuelve a OpenClient para intentarlo de nuevo.</p></div></body></html>"
    };
    let _ = stream.write_all(html.as_bytes()).await;
    let _ = stream.shutdown().await;
    if oauth_err.is_some() {
        return Err("Inicio de sesión cancelado o rechazado en el navegador.".into());
    }
    if returned_state.as_deref() != Some(state_token.as_str()) {
        return Err("La validación de seguridad (state) no coincidió. Inténtalo de nuevo.".into());
    }
    let auth_code = code.ok_or("Microsoft no devolvió un código de autorización.")?;
    let client = auth_client()?;
    let body = format!(
        "client_id={}&grant_type=authorization_code&code={}&redirect_uri={}&code_verifier={}&scope={}",
        url_encode(&cid),
        url_encode(&auth_code),
        url_encode(&redirect_uri),
        url_encode(&verifier),
        url_encode(MINECRAFT_SCOPE)
    );
    let raw = http_post_form(&client, TOKEN_URL, &body).await?;
    let tokens = parse_ms_token(&raw)?;
    finish_login(root, tokens, attempt)
        .await
        .map(account_for_interface)
}

async fn login_inner(
    root: &Path,
    attempt: u64,
    show_prompt: impl FnOnce(&DevicePrompt) -> Result<(), String>,
) -> Result<MinecraftAccount, String> {
    if AUTH_ATTEMPT.load(Ordering::SeqCst) != attempt {
        return Err("Inicio de sesión cancelado.".into());
    }
    let client = auth_client()?;
    let cid = active_client_id();
    let body = format!(
        "client_id={}&scope={}",
        cid,
        url_encode(MINECRAFT_SCOPE)
    );
    let raw = http_post_form(
        &client,
        "https://login.microsoftonline.com/consumers/oauth2/v2.0/devicecode",
        &body,
    )
    .await?;
    let authorization = parse_device_authorization(&raw)?;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(authorization.expires_in);
    if AUTH_ATTEMPT.load(Ordering::SeqCst) != attempt {
        return Err("Inicio de sesión cancelado.".into());
    }
    let prompt = DevicePrompt {
        user_code: authorization.user_code,
        verification_uri: authorization.verification_uri,
        expires_in: authorization.expires_in,
    };
    show_prompt(&prompt)?;
    let mut interval = authorization.interval.max(5);
    let form = format!(
        "grant_type=urn:ietf:params:oauth:grant-type:device_code&client_id={}&device_code={}",
        cid,
        url_encode(&authorization.device_code)
    );
    while tokio::time::Instant::now() < deadline {
        wait_device_interval(interval, attempt).await?;
        if tokio::time::Instant::now() >= deadline {
            break;
        }
        let raw = http_post_form(&client, TOKEN_URL, &form).await?;
        if AUTH_ATTEMPT.load(Ordering::SeqCst) != attempt {
            return Err("Inicio de sesión cancelado.".into());
        }
        match parse_device_poll(&raw)? {
            DevicePoll::Pending => {}
            DevicePoll::SlowDown => interval = interval.saturating_add(5),
            DevicePoll::Authorized(tokens) => {
                return finish_login(root, tokens, attempt)
                    .await
                    .map(account_for_interface)
            }
        }
    }
    Err("El código de Microsoft caducó. Vuelve a iniciar sesión.".into())
}

#[cfg(test)]
mod regression_tests {
    use super::*;

    #[test]
    fn device_prompt_accepts_only_official_pages_and_never_serializes_private_device_code() {
        let mut value = serde_json::json!({"device_code":"private-device-test-only","user_code":"ABC123XYZ","verification_uri":"https://www.microsoft.com/link","expires_in":900,"interval":5});
        let authorization = parse_device_authorization(&value.to_string()).unwrap();
        let prompt = DevicePrompt {
            user_code: authorization.user_code,
            verification_uri: authorization.verification_uri,
            expires_in: authorization.expires_in,
        };
        assert!(!serde_json::to_string(&prompt)
            .unwrap()
            .contains("private-device-test-only"));
        for address in [
            "http://www.microsoft.com/link",
            "https://www.microsoft.com.attacker.example/link",
            "https://attacker.example/",
            "https://www.microsoft.com/link?code=private",
            "https://user@www.microsoft.com/link",
        ] {
            value["verification_uri"] = address.into();
            assert!(parse_device_authorization(&value.to_string()).is_err());
        }
    }

    #[test]
    fn device_poll_distinguishes_waiting_denial_expiry_and_success_without_error_leaks() {
        assert!(matches!(
            parse_device_poll(r#"{"error":"authorization_pending"}"#).unwrap(),
            DevicePoll::Pending
        ));
        assert!(matches!(
            parse_device_poll(r#"{"error":"slow_down"}"#).unwrap(),
            DevicePoll::SlowDown
        ));
        assert!(matches!(
            parse_device_poll(r#"{"access_token":"test-access","refresh_token":"test-refresh"}"#)
                .unwrap(),
            DevicePoll::Authorized(_)
        ));
        for code in [
            "authorization_declined",
            "expired_token",
            "unauthorized_client",
            "unexpected_error",
        ] {
            let raw =
                serde_json::json!({"error":code,"error_description":"test-secret-must-not-appear"})
                    .to_string();
            let error = match parse_device_poll(&raw) {
                Err(error) => error,
                Ok(_) => panic!("Expected failure"),
            };
            assert!(!error.contains("test-secret-must-not-appear"));
        }
    }

    #[tokio::test]
    async fn cancelled_device_attempt_does_not_continue_polling() {
        let inactive = AUTH_ATTEMPT.load(Ordering::SeqCst).wrapping_add(1);
        assert!(wait_device_interval(900, inactive).await.is_err());
    }

    #[tokio::test]
    async fn authentication_transport_rejects_plain_http() {
        let result = auth_client()
            .unwrap()
            .post("http://127.0.0.1:1/")
            .send()
            .await;
        assert!(result.unwrap_err().is_builder());
    }

    #[test]
    fn interface_profile_does_not_expose_game_credentials() {
        let mut private = crate::identity::login_offline("Jugador_1".into()).unwrap();
        private.access_token = "TEST ONLY: private game token".into();
        private.account_type = "microsoft".into();
        let public = account_for_interface(private.clone());
        assert_eq!(public.username, private.username);
        assert!(public.access_token.is_empty());
        assert!(!serde_json::to_string(&public)
            .unwrap()
            .contains("TEST ONLY: private game token"));
        assert_eq!(private.access_token, "TEST ONLY: private game token");
    }

    #[cfg(windows)]
    #[test]
    fn accounts_live_in_one_file_each_and_the_legacy_session_migrates() {
        let root = std::env::temp_dir().join(format!("openclient-accounts-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let session = |id: &str, name: &str, client: &str| {
            let mut account = crate::identity::login_offline(name.into()).unwrap();
            account.id = id.into();
            account.account_type = "microsoft".into();
            account.access_token = "dummy-token".into();
            SavedSession {
                oauth_client_id: client.into(),
                account,
                refresh_token: "dummy-refresh".into(),
                expires_at: session_store::now() + 900,
            }
        };
        assert!(list_accounts(&root).unwrap().is_empty());
        // Sesión única de versiones anteriores.
        session_store::save(&root.join("session.dpapi"), &session("ms-aaaa", "Zeta", MINECRAFT_CLIENT_ID)).unwrap();
        let listed = list_accounts(&root).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!((listed[0].id.as_str(), listed[0].status), ("ms-aaaa", "ok"));
        assert!(!root.join("session.dpapi").exists());
        assert!(account_path(&root, "ms-aaaa").unwrap().exists());
        assert_eq!(list_accounts(&root).unwrap().len(), 1, "idempotente");

        session_store::save(&account_path(&root, "ms-bbbb").unwrap(), &session("ms-bbbb", "alfa", "otro-cliente")).unwrap();
        std::fs::write(session_store::sessions_dir(&root).join("ms-cccc.dpapi"), b"basura").unwrap();
        // Un archivo con el id de otra cuenta no vale.
        session_store::save(&account_path(&root, "ms-dddd").unwrap(), &session("ms-eeee", "Mala", MINECRAFT_CLIENT_ID)).unwrap();
        std::fs::write(session_store::sessions_dir(&root).join("otra-cosa.dpapi"), b"x").unwrap();
        let listed = list_accounts(&root).unwrap();
        let summary: Vec<(&str, &str)> = listed.iter().map(|entry| (entry.id.as_str(), entry.status)).collect();
        assert_eq!(
            summary,
            [("ms-bbbb", "reauth"), ("ms-aaaa", "ok"), ("ms-cccc", "broken"), ("ms-dddd", "broken")]
        );
        assert!(listed.iter().filter_map(|entry| entry.account.as_ref()).all(|a| a.access_token.is_empty()));
        assert_eq!(listed[0].account.as_ref().unwrap().username, "alfa");

        for bad in ["", "ms-", "../ms-aaaa", "ms-aa/bb", "ms-zz", "aaaa", &format!("ms-{}", "a".repeat(41))] {
            assert!(account_path(&root, bad).is_err(), "{bad}");
        }
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn diagnosis_classifies_every_known_failure() {
        let cases = [
            ("SESSION_REAUTH: Microsoft requiere que vuelvas a iniciar sesión.", "reauth", "reconnect", 2),
            ("SESSION_REAUTH: La sesión recibida pertenece a otra cuenta. Inicia sesión de nuevo.", "reauth", "reconnect", 2),
            ("SESSION_NETWORK: No se pudo conectar con Microsoft. Comprueba tu conexión e inténtalo otra vez.", "network", "retry", 2),
            ("SESSION_NETWORK: No se pudo conectar con Xbox/Minecraft. Inténtalo otra vez.", "network", "retry", 3),
            ("SESSION_NETWORK: No se pudo cargar el perfil de Minecraft.", "network", "retry", 3),
            ("No se pudo autenticar en Xbox Live. Revisa tu conexión y la configuración de tu cuenta Xbox.", "xbox", "open_xbox", 3),
            ("Xbox no autorizó esta cuenta para Minecraft. Revisa que tenga un perfil Xbox y los permisos familiares necesarios.", "xsts", "open_xbox", 3),
            ("Minecraft rechazó el acceso. Vuelve a intentarlo más tarde.", "minecraft", "retry", 3),
            ("Esta cuenta de Microsoft no tiene un perfil de Minecraft: Java Edition.", "no_java", "open_minecraft", 4),
            ("La configuración de acceso Microsoft fue rechazada por el proveedor.", "client", "none", 2),
            ("algo inesperado", "unknown", "retry", 2),
        ];
        for (message, code, action, step) in cases {
            assert_eq!(classify(message), (code, action, step), "{message}");
        }
        assert_eq!(strip_prefix("SESSION_NETWORK: Hola "), "Hola");
        let states = |failed| steps_with(failed).iter().map(|s| s.state).collect::<Vec<_>>();
        assert_eq!(states(None), ["ok"; 5]);
        assert_eq!(states(Some(2)), ["ok", "ok", "fail", "skip", "skip"]);
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn diagnosis_reports_local_problems_without_touching_the_network() {
        let root = std::env::temp_dir().join(format!("openclient-diag-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        assert!(diagnose(&root, "../x").await.is_err());
        let missing = diagnose(&root, "ms-aaaa").await.unwrap();
        assert_eq!((missing.code, missing.action), ("missing", "reconnect"));
        assert_eq!(missing.steps[0].state, "fail");
        assert!(missing.steps[1..].iter().all(|step| step.state == "skip"));

        let path = account_path(&root, "ms-bbbb").unwrap();
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"basura").unwrap();
        assert_eq!(diagnose(&root, "ms-bbbb").await.unwrap().code, "broken");

        let mut account = crate::identity::login_offline("Prueba".into()).unwrap();
        account.id = "ms-cccc".into();
        let session = SavedSession {
            oauth_client_id: "otro-cliente".into(),
            account,
            refresh_token: "dummy".into(),
            expires_at: session_store::now() + 900,
        };
        session_store::save(&account_path(&root, "ms-cccc").unwrap(), &session).unwrap();
        let old = diagnose(&root, "ms-cccc").await.unwrap();
        assert_eq!((old.code, old.steps[1].state), ("app", "fail"));
        assert!(!old.detail.contains("dummy"));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn session_expiry_reserves_time_for_launch() {
        assert!(session_is_fresh(1121, 1000));
        assert!(!session_is_fresh(1120, 1000));
        assert!(!session_is_fresh(999, 1000));
        assert!(!session_is_fresh(0, 1000));
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn saved_session_reopens_without_exposing_the_game_token() {
        let path =
            std::env::temp_dir().join(format!("openclient-session-{}.dpapi", uuid::Uuid::new_v4()));
        let mut account = crate::identity::login_offline("Prueba".into()).unwrap();
        account.id = "ms-test".into();
        account.account_type = "microsoft".into();
        account.access_token = "dummy-game-token".into();
        let session = SavedSession {
            oauth_client_id: MINECRAFT_CLIENT_ID.into(),
            account,
            refresh_token: "dummy-refresh".into(),
            expires_at: session_store::now() + 900,
        };
        session_store::save(&path, &session).unwrap();
        let restored = saved_profile(&path).unwrap().unwrap();
        let playable = ensure_microsoft_session(&path, &restored.id, false)
            .await
            .unwrap();
        forget_session(&path).await.unwrap();
        assert_eq!(restored.username, "Prueba");
        assert!(restored.access_token.is_empty());
        assert_eq!(playable.access_token, "dummy-game-token");
        assert!(saved_profile(&path).unwrap().is_none());
    }

    #[test]
    fn refresh_errors_distinguish_reauth_from_retry_without_exposing_response() {
        let reauth =
            parse_ms_token(r#"{"error":"invalid_grant","error_description":"private-details"}"#)
                .err()
                .unwrap();
        assert!(reauth.starts_with("SESSION_REAUTH:"));
        assert!(!reauth.contains("private-details"));
        assert!(parse_ms_token(r#"{"error":"temporarily_unavailable"}"#)
            .err()
            .unwrap()
            .starts_with("SESSION_NETWORK:"));
        let rotated =
            parse_ms_token(r#"{"access_token":"test-access","refresh_token":"test-rotation"}"#)
                .unwrap();
        assert_eq!(rotated.refresh_token.as_deref(), Some("test-rotation"));
        assert!(parse_ms_token(r#"{"access_token":"test-access"}"#)
            .unwrap()
            .refresh_token
            .is_none());
    }

    #[test]
    fn pkce_challenge_uses_unpadded_base64url() {
        assert_eq!(base64url(&[0xfb, 0xff]), "-_8");
        use sha2::{Digest, Sha256};
        // RFC 7636, apéndice B.
        let challenge = base64url(&Sha256::digest(b"dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"));
        assert_eq!(challenge, "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM");
    }

    #[test]
    fn offline_rejects_invalid_nickname() {
        assert!(crate::identity::login_offline("a".into()).is_err());
        assert!(crate::identity::login_offline("user/name".into()).is_err());
        assert!(crate::identity::login_offline("Jugador_1".into()).is_ok());
    }

    #[test]
    fn offline_uuid_matches_minecraft_name_uuid_without_namespace() {
        assert_eq!(
            crate::identity::generate_offline_uuid("Notch"),
            "b50ad385-829d-3141-a216-7e7d7539ba7f"
        );
    }
}
