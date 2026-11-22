//! Servicios compartidos por las interfaces de OpenClient.

pub mod cancel;
pub mod content_index;
pub mod content_meta;
pub mod crash_analysis;
mod directory_install;
pub mod foreign_import;
pub mod game_launcher;
pub mod hooks;
pub mod identity;
pub mod instance_storage;
pub mod jar_meta;
pub mod java_runtime;
pub mod library_state;
pub mod microsoft;
mod session_store;
pub use session_store::now as session_time;
pub mod modpack;
pub mod modpack_tx;
pub mod mrpack_export;
pub mod performance;
pub mod portable_instance;
pub mod storage;
pub mod versions;
pub mod world_backup;

pub use cancel::CancelToken;

use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

pub const DEFAULT_DOWNLOADS: usize = 16;
static DOWNLOADS: AtomicUsize = AtomicUsize::new(DEFAULT_DOWNLOADS);

pub fn set_download_concurrency(value: usize) {
    DOWNLOADS.store(value.clamp(1, 20), Ordering::Relaxed);
}

pub fn download_concurrency() -> usize {
    DOWNLOADS.load(Ordering::Relaxed)
}

type EventSink = dyn Fn(&str, serde_json::Value) -> Result<(), String> + Send + Sync;

#[derive(Clone)]
pub struct GameContext {
    pub root: PathBuf,
    pub launcher_version: &'static str,
    sink: Arc<EventSink>,
    pub cancel: CancelToken,
}

impl GameContext {
    pub fn new(
        root: PathBuf,
        launcher_version: &'static str,
        sink: impl Fn(&str, serde_json::Value) -> Result<(), String> + Send + Sync + 'static,
    ) -> Self {
        Self {
            root,
            launcher_version,
            sink: Arc::new(sink),
            cancel: CancelToken::default(),
        }
    }

    /// Sustituye el token de cancelación por el de la operación en curso.
    pub fn with_cancel(mut self, token: CancelToken) -> Self {
        self.cancel = token;
        self
    }

    pub(crate) fn emit(&self, event: &str, payload: impl serde::Serialize) -> Result<(), String> {
        (self.sink)(
            event,
            serde_json::to_value(payload).map_err(|error| error.to_string())?,
        )
    }

    /// Notifica el avance de una operación (`operation-progress`).
    pub fn progress(&self, phase: &str, percent: u8, message: &str) {
        let _ = self.emit(
            "operation-progress",
            serde_json::json!({ "stage": phase, "percent": percent, "message": message }),
        );
    }
}
