use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::commands::log;
use crate::commands::web::util::escape_html;

pub fn activation_dir() -> PathBuf {
    log::operations_dir()
}

pub fn load_activation_state(id: &str) -> Option<serde_json::Value> {
    let p = activation_dir().join(format!("{}.json", id));
    if let Ok(s) = fs::read_to_string(&p) {
        if let Ok(v) = serde_json::from_str(&s) {
            return Some(v);
        }
    }
    None
}

pub fn load_log_tail(id: &str, n: usize) -> String {
    let p = activation_dir().join(format!("{}.log", id));
    if let Ok(content) = fs::read_to_string(&p) {
        let lines: Vec<&str> = content.lines().collect();
        let start = if lines.len() > n { lines.len() - n } else { 0 };
        let tail = lines[start..].join("\n");
        return tail;
    }
    "(no log yet)".to_string()
}

/// Find the most recent in-progress op whose id starts with `prefix` (e.g. "activation_" / "update_").
fn find_recent_in_progress(prefix: &str) -> Option<String> {
    let dir = activation_dir();
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

pub fn find_recent_in_progress_activation() -> Option<String> {
    find_recent_in_progress("activation_")
}

pub fn find_recent_in_progress_update() -> Option<String> {
    find_recent_in_progress("update_")
}

pub fn gc_old_activations() {
    let dir = activation_dir();
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
    let mut files: Vec<_> = fs::read_dir(&dir)
        .unwrap_or_else(|_| fs::read_dir(&dir).unwrap())
        .flatten()
        .collect();
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

#[derive(Clone, Copy)]
enum OpKind {
    Activation,
    Update,
}

/// Ordered UI steps for an operation. Index maps onto a daisyUI range (`min=0`, `max=n-1`).
struct ProgressSteps {
    labels: &'static [&'static str],
}

const ACTIVATION_STEPS: ProgressSteps = ProgressSteps {
    labels: &[
        "Start",
        "Write Flake",
        "Build",
        "Save Checkpoints",
        "Switch",
    ],
};

const UPDATE_STEPS: ProgressSteps = ProgressSteps {
    labels: &[
        "Start",
        "Reinitialize",
        "Write Flake",
        "Update Dependencies",
        "Migrate",
    ],
};

impl OpKind {
    fn monitor_id(self) -> &'static str {
        match self {
            OpKind::Activation => "activation-monitor",
            OpKind::Update => "update-monitor",
        }
    }

    fn title(self) -> &'static str {
        match self {
            OpKind::Activation => "Activation",
            OpKind::Update => "Update",
        }
    }

    fn log_div_id(self) -> &'static str {
        match self {
            OpKind::Activation => "act-log",
            OpKind::Update => "update-log",
        }
    }

    fn status_div_id(self) -> &'static str {
        match self {
            OpKind::Activation => "act-status",
            OpKind::Update => "update-status",
        }
    }

    fn log_path(self, id: &str) -> String {
        match self {
            OpKind::Activation => format!("/activation/log/{}", id),
            OpKind::Update => format!("/update/log/{}", id),
        }
    }

    fn status_path(self, id: &str) -> String {
        match self {
            OpKind::Activation => format!("/activation/status/{}", id),
            OpKind::Update => format!("/update/status/{}", id),
        }
    }

    fn steps(self) -> ProgressSteps {
        match self {
            OpKind::Activation => ACTIVATION_STEPS,
            OpKind::Update => UPDATE_STEPS,
        }
    }

    /// Map the JSON `phase` string written by activate/update onto a step index.
    fn step_index(self, phase: &str) -> usize {
        match self {
            OpKind::Activation => match phase {
                "triggered" | "starting" => 0,
                "write-flake" | "write-flake-done" => 1,
                "toplevel-build" | "toplevel-built" => 2,
                "git-add" | "build-branch" | "build-commit" | "branches-created" | "amend-add"
                | "amend-commit" | "branch-failed" => 3,
                "pre-rebuild"
                | "rebuild-failed"
                | "completed"
                | "completed-with-warnings"
                | "complete" => 4,
                _ => 0,
            },
            OpKind::Update => match phase {
                "triggered" | "starting" => 0,
                "flake init" | "post-init restore" => 1,
                "write-flake" => 2,
                "flake update" => 3,
                "migrate" | "complete" => 4,
                _ => 0,
            },
        }
    }
}

