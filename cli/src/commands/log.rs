// Helpers for the state JSON and log files under /tmp/neo-activations (shared by
// activate/update and the web UI for progress monitoring of long-running ops).
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Result;

pub const OPERATIONS_DIR: &str = "/tmp/neo-activations";

pub fn operations_dir() -> PathBuf {
    PathBuf::from(OPERATIONS_DIR)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OperationKind {
    Activation,
    Update,
}

pub struct OperationLog {
    id: String,
    suffix: String,
    state_path: PathBuf,
    log_path: PathBuf,
}

impl OperationLog {
    pub fn new(kind: OperationKind, suffix: &str) -> Self {
        let prefix = match kind {
            OperationKind::Activation => "activation",
            OperationKind::Update => "update",
        };
        let id = format!("{}_{}", prefix, suffix);
        let dir = operations_dir();
        let _ = fs::create_dir_all(&dir);
        let state_path = dir.join(format!("{}.json", id));
        let log_path = dir.join(format!("{}.log", id));
        OperationLog {
            id,
            suffix: suffix.to_string(),
            state_path,
            log_path,
        }
    }

    pub fn new_activation(suffix: &str) -> Self {
        Self::new(OperationKind::Activation, suffix)
    }

    pub fn new_update(suffix: &str) -> Self {
        Self::new(OperationKind::Update, suffix)
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn suffix(&self) -> &str {
        &self.suffix
    }

    pub fn state_path(&self) -> &Path {
        &self.state_path
    }

    pub fn log_path(&self) -> &Path {
        &self.log_path
    }

    pub fn write_state(&self, status: &str, phase: &str, err: Option<&str>, branch: Option<&str>) {
        let mut s = serde_json::json!({
            "id": &self.id,
            "status": status,
            "phase": phase,
            "started_at": &self.suffix,
            "log_path": self.log_path.to_string_lossy(),
        });
        if let Some(e) = err {
            s["error"] = serde_json::json!(e);
        }
        if let Some(b) = branch {
            s["branch"] = serde_json::json!(b);
        }
        let _ = fs::write(
            &self.state_path,
            serde_json::to_string_pretty(&s).unwrap_or_else(|_| "{}".to_string()),
        );
    }

    /// Write an "in_progress" marker for the given phase (before running the step body).
    pub fn record_step(&self, phase: &str) {
        self.write_state("in_progress", phase, None, None);
    }

    /// Execute `f` as the body of `phase`: first records "in_progress"/phase, then on any
    /// error from f() records "failed"/phase (with the error string) before returning the Err.
    /// On success, returns the value (caller may record a follow-up "xxx-done" state if desired).
    pub fn step<T, F: FnOnce() -> Result<T>>(&self, phase: &str, f: F) -> Result<T> {
        self.record_step(phase);
        match f() {
            Ok(v) => Ok(v),
            Err(e) => {
                self.write_state("failed", phase, Some(&e.to_string()), None);
                Err(e)
            }
        }
    }

    /// Write the initial "triggered" state used by the web trigger path (before systemd-run
    /// hands off to the real `neo activate` / `neo update` which will overwrite with "starting").
    pub fn init_for_web_trigger(&self, ts: &str) {
        self.write_state("in_progress", "triggered", None, None);
        let _ = fs::write(
            &self.log_path,
            format!("{} triggered via web at {}\n", self.id, ts),
        );
    }
}

/// Resolve an explicit suffix (e.g. from CLI arg), falling back to the named env var,
/// then to a fresh timestamp. Used by both activate and update to get consistent IDs.
pub fn resolve_suffix(provided: Option<&str>, env_var: &str) -> String {
    provided
        .map(|s| s.to_string())
        .unwrap_or_else(|| std::env::var(env_var).unwrap_or_else(|_| super::get_timestamp()))
}
