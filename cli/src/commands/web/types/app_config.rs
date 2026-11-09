use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use tokio::sync::Mutex as AsyncMutex;

use super::super::resources::ResourceSampler;
use super::super::schema_cache::SchemaCache;
use super::super::units::UnitStatusCache;
use super::super::util::InFlightSet;
use crate::utils::secrets_store::Store;

#[derive(Clone, Debug)]
pub struct AppConfig {
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
    /// Few-second, single-flight cache of batched `systemctl show` results for the
    /// services-page status dots (one systemd query per refresh for the whole page).
    pub unit_status: Arc<UnitStatusCache>,
    /// CPU/mem/disk/net/process sampler backing the resources panel; owns the
    /// previous `/proc` reading needed for percent/rate deltas.
    pub resources: Arc<ResourceSampler>,
}

impl AppConfig {
    /// The config repo: the folder of settings.toml.
    pub fn config_dir(&self) -> PathBuf {
        self.settings_path
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."))
    }

    pub fn secrets_store(&self) -> Store {
        Store::new(self.config_dir())
    }
}
