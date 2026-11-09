//! Live operation monitor (activation / update / store repair / generation switch).
//!
//! The server only renders a mount point ([`monitor_fragment`]); `static/op_monitor.js`
//! builds the dialog body once and keeps it in place, fed by `/ws/op/<id>`:
//!
//! - `{"t":"state", status, phase, steps, step, …}` whenever the op JSON changes
//! - `{"t":"log", from, to, data, truncated?}` appended log bytes (whole lines only
//!   until the op ends), so the client appends text instead of re-rendering it
//! - `{"t":"reset"}` when the log file shrank (client clears and starts over)
//! - `{"t":"end"}` once the op is terminal and the log is drained
//!
//! The client reconnects with `?offset=<to>` after a drop (e.g. neo-web restarting
//! during a switch) and continues exactly where it left off.
use crate::utils::ops::OpStatus;
use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use serde_json::{json, Value};

use super::kind::id_timestamp;
use super::store::state_path;
use crate::commands::web::util::{escape_attr, escape_html, op_id_ok};
use crate::utils::ops::OperationKind;

/// Most bytes sent per log message.
const MAX_CHUNK: usize = 256 * 1024;
/// On a fresh connect (offset 0) only the last part of a huge log is sent.
const INITIAL_TAIL: u64 = 512 * 1024;

