//! Shared on-disk operation store (JSON state + log files under the ops dir).
//! Used by activation, update, genswitch, and nix-store repair jobs.

use std::fs;
use std::time::SystemTime;

use crate::utils::command::{mtime_secs, now_secs};
use crate::utils::locks::{Holder, LockManager};
pub use crate::utils::ops::{append_log, log_path, state_path};
use crate::utils::ops::{operations_dir, write_op_state, OpStatus, OperationKind};

/// How long a web-triggered op may stay in phase `triggered` before the child
/// process has taken its lock (systemd-run handoff + short lock wait).
const TRIGGER_GRACE_SECS: u64 = 120;

/// Write op state (no branch / start time / extra fields).
pub fn write_state(id: &str, status: OpStatus, phase: &str, err: Option<&str>) {
    write_op_state(id, status, phase, err, None, None, None);
}

/// True when a live lock holder still owns this op (or the web trigger grace applies).
fn op_still_running(
    id: &str,
    kind: OperationKind,
    phase: &str,
    mtime_secs: u64,
    now_secs: u64,
    holders: &[Holder],
) -> bool {
    if holders.iter().any(|h| h.op_id.as_deref() == Some(id)) {
        return true;
    }
    let prefix = kind.prefix();
    // CLI activate/update without a suffix leaves the holder without `op_id`.
    // Trust kind only when the holder does not claim a different op.
    if holders
        .iter()
        .any(|h| h.kind == prefix && h.op_id.as_deref().is_none_or(|oid| oid == id))
    {
        return true;
    }
    // Web UI writes `triggered` before systemd-run's child acquires the lock.
    if phase == "triggered" && now_secs.saturating_sub(mtime_secs) < TRIGGER_GRACE_SECS {
        return true;
    }
    false
}

/// Mark an in-progress op finished as failed when its process is gone.
fn mark_op_aborted(id: &str, last_phase: &str) {
    let existing = fs::read_to_string(state_path(id))
        .ok()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok());
    let started_at = existing
        .as_ref()
        .and_then(|v| v.get("started_at").and_then(|x| x.as_str()));
    let branch = existing
        .as_ref()
        .and_then(|v| v.get("branch").and_then(|x| x.as_str()));
    let mut extra = serde_json::Map::new();
    if let Some(v) = existing.as_ref() {
        for key in ["generation", "mode"] {
            if let Some(val) = v.get(key) {
                extra.insert(key.to_string(), val.clone());
            }
        }
    }
    let err = format!("no longer running (last phase: {last_phase}); aborted or crashed");
    write_op_state(
        id,
        OpStatus::Failed,
        "aborted",
        Some(&err),
        branch,
        started_at,
        if extra.is_empty() {
            None
        } else {
            Some(serde_json::Value::Object(extra))
        },
    );
    append_log(
        &log_path(id),
        &format!("operation {id} marked aborted (process no longer running)"),
    );
}

/// An op whose state file says `in_progress`.
struct RunningOp {
    id: String,
    kind: OperationKind,
    phase: String,
    mtime: u64,
}

/// Every op with `status = in_progress`, whatever its process is doing.
fn in_progress_ops() -> Vec<RunningOp> {
    let Ok(rd) = fs::read_dir(operations_dir()) else {
        return Vec::new();
    };
    rd.flatten()
        .filter_map(|e| {
            let name = e.file_name();
            let id = name.to_str()?.strip_suffix(".json")?.to_string();
            let kind = OperationKind::from_id(&id)?;
            let mtime = mtime_secs(&e.metadata().ok()?);
            let s = fs::read_to_string(e.path()).ok()?;
            let v: serde_json::Value = serde_json::from_str(&s).ok()?;
            let in_progress = OpStatus::InProgress.as_str();
            (v.get("status").and_then(|x| x.as_str()) == Some(in_progress)).then(|| RunningOp {
                id,
                kind,
                phase: v
                    .get("phase")
                    .and_then(|x| x.as_str())
                    .unwrap_or("")
                    .to_string(),
                mtime,
            })
        })
        .collect()
}

