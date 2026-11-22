//! Operación de contenido activa (una sola a la vez): estado, progreso y cancelación.
//!
//! La DLL permanece cargada entre llamadas, así que los `static` sobreviven como los de `game`.
//! Qt consulta `operation_status` desde el hilo GUI mientras otro hilo espera en la llamada FFI.

use openclient_core::{CancelToken, GameContext};
use serde_json::{json, Value};
use std::{
    cell::RefCell,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex, MutexGuard,
    },
};

const BUSY: &str = "Ya hay una operación de contenido en curso.";
const MESSAGE_LIMIT: usize = 200;

static OPERATION: Mutex<Option<OpState>> = Mutex::new(None);
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

thread_local! {
    static CURRENT: RefCell<Option<GameContext>> = const { RefCell::new(None) };
}

struct OpState {
    id: u64,
    kind: String,
    phase: String,
    message: String,
    percent: Option<u8>,
    cancellable: bool,
    active: bool,
    error: Option<String>,
    token: CancelToken,
}

/// Asa de la operación activa. Si se suelta sin `finish`, la operación queda como `failed`.
pub struct OpGuard {
    id: u64,
}

impl OpGuard {
    pub fn id(&self) -> u64 {
        self.id
    }
}

impl Drop for OpGuard {
    fn drop(&mut self) {
        let mut slot = lock();
        if let Some(state) = slot.as_mut().filter(|s| s.id == self.id && s.active) {
            state.active = false;
            state.phase = "failed".into();
            state.cancellable = false;
            state.error = Some("La operación terminó de forma inesperada.".into());
        }
    }
}

fn lock() -> MutexGuard<'static, Option<OpState>> {
    OPERATION.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Las pruebas que tocan el estado global (directa o indirectamente vía `tracked`) se serializan.
#[cfg(test)]
pub(crate) fn test_lock() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn clean(message: &str) -> String {
    message
        .chars()
        .filter(|character| !character.is_control())
        .take(MESSAGE_LIMIT)
        .collect()
}

fn default_cancellable(kind: &str) -> bool {
    matches!(
        kind,
        "modpack_install"
            | "modpack_import"
            | "modpack_update"
            | "modpack_link"
            | "modpack_export"
            | "import_curseforge"
            | "import_mmc"
            | "content_add"
            | "content_sync"
            | "content_update"
            | "world_backup"
            | "world_restore"
    )
}

/// Fases a partir de las cuales ya no se puede cancelar (cambios locales en curso).
fn late_phase(phase: &str) -> bool {
    matches!(phase, "snapshot" | "applying" | "writing")
}

/// Registra una operación nueva; falla si ya hay otra activa.
pub fn begin(kind: &str) -> Result<(OpGuard, CancelToken), String> {
    let mut slot = lock();
    if slot.as_ref().is_some_and(|state| state.active) {
        return Err(BUSY.into());
    }
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    let token = CancelToken::default();
    *slot = Some(OpState {
        id,
        kind: kind.to_owned(),
        phase: "starting".into(),
        message: String::new(),
        percent: None,
        cancellable: default_cancellable(kind),
        active: true,
        error: None,
        token: token.clone(),
    });
    Ok((OpGuard { id }, token))
}

/// Actualiza fase, porcentaje y mensaje (ignorado si `id` ya no es la operación activa).
/// Para las fases que no pasan por el sink del motor.
#[allow(dead_code)]
pub fn update(id: u64, phase: &str, percent: Option<u8>, message: &str, cancellable: bool) {
    let mut slot = lock();
    if let Some(state) = slot.as_mut().filter(|s| s.id == id && s.active) {
        state.phase = clean(phase).chars().take(32).collect();
        state.percent = percent.map(|value| value.min(100));
        state.message = clean(message);
        state.cancellable = cancellable;
    }
}

/// Cierra la operación con `done`, `failed` o `cancelled` según el resultado.
pub fn finish(guard: OpGuard, result: &Result<Value, String>) {
    {
        let mut slot = lock();
        if let Some(state) = slot.as_mut().filter(|s| s.id == guard.id) {
            state.active = false;
            state.cancellable = false;
            match result {
                Ok(_) => {
                    state.phase = "done".into();
                    state.percent = Some(100);
                    state.message = "Completado.".into();
                }
                Err(error) if error.starts_with("CANCELLED:") => {
                    state.phase = "cancelled".into();
                    state.message = "Operación cancelada.".into();
                }
                Err(error) => {
                    state.phase = "failed".into();
                    state.message = clean(error);
                    state.error = Some(clean(error));
                }
            }
        }
    }
    drop(guard);
}

/// Traduce `launch-progress` / `operation-progress` (`{stage, percent, message}`).
fn translate(data: &Value) -> (String, Option<u8>, String) {
    let phase = data["stage"].as_str().unwrap_or("downloading").to_owned();
    let percent = data["percent"].as_u64().map(|value| value.min(100) as u8);
    let message = data["message"].as_str().unwrap_or_default().to_owned();
    (phase, percent, message)
}

