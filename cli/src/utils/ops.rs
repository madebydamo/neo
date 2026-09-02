//! On-disk operation state + log files under `/tmp/neo-activations`.
//!
//! Shared by CLI activate/update/generation and the web UI (progress monitors,
//! store repair, genswitch).

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::Result;

use super::command::get_timestamp;

pub const OPERATIONS_DIR: &str = "/tmp/neo-activations";

pub fn operations_dir() -> PathBuf {
    PathBuf::from(OPERATIONS_DIR)
}

pub fn state_path(id: &str) -> PathBuf {
    operations_dir().join(format!("{id}.json"))
}

pub fn log_path(id: &str) -> PathBuf {
    operations_dir().join(format!("{id}.log"))
}

/// Single writer for op JSON state used by both [`OperationLog`] and the web store.
pub fn write_op_state(
    id: &str,
    status: &str,
    phase: &str,
    err: Option<&str>,
    branch: Option<&str>,
    started_at: Option<&str>,
    extra: Option<serde_json::Value>,
) {
    let _ = fs::create_dir_all(operations_dir());
    let mut s = serde_json::json!({
        "id": id,
        "status": status,
        "phase": phase,
        "log_path": log_path(id).to_string_lossy(),
    });
    if let Some(ts) = started_at {
        s["started_at"] = serde_json::json!(ts);
    }
    if let Some(e) = err {
        s["error"] = serde_json::json!(e);
    }
    if let Some(b) = branch {
        s["branch"] = serde_json::json!(b);
    }
    if let Some(serde_json::Value::Object(map)) = extra {
        if let Some(obj) = s.as_object_mut() {
            for (k, v) in map {
                obj.insert(k, v);
            }
        }
    }
    let _ = fs::write(
        state_path(id),
        serde_json::to_string_pretty(&s).unwrap_or_else(|_| "{}".to_string()),
    );
}

pub fn append_log(path: &Path, line: &str) {
    if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(f, "{line}");
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OperationKind {
    Activation,
    Update,
    /// System generation switch/boot (web triggers via systemd-run).
    Generation,
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
            OperationKind::Generation => "genswitch",
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

    pub fn new_generation(suffix: &str) -> Self {
        Self::new(OperationKind::Generation, suffix)
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
        self.write_state_extra(status, phase, err, branch, None);
    }

    /// Like [`write_state`] with optional extra JSON fields (e.g. generation number).
    pub fn write_state_extra(
        &self,
        status: &str,
        phase: &str,
        err: Option<&str>,
        branch: Option<&str>,
        extra: Option<serde_json::Value>,
    ) {
        write_op_state(
            &self.id,
            status,
            phase,
            err,
            branch,
            Some(&self.suffix),
            extra,
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

    /// Mirror process stdout/stderr to this op's `.log` and the console.
    /// Keep the returned guard alive for the duration of the operation.
    pub fn capture_stdio(&self) -> Option<super::stdio_tee::StdioTee> {
        super::stdio_tee::tee_stdio_to_log(&self.log_path).ok()
    }
}

/// Resolve an explicit suffix (e.g. from CLI arg), falling back to the named env var,
/// then to a fresh timestamp. Used by both activate and update to get consistent IDs.
pub fn resolve_suffix(provided: Option<&str>, env_var: &str) -> String {
    provided
        .map(|s| s.to_string())
        .unwrap_or_else(|| std::env::var(env_var).unwrap_or_else(|_| get_timestamp()))
}