/// Finish in-progress ops whose lock holder is gone (Ctrl+C / crash / killed unit).
/// Safe to call often: only rewrites state when something is actually stale.
pub fn reap_stale_ops() {
    let now = now_secs();
    let holders = LockManager::system().holders();
    for op in in_progress_ops() {
        if !op_still_running(&op.id, op.kind, &op.phase, op.mtime, now, &holders) {
            mark_op_aborted(&op.id, &op.phase);
        }
    }
}

/// Most recent in-progress op of each `kind` (within the last hour), `None` where
/// none runs. Stale ops are reaped once, before looking.
pub fn running_ops<const N: usize>(kinds: [OperationKind; N]) -> [Option<String>; N] {
    // Drop aborted CLI/web jobs before we advertise them as still running.
    reap_stale_ops();
    let now = now_secs();
    let ops = in_progress_ops();
    kinds.map(|kind| {
        ops.iter()
            .filter(|op| op.kind == kind && now <= op.mtime + 3600)
            .max_by_key(|op| op.mtime)
            .map(|op| op.id.clone())
    })
}

/// Most recent in-progress op of `kind` (within the last hour).
pub fn find_recent_in_progress(kind: OperationKind) -> Option<String> {
    let [id] = running_ops([kind]);
    id
}

/// Drop ops older than 7 days, then keep only the 10 newest files.
/// Called from the action-bar watcher and once at op trigger start — not from
/// per-poll monitor/status/log fragment builders (those are on the hot path).
pub fn gc_old_ops() {
    reap_stale_ops();

    let dir = operations_dir();
    if !dir.exists() {
        return;
    }
    let now = now_secs();
    if let Ok(rd) = fs::read_dir(&dir) {
        for e in rd.flatten() {
            if let Ok(meta) = e.metadata() {
                if now > mtime_secs(&meta) + 7 * 86400 {
                    let _ = fs::remove_file(e.path());
                }
            }
        }
    }
    let mut files: Vec<_> = match fs::read_dir(&dir) {
        Ok(rd) => rd.flatten().collect(),
        Err(_) => return,
    };
    if files.len() > 10 {
        files.sort_by_key(|e| {
            e.metadata()
                .ok()
                .and_then(|m| m.modified().ok())
                .unwrap_or(SystemTime::UNIX_EPOCH)
        });
        for old in files.iter().take(files.len() - 10) {
            let _ = fs::remove_file(old.path());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::locks::{HeldScope, LockMode};

    fn holder(kind: &str, op_id: Option<&str>) -> Holder {
        Holder {
            id: "1-0".into(),
            scopes: vec![HeldScope {
                scope: "system".into(),
                mode: LockMode::Exclusive,
            }],
            kind: kind.into(),
            label: kind.into(),
            op_id: op_id.map(|s| s.into()),
            pid: 1,
            started_at: 0,
            guarded_units: vec![],
        }
    }

    #[test]
    fn live_when_holder_claims_op_id() {
        let h = vec![holder("activation", Some("activation_1"))];
        assert!(op_still_running(
            "activation_1",
            OperationKind::Activation,
            "pre-rebuild",
            100,
            500,
            &h
        ));
        assert!(!op_still_running(
            "activation_2",
            OperationKind::Activation,
            "pre-rebuild",
            100,
            500,
            &h
        ));
    }

    #[test]
    fn live_when_cli_holder_has_kind_only() {
        let h = vec![holder("update", None)];
        assert!(op_still_running(
            "update_1",
            OperationKind::Update,
            "flake update",
            100,
            500,
            &h
        ));
        assert!(!op_still_running(
            "activation_1",
            OperationKind::Activation,
            "starting",
            100,
            500,
            &h
        ));
    }

    #[test]
    fn triggered_grace_without_holder() {
        assert!(op_still_running(
            "activation_1",
            OperationKind::Activation,
            "triggered",
            100,
            150,
            &[]
        ));
        assert!(!op_still_running(
            "activation_1",
            OperationKind::Activation,
            "triggered",
            100,
            100 + TRIGGER_GRACE_SECS + 1,
            &[]
        ));
        // Past grace and not triggered → dead.
        assert!(!op_still_running(
            "activation_1",
            OperationKind::Activation,
            "starting",
            100,
            150,
            &[]
        ));
    }
}