fn apply_event(id: u64, data: &Value) {
    let (phase, percent, message) = translate(data);
    let mut slot = lock();
    if let Some(state) = slot.as_mut().filter(|s| s.id == id && s.active) {
        state.phase = clean(&phase).chars().take(32).collect();
        state.percent = percent;
        state.message = clean(&message);
        state.cancellable = state.cancellable && !late_phase(&phase);
    }
}

/// Contexto del motor cuyo progreso alimenta la operación `id` y cuya cancelación usa `token`.
pub fn context(root: &Path, id: u64, token: CancelToken) -> GameContext {
    GameContext::new(root.to_owned(), "0.1.0-preview.25", move |event, data| {
        if matches!(event, "launch-progress" | "operation-progress") {
            apply_event(id, &data);
        }
        Ok(())
    })
    .with_cancel(token)
}

/// Contexto sin operación asociada (consultas e inspecciones que no se registran).
pub fn detached_context(request: &Value) -> Result<GameContext, String> {
    let root = root_of(request)?;
    Ok(GameContext::new(root, "0.1.0-preview.25", |_, _| Ok(())))
}

pub fn root_of(request: &Value) -> Result<PathBuf, String> {
    let root = Path::new(
        request["root"]
            .as_str()
            .ok_or("Falta la carpeta de datos.")?,
    );
    if !root.is_absolute() || !root.is_dir() {
        return Err("Carpeta de datos inválida.".into());
    }
    Ok(root.to_owned())
}

struct CurrentGuard;

impl CurrentGuard {
    fn set(context: GameContext) -> Self {
        CURRENT.with(|slot| *slot.borrow_mut() = Some(context));
        Self
    }
}

impl Drop for CurrentGuard {
    fn drop(&mut self) {
        CURRENT.with(|slot| *slot.borrow_mut() = None);
    }
}

/// Contexto de la operación registrada que se ejecuta en este hilo, si la hay.
/// Permite que funciones que solo reciben la petición (`catalog`) usen progreso y cancelación.
pub fn current() -> Option<GameContext> {
    CURRENT.with(|slot| slot.borrow().clone())
}

/// Ejecuta `f` como operación registrada: rechaza una segunda operación, publica el progreso,
/// y libera la operación aunque `f` falle o haga panic.
pub fn tracked(
    kind: &str,
    request: &Value,
    f: impl FnOnce(&Value, &GameContext) -> Result<Value, String>,
) -> Result<Value, String> {
    #[cfg(test)]
    let _serial = test_lock();
    let root = root_of(request)?;
    let (guard, token) = begin(kind)?;
    let context = context(&root, guard.id(), token);
    let _current = CurrentGuard::set(context.clone());
    let result = f(request, &context);
    finish(guard, &result);
    result
}

fn describe(state: &Option<OpState>) -> Value {
    match state {
        None => json!({"active": false}),
        Some(state) => json!({
            "active": state.active, "id": state.id, "kind": state.kind, "phase": state.phase,
            "message": state.message, "percent": state.percent,
            "cancellable": state.cancellable && state.active, "error": state.error
        }),
    }
}

pub fn status() -> Value {
    describe(&lock())
}

