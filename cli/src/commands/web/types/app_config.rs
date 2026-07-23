use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use tokio::sync::Mutex as AsyncMutex;

use super::super::schema_cache::SchemaCache;
use super::super::util::InFlightSet;

#[derive(Clone, Debug)]
pub struct AppConfig {
    pub nix_cmd: String,
    pub neo_input: String,
    pub settings_path: PathBuf,
    pub evaluator: Arc<AsyncMutex<super::super::nix::NixEvaluator>>,
    /// Shared with the persistent nix repl: true while an evaluation/refresh is in flight.
    pub eval_busy: Arc<AtomicBool>,
    /// Sender for broadcasting HTML OOB swap fragments over WS to htmx clients
    /// (unit controls + action-bar status + container pull progress).
    pub unit_updates: tokio::sync::broadcast::Sender<String>,
    /// Systemd unit names with an in-flight docker pull+restart (dedup + disable ↻ UI).
    pub pulls_in_flight: Arc<InFlightSet>,
    /// Service names with an in-flight clear-appdata operation (stop → rm → start if was running).
    pub clear_appdata_in_flight: Arc<InFlightSet>,
    /// Process-local option schema cache for helper resolution (avoids re-taking eval mutex).
    pub schema_cache: Arc<tokio::sync::RwLock<SchemaCache>>,
}
