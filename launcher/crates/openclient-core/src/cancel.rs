//! Cancelación cooperativa de operaciones largas (descargas, importaciones).

use std::{
    future::Future,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use tokio::sync::Notify;

/// Mensaje fijo de cancelación; la interfaz lo reconoce por el prefijo `CANCELLED:`.
pub const CANCELLED: &str = "CANCELLED: Operación cancelada.";

#[derive(Default)]
struct Inner {
    flag: AtomicBool,
    notify: Notify,
}

/// Señal de cancelación compartida entre clones.
#[derive(Clone, Default)]
pub struct CancelToken(Arc<Inner>);

impl CancelToken {
    pub fn cancel(&self) {
        self.0.flag.store(true, Ordering::SeqCst);
        self.0.notify.notify_waiters();
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.flag.load(Ordering::SeqCst)
    }

    /// Se completa cuando el token se cancela (inmediatamente si ya lo estaba).
    pub async fn cancelled(&self) {
        let notified = self.0.notify.notified();
        tokio::pin!(notified);
        // Registrarse antes de leer la bandera evita perder un `cancel` concurrente.
        notified.as_mut().enable();
        if self.is_cancelled() {
            return;
        }
        notified.await;
    }

    pub fn check(&self) -> Result<(), String> {
        if self.is_cancelled() {
            Err(CANCELLED.to_string())
        } else {
            Ok(())
        }
    }
}

/// Ejecuta `fut` y lo abandona con `Err(CANCELLED)` si el token se cancela antes.
pub async fn guard<T>(
    token: &CancelToken,
    fut: impl Future<Output = Result<T, String>>,
) -> Result<T, String> {
    token.check()?;
    tokio::select! {
        biased;
        _ = token.cancelled() => Err(CANCELLED.to_string()),
        result = fut => result,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn check_reflects_cancel_and_clones_share_state() {
        let token = CancelToken::default();
        let clone = token.clone();
        assert!(token.check().is_ok());
        clone.cancel();
        assert!(token.is_cancelled());
        assert_eq!(token.check().unwrap_err(), CANCELLED);
        assert!(CANCELLED.starts_with("CANCELLED:"));
    }

    #[tokio::test]
    async fn guard_returns_cancelled_quickly_on_pending_future() {
        let token = CancelToken::default();
        let remote = token.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(10)).await;
            remote.cancel();
        });
        let started = Instant::now();
        let result: Result<(), String> =
            guard(&token, std::future::pending::<Result<(), String>>()).await;
        assert_eq!(result.unwrap_err(), CANCELLED);
        assert!(started.elapsed() < Duration::from_millis(50));
    }

    #[tokio::test]
    async fn guard_reacts_within_50ms_after_cancel() {
        let token = CancelToken::default();
        let waiter = {
            let token = token.clone();
            tokio::spawn(async move {
                guard(&token, std::future::pending::<Result<(), String>>()).await
            })
        };
        tokio::time::sleep(Duration::from_millis(20)).await;
        let cancelled_at = Instant::now();
        token.cancel();
        let result = waiter.await.unwrap();
        assert_eq!(result.unwrap_err(), CANCELLED);
        assert!(cancelled_at.elapsed() < Duration::from_millis(50));
    }

    #[tokio::test]
    async fn guard_with_already_cancelled_token_does_not_poll_future() {
        let token = CancelToken::default();
        token.cancel();
        let result = guard(&token, async { Ok::<_, String>(1) }).await;
        assert_eq!(result.unwrap_err(), CANCELLED);
    }

    #[tokio::test]
    async fn guard_passes_through_result_when_not_cancelled() {
        let token = CancelToken::default();
        assert_eq!(guard(&token, async { Ok::<_, String>(7) }).await, Ok(7));
        let err = guard(&token, async { Err::<u8, _>("fallo".to_string()) }).await;
        assert_eq!(err.unwrap_err(), "fallo");
    }
}