fn state_fields(id: &str) -> (String, String, String, String) {
    let st = load_activation_state(id);
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

/// Build the live status strip (phase label + daisyUI range).
///
/// Must re-emit the same `id` and `hx-*` attributes on every response: the monitor uses
/// `hx-swap="outerHTML"`, so dropping them after the first poll permanently kills updates.
fn build_status_fragment_for(kind: OpKind, id: &str) -> String {
    let (status, phase, branch, _) = state_fields(id);
    let steps = kind.steps();
    let max = steps.labels.len().saturating_sub(1);
    let mut idx = kind.step_index(&phase);
    if status == "success" {
        idx = max;
    } else if idx > max {
        idx = max;
    }

    let (label_class, range_color, headline) = match status.as_str() {
        "success" => {
            let msg = match kind {
                OpKind::Activation => {
                    let b = if branch.is_empty() {
                        id
                    } else {
                        branch.as_str()
                    };
                    format!("complete: {}", escape_html(b))
                }
                OpKind::Update => "update complete".to_string(),
            };
            ("text-success", "range-success", msg)
        }
        "failed" => (
            "text-error",
            "range-error",
            format!("failed in {}", escape_html(&phase)),
        ),
        "in_progress" => {
            let color = match kind {
                OpKind::Activation => "range-warning",
                OpKind::Update => "range-info",
            };
            let cls = match kind {
                OpKind::Activation => "text-warning",
                OpKind::Update => "text-info",
            };
            (cls, color, format!("in progress: {}", escape_html(&phase)))
        }
        _ => (
            "opacity-60",
            "range-neutral",
            format!("status: {}", escape_html(&status)),
        ),
    };

    let ticks: String = steps
        .labels
        .iter()
        .map(|_| "<span>|</span>")
        .collect::<Vec<_>>()
        .join("");
    let labels: String = steps
        .labels
        .iter()
        .map(|l| format!("<span>{}</span>", l))
        .collect::<Vec<_>>()
        .join("");

    // While in progress, re-emit id + hx-* so outerHTML polling keeps working.
    // Terminal states drop hx-trigger so we stop hitting the server every second.
    let hx_attrs = if status == "in_progress" {
        format!(
            r#" hx-get="{}" hx-trigger="load, every 1s" hx-swap="outerHTML""#,
            kind.status_path(id)
        )
    } else {
        String::new()
    };

    format!(
        r#"
        <div id="{id}"{hx_attrs} class="text-xs mt-1 space-y-1">
            <div class="flex items-center justify-between gap-2">
            <span class="{label_class}">{headline}</span>
            <span class="opacity-60 tabular-nums">{step}/{total}</span>
            </div>
            <div class="w-full">
            <input type="range" min="0" max="{max}" value="{idx}" step="1" class="w-full range range-xs {range_color} pointer-events-none" tabindex="-1" aria-label="Progress" />
            <div class="flex justify-between px-1 mt-1 text-[10px] opacity-50">{ticks}</div>
            <div class="flex justify-between px-1 text-[10px] opacity-70">{labels}</div>
            </div>
        </div>
        "#,
        id = kind.status_div_id(),
        hx_attrs = hx_attrs,
        label_class = label_class,
        headline = headline,
        step = idx + 1,
        total = steps.labels.len(),
        max = max,
        idx = idx,
        range_color = range_color,
        ticks = ticks,
        labels = labels,
    )
}

fn build_monitor_fragment_for(kind: OpKind, id: &str) -> String {
    gc_old_activations();
    let (status, _phase, branch, err) = state_fields(id);
    let mut html = String::new();
    html.push_str(&format!(
        r#"<div id="{}" data-id="{}" class="p-2 bg-base-200 rounded">
<div class="text-sm font-semibold">{} {}</div>"#,
        kind.monitor_id(),
        id,
        kind.title(),
        id,
    ));
    match (kind, status.as_str()) {
        (OpKind::Activation, "in_progress") => {
            html.push_str(r#"<div class="text-warning text-xs">Running (UI may restart during rebuild). Logs update live.</div>"#);
        }
        (OpKind::Update, "in_progress") => {
            html.push_str(r#"<div class="text-info text-xs">Running. Logs update live.</div>"#);
        }
        (OpKind::Activation, "success") => {
            html.push_str(&format!(
                r#"<div class="alert alert-success text-sm">Success as {}</div>"#,
                escape_html(&branch)
            ));
        }
        (OpKind::Update, "success") => {
            html.push_str(r#"<div class="alert alert-success text-sm">Update complete</div>"#);
        }
        (_, "failed") => {
            html.push_str(&format!(
                r#"<div class="alert alert-error text-sm">Failed: {}</div>"#,
                escape_html(&err)
            ));
        }
        _ => {}
    }
    // Progress first so step updates are visible above the log stream.
    html.push_str(&build_status_fragment_for(kind, id));
    html.push_str(&format!(
        r#"<div id="{}" class="text-[10px] bg-base-300 p-1 mt-1 max-h-80 overflow-auto font-mono" hx-get="{}" hx-trigger="load, every 1s" hx-swap="innerHTML"></div>"#,
        kind.log_div_id(),
        kind.log_path(id)
    ));
    if matches!(kind, OpKind::Activation) && status == "success" {
        html.push_str(
            r#"<div class="mt-4 flex flex-nowrap items-center justify-end gap-2" data-dialog-actions>
<button type="button" onclick="openActivationSuccess(this)" class="btn btn-sm btn-success">Confirm &amp; reload</button>
</div>"#,
        );
    }
    html.push_str("</div>");
    html
}

pub fn build_monitor_fragment(id: &str) -> String {
    build_monitor_fragment_for(OpKind::Activation, id)
}

pub fn build_update_monitor_fragment(id: &str) -> String {
    build_monitor_fragment_for(OpKind::Update, id)
}

pub fn build_status_fragment(id: &str) -> String {
    build_status_fragment_for(OpKind::Activation, id)
}

pub fn build_update_status_fragment(id: &str) -> String {
    build_status_fragment_for(OpKind::Update, id)
}

pub fn build_log_fragment(id: &str) -> String {
    let tail = load_log_tail(id, 300);
    format!(
        "<pre class=\"whitespace-pre-wrap\">{}</pre>",
        escape_html(&tail)
    )
}

pub fn is_activation_in_progress() -> bool {
    find_recent_in_progress_activation().is_some()
}
