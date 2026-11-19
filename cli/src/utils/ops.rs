//! On-disk operation state + log files under `/tmp/neo-activations`.
//!
//! Shared by CLI activate/update/generation and the web UI (progress monitors,
//! store repair, genswitch).

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::Serialize;

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

/// `status` of an op's JSON state (the web UI reads the same strings).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OpStatus {
    InProgress,
    Success,
    Failed,
}

impl OpStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            OpStatus::InProgress => "in_progress",
            OpStatus::Success => "success",
            OpStatus::Failed => "failed",
        }
    }
}

/// One op's JSON state file, as the web UI polls it.
#[derive(Debug, Serialize)]
pub struct OpState<'a> {
    pub id: &'a str,
    pub status: OpStatus,
    pub phase: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<&'a str>,
    /// More fields (mode, generation number); they may override the ones above.
    #[serde(skip)]
    pub extra: Option<serde_json::Value>,
}

impl<'a> OpState<'a> {
    pub fn new(id: &'a str, status: OpStatus, phase: &'a str) -> Self {
        OpState {
            id,
            status,
            phase,
            error: None,
            branch: None,
            started_at: None,
            extra: None,
        }
    }

    /// The JSON object written to the state file (`log_path` included).
    fn to_json(&self) -> serde_json::Value {
        let mut s = serde_json::to_value(self).unwrap_or_else(|_| serde_json::json!({}));
        if let Some(obj) = s.as_object_mut() {
            obj.insert(
                "log_path".into(),
                log_path(self.id).to_string_lossy().into(),
            );
            if let Some(serde_json::Value::Object(map)) = &self.extra {
                obj.extend(map.clone());
            }
        }
        s
    }

    /// Replace the op's state file. The web UI polls it: it is replaced in
    /// one step, never truncated; a failed write is reported, not fatal.
    pub fn write(&self) {
        let _ = fs::create_dir_all(operations_dir());
        let written = serde_json::to_vec_pretty(&self.to_json())
            .map_err(anyhow::Error::from)
            .and_then(|body| super::fsutil::write_atomic(&state_path(self.id), &body, None));
        if let Err(e) = written {
            eprintln!("neo: could not record state of {}: {e:#}", self.id);
        }
    }
}

pub fn append_log(path: &Path, line: &str) {
    if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(f, "{line}");
    }
}

/// Background operation kind. Op ids are `<prefix>_<suffix>`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OperationKind {
    Activation,
    Update,
    /// System generation switch/boot (web triggers via systemd-run).
    Generation,
    /// Nix store verify/repair (web only).
    Repair,
    /// Restore of a backup-store client onto a new machine (neo backup-store rescue).
    Rescue,
}

impl OperationKind {
    const ALL: [Self; 5] = [
        Self::Activation,
        Self::Update,
        Self::Generation,
        Self::Repair,
        Self::Rescue,
    ];

    pub fn prefix(self) -> &'static str {
        match self {
            Self::Activation => "activation",
            Self::Update => "update",
            Self::Generation => "genswitch",
            Self::Repair => "repair",
            Self::Rescue => "rescue",
        }
    }

    /// Kind from an op id prefix (`activation_…`, `update_…`, `genswitch_…`, `repair_…`).
    pub fn from_id(id: &str) -> Option<Self> {
        let (prefix, _) = id.split_once('_')?;
        Self::ALL.into_iter().find(|k| k.prefix() == prefix)
    }
}

pub struct OperationLog {
    id: String,
    suffix: String,
    /// JSON fields carried by every state this op writes.
    extra: Option<serde_json::Value>,
}

impl OperationLog {
    pub fn new(kind: OperationKind, suffix: &str) -> Self {
        let _ = fs::create_dir_all(operations_dir());
        OperationLog {
            id: format!("{}_{}", kind.prefix(), suffix),
            suffix: suffix.to_string(),
            extra: None,
        }
    }

