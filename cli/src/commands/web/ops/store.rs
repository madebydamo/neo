//! Shared on-disk operation store (JSON state + log files under the ops dir).
//! Used by activation, update, genswitch, and nix-store repair jobs.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::commands::web::util::{activation_id_ok, repair_id_ok};
use crate::utils::ops::{self, append_log as ops_append_log, write_op_state};

pub fn ops_dir() -> PathBuf {
    ops::operations_dir()
}

pub fn state_path(id: &str) -> PathBuf {
    ops::state_path(id)
}

pub fn log_path(id: &str) -> PathBuf {
    ops::log_path(id)
}

fn id_ok(id: &str) -> bool {
    activation_id_ok(id) || repair_id_ok(id)
}

pub fn load_state(id: &str) -> Option<serde_json::Value> {
    if !id_ok(id) {
        return None;
    }
    let s = fs::read_to_string(state_path(id)).ok()?;
    serde_json::from_str(&s).ok()
}

pub fn load_log_tail(id: &str, n: usize) -> String {
    if !id_ok(id) {
        return "(invalid id)".to_string();
    }
    let p = log_path(id);
    if let Ok(content) = fs::read_to_string(&p) {
        let lines: Vec<&str> = content.lines().collect();
        let start = if lines.len() > n { lines.len() - n } else { 0 };
        return lines[start..].join("\n");
    }
    "(no log yet)".to_string()
}

/// Write op state (thin wrapper over shared [`write_op_state`]).
pub fn write_state(id: &str, status: &str, phase: &str, err: Option<&str>) {
    write_op_state(id, status, phase, err, None, None, None);
}

pub fn append_log(path: &Path, line: &str) {
    ops_append_log(path, line);
}

/// Find the most recent in-progress op whose id starts with `prefix` (within last hour).
pub fn find_recent_in_progress(prefix: &str) -> Option<String> {
    let dir = ops_dir();
    if !dir.exists() {
        return None;
    }
    let mut best: Option<(String, u64)> = None;
    if let Ok(rd) = fs::read_dir(&dir) {
        for e in rd.flatten() {
            let name = e.file_name();
            let name = name.to_string_lossy();
            if !(name.ends_with(".json") && name.starts_with(prefix)) {
                continue;
            }
            let Ok(meta) = e.metadata() else { continue };
            let Ok(mtime) = meta.modified() else { continue };
            let t = mtime
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            if now > t + 3600 {
                continue;
            }
            let Ok(s) = fs::read_to_string(e.path()) else {
                continue;
            };
            let Ok(v) = serde_json::from_str::<serde_json::Value>(&s) else {
                continue;
            };
            if v.get("status").and_then(|x| x.as_str()) != Some("in_progress") {
                continue;
            }
            if best.as_ref().map_or(true, |&(_, bt)| t > bt) {
                let id = name.trim_end_matches(".json").to_string();
                best = Some((id, t));
            }
        }
    }
    best.map(|(id, _)| id)
}

/// Drop ops older than 7 days, then keep only the 10 newest files.
/// Called from the action-bar watcher and once at op trigger start — not from
/// per-poll monitor/status/log fragment builders (those are on the hot path).
pub fn gc_old_ops() {
    let dir = ops_dir();
    if !dir.exists() {
        return;
    }
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    if let Ok(rd) = fs::read_dir(&dir) {
        for e in rd.flatten() {
            let p = e.path();
            if let Ok(meta) = e.metadata() {
                if let Ok(mtime) = meta.modified() {
                    let t = mtime
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs();
                    if now > t + 7 * 86400 {
                        let _ = fs::remove_file(&p);
                    }
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

/// Read common status fields from an op state document.
pub fn state_fields(id: &str) -> (String, String, String, String) {
    let st = load_state(id);
    let status = st
        .as_ref()
        .and_then(|v| v.get("status").and_then(|s| s.as_str()))
        .unwrap_or("unknown")
        .to_string();
    let phase = st
        .as_ref()
        .and_then(|v| v.get("phase").and_then(|s| s.as_str()))
        .unwrap_or("")
        .to_string();
    let branch = st
        .as_ref()
        .and_then(|v| v.get("branch").and_then(|s| s.as_str()))
        .unwrap_or("")
        .to_string();
    let err = st
        .as_ref()
        .and_then(|v| v.get("error").and_then(|s| s.as_str()))
        .unwrap_or("")
        .to_string();
    (status, phase, branch, err)
}
