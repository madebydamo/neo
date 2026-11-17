use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use rocket::response::content::RawHtml;

use super::super::nix::EvalPool;
use super::super::resources::ResourceSampler;
use super::super::units::UnitStatusCache;
use super::super::util::InFlightSet;
use crate::utils::secrets_store::Store;

#[derive(Clone)]
pub struct AppConfig {
    pub settings_path: PathBuf,
    /// Evaluations of the config repo and their cached results (nix/pool/).
    pub eval: Arc<EvalPool>,
    /// Pages wait for an evaluation (none yet, or a structural change): navbar spinner.
    pub eval_busy: Arc<AtomicBool>,
    /// A settings change is being evaluated in the background (pages are not blocked).
    pub eval_pending: Arc<AtomicBool>,
    /// Sender for broadcasting HTML OOB swap fragments over WS to htmx clients
    /// (unit controls + action-bar status + container pull progress).
    pub unit_updates: tokio::sync::broadcast::Sender<String>,
    /// Systemd unit names with an in-flight docker pull+restart (dedup + disable ↻ UI).
    pub pulls_in_flight: Arc<InFlightSet>,
    /// Service names with an in-flight clear-appdata operation (stop → rm → start if was running).
    pub clear_appdata_in_flight: Arc<InFlightSet>,
    /// Few-second, single-flight cache of batched `systemctl show` results for the
    /// services-page status dots (one systemd query per refresh for the whole page).
    pub unit_status: Arc<UnitStatusCache>,
    /// CPU/mem/disk/net/process sampler backing the resources panel; owns the
    /// previous `/proc` reading needed for percent/rate deltas.
    pub resources: Arc<ResourceSampler>,
}

impl AppConfig {
    /// A config for tests around `settings_path`, without evaluations.
    #[cfg(test)]
    pub fn for_tests(settings_path: PathBuf) -> Self {
        let dir = settings_path
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_default();
        Self {
            eval: EvalPool::for_tests(&dir),
            settings_path,
            eval_busy: Default::default(),
            eval_pending: Default::default(),
            unit_updates: tokio::sync::broadcast::channel(1).0,
            pulls_in_flight: Default::default(),
            clear_appdata_in_flight: Default::default(),
            unit_status: Default::default(),
            resources: Default::default(),
        }
    }

    /// The config repo: the folder of settings.toml.
    pub fn config_dir(&self) -> PathBuf {
        self.settings_path
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."))
    }

    /// Push an OOB fragment to every `/ws/status` client (none connected is fine).
    pub fn broadcast(&self, fragment: String) {
        let _ = self.unit_updates.send(fragment);
    }

    /// Push `fragments` and answer the request with the same HTML.
    pub fn broadcast_all(&self, fragments: &[&str]) -> RawHtml<String> {
        for f in fragments {
            self.broadcast(f.to_string());
        }
        RawHtml(fragments.concat())
    }

    pub fn secrets_store(&self) -> Store {
        Store::new(self.config_dir())
    }
}