    /// Fields to keep in every state this op writes (mode, generation number).
    pub fn with_extra(mut self, extra: serde_json::Value) -> Self {
        self.extra = Some(extra);
        self
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn suffix(&self) -> &str {
        &self.suffix
    }

    pub fn write_state(
        &self,
        status: OpStatus,
        phase: &str,
        err: Option<&str>,
        branch: Option<&str>,
    ) {
        self.write_state_extra(status, phase, err, branch, None);
    }

    /// Like [`write_state`] with optional extra JSON fields (e.g. generation number).
    pub fn write_state_extra(
        &self,
        status: OpStatus,
        phase: &str,
        err: Option<&str>,
        branch: Option<&str>,
        extra: Option<serde_json::Value>,
    ) {
        let extra = match (self.extra.clone(), extra) {
            (Some(serde_json::Value::Object(mut base)), Some(serde_json::Value::Object(more))) => {
                base.extend(more);
                Some(serde_json::Value::Object(base))
            }
            (base, more) => more.or(base),
        };
        OpState {
            error: err,
            branch,
            started_at: Some(&self.suffix),
            extra,
            ..OpState::new(&self.id, status, phase)
        }
        .write();
    }

    /// Write an "in_progress" marker for the given phase (before running the step body).
    fn record_step(&self, phase: &str) {
        self.write_state(OpStatus::InProgress, phase, None, None);
    }

    /// Execute `f` as the body of `phase`: first records "in_progress"/phase, then on any
    /// error from f() records "failed"/phase (with the error string) before returning the Err.
    /// On success, returns the value (caller may record a follow-up "xxx-done" state if desired).
    pub fn step<T, F: FnOnce() -> Result<T>>(&self, phase: &str, f: F) -> Result<T> {
        self.record_step(phase);
        match f() {
            Ok(v) => Ok(v),
            Err(e) => {
                self.write_state(OpStatus::Failed, phase, Some(&format!("{e:#}")), None);
                Err(e)
            }
        }
    }

    /// Write the initial "triggered" state used by the web trigger path (before systemd-run
    /// hands off to the real `neo activate` / `neo update` which will overwrite with "starting").
    pub fn init_for_web_trigger(&self, ts: &str) {
        self.write_state(OpStatus::InProgress, "triggered", None, None);
        let _ = fs::write(
            log_path(&self.id),
            format!("{} triggered via web at {}\n", self.id, ts),
        );
    }

    /// Mirror process stdout/stderr to this op's `.log` and the console.
    /// Keep the returned guard alive for the duration of the operation.
    pub fn capture_stdio(&self) -> Option<super::stdio_tee::StdioTee> {
        super::stdio_tee::tee_stdio_to_log(&log_path(&self.id)).ok()
    }
}

/// Resolve an explicit suffix (e.g. from CLI arg), falling back to the named env var,
/// then to a fresh timestamp. Used by both activate and update to get consistent IDs.
pub fn resolve_suffix(provided: Option<&str>, env_var: &str) -> String {
    provided
        .map(|s| s.to_string())
        .unwrap_or_else(|| std::env::var(env_var).unwrap_or_else(|_| get_timestamp()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn op_state_json_has_the_fields_the_web_reads() {
        let state = OpState {
            branch: Some("activation_x"),
            started_at: Some("20260101-000000"),
            extra: Some(serde_json::json!({ "generation": 7, "phase": "override" })),
            ..OpState::new("activation_x", OpStatus::InProgress, "build")
        };
        let v = state.to_json();
        assert_eq!(v["id"], "activation_x");
        assert_eq!(v["status"], "in_progress");
        assert_eq!(v["branch"], "activation_x");
        assert_eq!(v["started_at"], "20260101-000000");
        assert_eq!(v["generation"], 7);
        // Extra fields win, like the old merge into the JSON object.
        assert_eq!(v["phase"], "override");
        assert!(v["log_path"]
            .as_str()
            .unwrap()
            .ends_with("activation_x.log"));
        assert!(v.get("error").is_none());
        for (s, txt) in [
            (OpStatus::InProgress, "in_progress"),
            (OpStatus::Success, "success"),
            (OpStatus::Failed, "failed"),
        ] {
            assert_eq!(serde_json::to_value(s).unwrap(), txt);
            assert_eq!(s.as_str(), txt);
        }
    }

    #[test]
    fn op_ids_round_trip_to_their_kind() {
        for kind in OperationKind::ALL {
            let id = format!("{}_{}", kind.prefix(), "20260101-000000");
            assert_eq!(OperationKind::from_id(&id), Some(kind));
        }
        assert_eq!(OperationKind::from_id("bogus_1"), None);
        assert_eq!(OperationKind::from_id("activation"), None);
    }

    #[test]
    fn suffix_prefers_argument_then_env_then_timestamp() {
        assert_eq!(resolve_suffix(Some("given"), "PATH"), "given");
        let path = std::env::var("PATH").unwrap();
        assert_eq!(resolve_suffix(None, "PATH"), path);
        let ts = resolve_suffix(None, "NEO_OPS_TEST_UNSET_VARIABLE");
        assert_eq!(ts.len(), 15, "{ts}");
    }
}