/// Mount point for the live monitor. `note` is an optional notice shown above the
/// progress (e.g. "the web UI may restart").
pub fn monitor_fragment(id: &str, note: Option<&str>) -> String {
    let Some(kind) = OperationKind::from_id(id).filter(|_| op_id_ok(id)) else {
        return format!(
            r#"<div class="alert alert-error text-sm">invalid operation id: {}</div>"#,
            escape_html(id)
        );
    };
    let note_attr = note
        .map(|n| format!(r#" data-op-note="{}""#, escape_attr(n)))
        .unwrap_or_default();
    format!(
        r#"<div class="neo-op" data-op-monitor="{id}" data-op-title="{title}" data-op-subtitle="{sub}"{note_attr}><div class="flex items-center justify-center gap-2 py-10 text-sm text-base-content/50"><span class="loading loading-spinner loading-sm"></span>Connecting…</div></div>"#,
        id = escape_attr(id),
        title = escape_attr(kind.title()),
        sub = escape_attr(id_timestamp(id)),
    )
}

fn str_field<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(|s| s.as_str()).unwrap_or("")
}

/// Raw op JSON (for change detection) plus the client `state` message.
/// `None` when the op has no state file (garbage-collected or never started).
pub fn read_state_message(id: &str) -> Option<(String, Value)> {
    let kind = OperationKind::from_id(id)?;
    let raw = fs::read_to_string(state_path(id)).ok()?;
    let v: Value = serde_json::from_str(&raw).ok()?;
    Some((raw, state_message(kind, id, &v)))
}

pub fn state_message(kind: OperationKind, id: &str, v: &Value) -> Value {
    let status = str_field(v, "status");
    let phase = str_field(v, "phase");
    let steps = kind.steps();
    let last = steps.len().saturating_sub(1);
    let step = if status == OpStatus::Success.as_str() {
        last
    } else {
        kind.step_index(phase).min(last)
    };
    json!({
        "t": "state",
        "id": id,
        "title": kind.title(),
        "status": status,
        "phase": phase,
        "error": str_field(v, "error"),
        "branch": str_field(v, "branch"),
        "generation": v.get("generation").cloned().unwrap_or(Value::Null),
        "mode": str_field(v, "mode"),
        "kind": kind.prefix(),
        "steps": steps,
        "step": step,
    })
}

/// Read position in an op log; yields `log` / `reset` messages.
pub struct LogCursor {
    offset: u64,
    fresh: bool,
    /// Next `log` message starts mid-file (older output was skipped).
    truncated: bool,
}

impl LogCursor {
    pub fn new(offset: u64) -> Self {
        LogCursor {
            offset,
            fresh: offset == 0,
            truncated: false,
        }
    }

    /// New log bytes since the last call. Only whole lines unless `flush` (op ended),
    /// so a line is never split across messages while it is still being written.
    pub fn read(&mut self, path: &Path, flush: bool) -> Vec<Value> {
        let mut out = Vec::new();
        let Ok(mut f) = fs::File::open(path) else {
            return out;
        };
        let len = f.metadata().map(|m| m.len()).unwrap_or(0);
        if len < self.offset {
            // Truncated / replaced: start over.
            self.offset = 0;
            self.fresh = true;
            out.push(json!({ "t": "reset" }));
        }
        if self.fresh {
            self.fresh = false;
            if len > INITIAL_TAIL {
                // Skip ahead to the first full line of the tail.
                let mut pos = len - INITIAL_TAIL;
                if f.seek(SeekFrom::Start(pos)).is_ok() {
                    let mut probe = vec![0u8; 64 * 1024];
                    if let Ok(n) = f.read(&mut probe) {
                        if let Some(i) = probe[..n].iter().position(|&b| b == b'\n') {
                            pos += i as u64 + 1;
                        }
                    }
                }
                self.offset = pos;
                self.truncated = true;
            }
        }
        while self.offset < len {
            if f.seek(SeekFrom::Start(self.offset)).is_err() {
                break;
            }
            let want = ((len - self.offset) as usize).min(MAX_CHUNK);
            let mut buf = vec![0u8; want];
            let Ok(n) = f.read(&mut buf) else { break };
            buf.truncate(n);
            if n == 0 {
                break;
            }
            let mut end = buf.len();
            if !flush || n == MAX_CHUNK {
                match buf.iter().rposition(|&b| b == b'\n') {
                    Some(i) => end = i + 1,
                    // No complete line yet: wait (unless a single line fills a chunk).
                    None if n < MAX_CHUNK => break,
                    None => {}
                }
            }
            let from = self.offset;
            self.offset += end as u64;
            let mut msg = json!({
                "t": "log",
                "from": from,
                "to": self.offset,
                "data": String::from_utf8_lossy(&buf[..end]),
            });
            if self.truncated {
                msg["truncated"] = json!(true);
                self.truncated = false;
            }
            out.push(msg);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn tmp(name: &str) -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!("neo-op-monitor-{}-{name}", std::process::id()));
        let _ = fs::remove_file(&p);
        p
    }

    fn append(p: &Path, s: &str) {
        let mut f = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(p)
            .unwrap();
        f.write_all(s.as_bytes()).unwrap();
    }

    fn data(msgs: &[Value]) -> String {
        msgs.iter()
            .filter(|m| m["t"] == "log")
            .map(|m| m["data"].as_str().unwrap().to_string())
            .collect()
    }

    #[test]
    fn whole_lines_until_flush() {
        let p = tmp("lines");
        append(&p, "one\ntw");
        let mut c = LogCursor::new(0);
        assert_eq!(data(&c.read(&p, false)), "one\n");
        assert!(c.read(&p, false).is_empty());
        append(&p, "o\nthree");
        assert_eq!(data(&c.read(&p, false)), "two\n");
        assert_eq!(data(&c.read(&p, true)), "three");
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn resume_from_offset_and_reset() {
        let p = tmp("resume");
        append(&p, "a\nb\n");
        let mut c = LogCursor::new(2);
        let m = c.read(&p, false);
        assert_eq!(data(&m), "b\n");
        assert_eq!(m[0]["from"], 2);
        assert_eq!(m[0]["to"], 4);
        fs::write(&p, "x\n").unwrap();
        let m = c.read(&p, false);
        assert_eq!(m[0]["t"], "reset");
        assert_eq!(data(&m), "x\n");
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn huge_log_sends_tail_from_line_start() {
        let p = tmp("tail");
        let line = "0123456789abcdef0123456789abcdef0123456789abcdef012345678\n"; // 59 bytes
        let mut s = String::new();
        while (s.len() as u64) < INITIAL_TAIL + 1000 {
            s.push_str(line);
        }
        fs::write(&p, &s).unwrap();
        let mut c = LogCursor::new(0);
        let m = c.read(&p, false);
        assert_eq!(m[0]["truncated"], true);
        let got = data(&m);
        assert!(got.starts_with("0123"));
        assert!(got.len() as u64 <= INITIAL_TAIL);
        assert_eq!(c.offset, s.len() as u64);
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn state_message_steps() {
        let v = json!({"status": "success", "phase": "completed", "branch": "b"});
        let m = state_message(OperationKind::Activation, "activation_1", &v);
        assert_eq!(m["step"], 5);
        assert_eq!(m["title"], "Activation");
        let v = json!({"status": "in_progress", "phase": "nix-store-verify-repair"});
        let m = state_message(OperationKind::Repair, "repair_1", &v);
        assert_eq!(m["steps"].as_array().unwrap().len(), 0);
    }
}