/// `{"target":"content"|"game","id":<u64 opcional>}` → `{"cancelled":bool}`. No toca disco.
pub fn cancel(request: &Value) -> Result<Value, String> {
    let cancelled = match request["target"].as_str() {
        Some("content") => {
            let wanted = match &request["id"] {
                Value::Null => None,
                value => Some(value.as_u64().ok_or("Identificador de operación inválido.")?),
            };
            let mut slot = lock();
            match slot.as_mut().filter(|state| {
                state.active && state.cancellable && wanted.is_none_or(|id| id == state.id)
            }) {
                Some(state) => {
                    state.token.cancel();
                    state.message = "Cancelando…".into();
                    true
                }
                None => false,
            }
        }
        Some("game") => crate::game::cancel_launch(),
        _ => return Err("Destino de cancelación inválido.".into()),
    };
    Ok(json!({"cancelled": cancelled}))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn idle() {
        *lock() = None;
    }

    #[test]
    fn second_begin_is_rejected_and_drop_without_finish_marks_failed() {
        let _serial = test_lock();
        idle();
        let (first, _) = begin("content_add").unwrap();
        assert_eq!(begin("content_sync").err().as_deref(), Some(BUSY));
        assert_eq!(status()["active"], true);
        assert_eq!(status()["kind"], "content_add");
        drop(first);
        let after = status();
        assert_eq!(after["active"], false);
        assert_eq!(after["phase"], "failed");
        assert!(begin("content_sync").is_ok());
        idle();
    }

    #[test]
    fn finish_reports_done_failed_and_cancelled() {
        let _serial = test_lock();
        idle();
        for (result, phase) in [
            (Ok(json!(null)), "done"),
            (Err("fallo".to_string()), "failed"),
            (Err("CANCELLED: Operación cancelada.".to_string()), "cancelled"),
        ] {
            let (guard, _) = begin("modpack_update").unwrap();
            finish(guard, &result);
            let status = status();
            assert_eq!(status["active"], false);
            assert_eq!(status["phase"], phase);
            assert_eq!(status["error"].is_string(), phase == "failed");
        }
        idle();
    }

    #[test]
    fn describe_without_state_is_inactive() {
        assert_eq!(describe(&None), json!({"active": false}));
    }

    #[test]
    fn sink_translates_progress_and_clamps_percent() {
        let _serial = test_lock();
        idle();
        let (guard, token) = begin("modpack_update").unwrap();
        let root = std::env::temp_dir();
        let ctx = context(&root, guard.id(), token);
        ctx.progress("downloading", 255, "Descargando archivos (1/2)");
        let status = status();
        assert_eq!(status["phase"], "downloading");
        assert_eq!(status["percent"], 100);
        assert_eq!(status["message"], "Descargando archivos (1/2)");
        assert_eq!(status["cancellable"], true);
        assert_eq!(translate(&json!({"stage": "java", "percent": 900, "message": "x"})).1, Some(100));
        // Pasada la fase `applying` ya no se puede cancelar.
        ctx.progress("applying", 50, "Aplicando");
        assert_eq!(super::status()["cancellable"], false);
        ctx.progress("downloading", 10, "x");
        assert_eq!(super::status()["cancellable"], false);
        // Mensajes largos o con controles se recortan.
        ctx.progress("verifying", 1, &format!("a\n{}", "b".repeat(500)));
        let message = super::status()["message"].as_str().unwrap().to_owned();
        assert_eq!(message.chars().count(), MESSAGE_LIMIT);
        assert!(!message.contains('\n'));
        drop(guard);
        idle();
    }

    #[test]
    fn cancel_hits_only_the_matching_cancellable_operation() {
        let _serial = test_lock();
        idle();
        assert_eq!(cancel(&json!({"target": "content"})).unwrap(), json!({"cancelled": false}));
        let (guard, token) = begin("modpack_update").unwrap();
        let id = guard.id();
        assert_eq!(cancel(&json!({"target": "content", "id": id + 1})).unwrap()["cancelled"], false);
        assert!(!token.is_cancelled());
        assert_eq!(cancel(&json!({"target": "content", "id": id})).unwrap()["cancelled"], true);
        assert!(token.is_cancelled());
        assert!(cancel(&json!({"target": "otro"})).is_err());
        assert!(cancel(&json!({})).is_err());
        drop(guard);
        let (copy, token) = begin("duplicate").unwrap();
        assert_eq!(cancel(&json!({"target": "content"})).unwrap()["cancelled"], false);
        assert!(!token.is_cancelled());
        drop(copy);
        idle();
    }

    /// Fase de la operación `id` si sigue siendo la última registrada (otra prueba puede haberla reiniciado).
    fn finished_phase(id: u64) -> Option<String> {
        let _serial = test_lock();
        lock().as_ref().filter(|state| state.id == id).map(|state| {
            assert!(!state.active, "la operación {id} sigue activa");
            state.phase.clone()
        })
    }

    #[test]
    fn tracked_releases_the_operation_on_error_and_on_panic() {
        let root = std::env::temp_dir();
        let request = json!({"root": root});
        let current_id = || status()["id"].as_u64().unwrap();
        let mut id = 0;
        let error = tracked("content_add", &request, |_, _| {
            id = current_id();
            Err("falla".to_string())
        });
        assert_eq!(error.unwrap_err(), "falla");
        assert!(finished_phase(id).is_none_or(|phase| phase == "failed"));
        let mut panicked_id = 0;
        let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = tracked("content_add", &request, |_, _| {
                panicked_id = current_id();
                panic!("boom")
            });
        }));
        assert!(panicked.is_err());
        assert!(finished_phase(panicked_id).is_none_or(|phase| phase == "failed"));
        let _serial = test_lock();
        assert!(current().is_none());
        idle();
        assert!(begin("content_sync").is_ok());
        idle();
    }

    #[test]
    fn tracked_validates_the_root_and_exposes_the_context_to_the_closure() {
        let missing = tracked("content_add", &json!({}), |_, _| Ok(json!(1)));
        assert_eq!(missing.unwrap_err(), "Falta la carpeta de datos.");
        let relative = tracked("content_add", &json!({"root": "relativa"}), |_, _| Ok(json!(1)));
        assert_eq!(relative.unwrap_err(), "Carpeta de datos inválida.");
        let request = json!({"root": std::env::temp_dir()});
        let mut id = 0;
        let seen = tracked("content_add", &request, |_, ctx| {
            assert!(current().is_some());
            id = status()["id"].as_u64().unwrap();
            ctx.progress("downloading", 5, "mitad");
            Ok(status()["percent"].clone())
        });
        assert_eq!(seen.unwrap(), json!(5));
        assert!(finished_phase(id).is_none_or(|phase| phase == "done"));
        let _serial = test_lock();
        idle();
    }
}
