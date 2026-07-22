use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::commands::log;
use crate::commands::web::util::{activation_id_ok, escape_html};

pub fn activation_dir() -> PathBuf {
    log::operations_dir()
}

fn invalid_id_fragment(id: &str) -> String {
    format!(
        r#"<div class="alert alert-error text-sm">invalid activation id: {}</div>"#,
        escape_html(id)
    )
}

pub fn load_activation_state(id: &str) -> Option<serde_json::Value> {
    if !activation_id_ok(id) {
        return None;
    }
    let p = activation_dir().join(format!("{}.json", id));
    if let Ok(s) = fs::read_to_string(&p) {
        if let Ok(v) = serde_json::from_str(&s) {
            return Some(v);
        }
    }
    None
}

pub fn load_log_tail(id: &str, n: usize) -> String {
    if !activation_id_ok(id) {
        return "(invalid id)".to_string();
    }
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

pub fn find_recent_in_progress_genswitch() -> Option<String> {
    find_recent_in_progress("genswitch_")
}

/// Lightweight monitor snippet for detached generation switch/boot oneshots.
pub fn build_genswitch_monitor_fragment(id: &str, generation: u64, mode: &str) -> String {
    let safe_id = escape_html(id);
    let mode_label = if mode == "boot" {
        "Set boot default"
    } else {
        "Switch generation"
    };
    format!(
        r#"<div class="space-y-2" id="genswitch-monitor" data-genswitch-id="{id}">
  <div class="alert alert-warning text-sm">
    <span><strong>{mode_label} {gen}</strong> started as a background job.
    The web UI may restart while the system switches — this is expected.
    Wait ~30s then reload if the page disconnects.</span>
  </div>
  <div class="text-xs opacity-60 font-mono">job {safe_id}</div>
  <div id="genswitch-status"
       hx-get="/genswitch/status/{id}"
       hx-trigger="every 2s"
       hx-swap="innerHTML">
    <span class="loading loading-spinner loading-xs"></span> starting…
  </div>
  <pre class="text-[10px] font-mono overflow-auto max-h-40 bg-base-300 p-2 rounded"
       hx-get="/genswitch/log/{id}"
       hx-trigger="every 2s"
       hx-swap="innerHTML">(log…)</pre>
</div>"#,
        id = safe_id,
        gen = generation,
        mode_label = mode_label,
        safe_id = safe_id,
    )
}

pub fn build_genswitch_status_fragment(id: &str) -> String {
    let Some(st) = load_activation_state(id) else {
        return r#"<span class="text-xs opacity-50">waiting for state…</span>"#.to_string();
    };
    let status = st
        .get("status")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");
    let phase = st
        .get("phase")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let gen = st
        .get("generation")
        .and_then(|v| v.as_u64())
        .map(|n| n.to_string())
        .unwrap_or_else(|| "?".to_string());
    let err = st
        .get("error")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    match status {
        "success" => format!(
            r#"<span class="text-success text-sm">Generation {gen} {phase} — done. Reload the page if needed.</span>"#
        ),
        "failed" => format!(
            r#"<span class="text-error text-sm">Generation {gen} failed ({phase}): {}</span>"#,
            escape_html(err)
        ),
        _ => format!(
            r#"<span class="text-warning text-sm"><span class="loading loading-spinner loading-xs"></span> gen {gen} · {phase}</span>"#
        ),
    }
}

pub fn build_genswitch_log_fragment(id: &str) -> String {
    escape_html(&load_log_tail(id, 80))
}

/// Drop ops older than 7 days, then keep only the 10 newest files.
/// Called from the action-bar watcher and once at op trigger start — not from
/// per-poll monitor/status/log fragment builders (those are on the hot path).
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

#[derive(Clone, Copy)]
enum OpKind {
    Activation,
    Update,
}

/// Ordered UI steps for an operation. Last label is always "Finished" so the
/// final working phase is never shown as a full bar (which reads as "done").
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
        "Finished",
    ],
};

const UPDATE_STEPS: ProgressSteps = ProgressSteps {
    labels: &[
        "Start",
        "Reinitialize",
        "Write Flake",
        "Update Dependencies",
        "Migrate",
        "Finished",
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
    /// Terminal "complete*" phases land on the final "Finished" step.
    fn step_index(self, phase: &str) -> usize {
        match self {
            OpKind::Activation => match phase {
                "triggered" | "starting" => 0,
                "write-flake" | "write-flake-done" => 1,
                "toplevel-build" | "toplevel-built" => 2,
                "git-add" | "build-branch" | "build-commit" | "branches-created" | "amend-add"
                | "amend-commit" | "branch-failed" => 3,
                "pre-rebuild" | "rebuild-failed" => 4,
                "completed" | "completed-with-warnings" | "complete" => 5,
                _ => 0,
            },
            OpKind::Update => match phase {
                "triggered" | "starting" => 0,
                "flake init" | "post-init restore" => 1,
                "write-flake" => 2,
                "flake update" => 3,
                "migrate" => 4,
                "complete" => 5,
                _ => 0,
            },
        }
    }

    fn active_color(self) -> &'static str {
        match self {
            OpKind::Activation => "text-warning",
            OpKind::Update => "text-info",
        }
    }
}

/// Checkmark icon for completed timeline steps.
const ICON_DONE: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" fill="currentColor" class="h-5 w-5"><path fill-rule="evenodd" d="M10 18a8 8 0 100-16 8 8 0 000 16zm3.857-9.809a.75.75 0 00-1.214-.882l-3.483 4.79-1.88-1.88a.75.75 0 10-1.06 1.061l2.5 2.5a.75.75 0 001.137-.089l4-5.5z" clip-rule="evenodd" /></svg>"#;

/// Hollow circle for pending timeline steps.
const ICON_PENDING: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" fill="currentColor" class="h-5 w-5 opacity-30"><path fill-rule="evenodd" d="M10 18a8 8 0 100-16 8 8 0 000 16zm0-2a6 6 0 100-12 6 6 0 000 12z" clip-rule="evenodd" /></svg>"#;

/// Error X for the failed step.
const ICON_FAILED: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" fill="currentColor" class="h-5 w-5"><path fill-rule="evenodd" d="M10 18a8 8 0 100-16 8 8 0 000 16zM8.28 7.22a.75.75 0 00-1.06 1.06L8.94 10l-1.72 1.72a.75.75 0 101.06 1.06L10 11.06l1.72 1.72a.75.75 0 101.06-1.06L11.06 10l1.72-1.72a.75.75 0 00-1.06-1.06L10 8.94 8.28 7.22z" clip-rule="evenodd" /></svg>"#;

const ICON_SPINNER: &str = r#"<span class="loading loading-ring loading-sm"></span>"#;

/// Build a daisyUI horizontal timeline for the given step progress.
fn build_timeline_html(kind: OpKind, labels: &[&str], idx: usize, status: &str) -> String {
    let n = labels.len();
    let mut items = String::new();
    for (i, label) in labels.iter().enumerate() {
        let done = status == "success" || i < idx;
        let current = i == idx && status != "success";
        let failed = current && status == "failed";
        let running = current && status == "in_progress";

        let (icon, icon_cls, box_cls) = if failed {
            (ICON_FAILED, "text-error", "timeline-box border-error")
        } else if done {
            (ICON_DONE, "text-success", "timeline-box")
        } else if running {
            (
                ICON_SPINNER,
                kind.active_color(),
                "timeline-box border-current",
            )
        } else {
            (ICON_PENDING, "opacity-50", "timeline-box opacity-60")
        };

        // Connector only fills once this step is done (next bullet has started).
        // Do not color while the current step is still running — that looked
        // like progress had already advanced to the next step.
        let hr_after = if i + 1 < n {
            if done {
                r#"<hr class="bg-success"/>"#.to_string()
            } else {
                "<hr/>".to_string()
            }
        } else {
            String::new()
        };

        // Leading connector mirrors the previous step's trailing one.
        let hr_before = if i > 0 {
            let prev_done = status == "success" || i - 1 < idx;
            if prev_done {
                r#"<hr class="bg-success"/>"#.to_string()
            } else {
                "<hr/>".to_string()
            }
        } else {
            String::new()
        };

        // Active (running) step: wrap the label box in daisyUI aura so it
        // stands out as a rotating border light around the stage.
        //
        // The status strip is hx-swapped every 1s (outerHTML), which remounts
        // the node and would reset CSS animations. Negative animation-delay
        // phase-locks to wall clock so the ring continues mid-cycle across
        // remounts (daisyUI default aura period is 6s).
        // Poll with `every 1s` only — never `load` with outerHTML (each swap
        // remounts and would re-fire load → request storm).
        let end = if running {
            let phase_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() % 6000)
                .unwrap_or(0);
            format!(
                r#"<div class="timeline-end"><div class="aura aura-sm {color}" style="animation-delay:-{phase_ms}ms"><div class="{box_cls} bg-base-100 text-[10px] whitespace-nowrap">{label}</div></div></div>"#,
                color = kind.active_color(),
                phase_ms = phase_ms,
                box_cls = box_cls,
                label = label,
            )
        } else {
            format!(
                r#"<div class="timeline-end {box_cls} text-[10px] whitespace-nowrap">{label}</div>"#,
                box_cls = box_cls,
                label = label,
            )
        };

        items.push_str(&format!(
            r#"<li>{hr_before}<div class="timeline-middle {icon_cls}">{icon}</div>{end}{hr_after}</li>"#,
            hr_before = hr_before,
            icon_cls = icon_cls,
            icon = icon,
            end = end,
            hr_after = hr_after,
        ));
    }
    format!(
        r#"<ul class="timeline timeline-horizontal timeline-compact w-full justify-center overflow-x-auto">{items}</ul>"#,
        items = items
    )
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

/// Suffix after `activation_` / `update_` (the op timestamp), or the full id.
fn id_timestamp(id: &str) -> &str {
    id.strip_prefix("activation_")
        .or_else(|| id.strip_prefix("update_"))
        .unwrap_or(id)
}

/// Dialog title HTML: bold label + fine timestamp (for `h3#changes-modal-title`).
fn dialog_title_html(kind: OpKind, id: &str) -> String {
    format!(
        r#"{} <span class="font-normal text-sm opacity-50">{}</span>"#,
        kind.title(),
        escape_html(id_timestamp(id)),
    )
}

/// Build the live status strip (daisyUI timeline only — phase text is redundant).
///
/// Must re-emit the same `id` and `hx-*` attributes on every in-progress response: the
/// monitor uses `hx-swap="outerHTML"`, so dropping them after the first poll permanently
/// kills updates. Use `every 1s` only (no `load`) — `load` + outerHTML remounts re-fire
/// load immediately and storm the server. Terminal status *poll* responses drop polling
/// and OOB-replace the log panel so its interval dies too. Action-bar WS already surfaces
/// completion; the dialog only needs live progress while it is open.
///
/// `stop_log_oob`: when true (status HTTP poll path), terminal responses include an OOB
/// log replace. When false (embedded in full monitor HTML), the caller owns the log panel.
fn build_status_fragment_for(kind: OpKind, id: &str, stop_log_oob: bool) -> String {
    if !activation_id_ok(id) {
        return invalid_id_fragment(id);
    }
    let (status, phase, _, _) = state_fields(id);
    let steps = kind.steps();
    let max = steps.labels.len().saturating_sub(1);
    let mut idx = kind.step_index(&phase);
    if status == "success" {
        idx = max;
    } else if idx > max {
        idx = max;
    }

    let timeline = build_timeline_html(kind, steps.labels, idx, &status);

    // While in progress, re-emit id + hx-* so outerHTML polling keeps working.
    // Never include `load` here — outerHTML remount would re-trigger load in a tight loop.
    // Terminal states drop hx-trigger so we stop hitting the server.
    let hx_attrs = if status == "in_progress" {
        format!(
            r#" hx-get="{}" hx-trigger="every 1s" hx-swap="outerHTML""#,
            kind.status_path(id)
        )
    } else {
        String::new()
    };

    let mut html = format!(
        r#"<div id="{id}"{hx_attrs} class="text-xs mt-1">{timeline}</div>"#,
        id = kind.status_div_id(),
        hx_attrs = hx_attrs,
        timeline = timeline,
    );

    // Log uses innerHTML polling and is not remounted with status — without this OOB
    // it would keep requesting forever after the op finishes.
    if stop_log_oob && status != "in_progress" {
        let tail = load_log_tail(id, 300);
        html.push_str(&format!(
            r#"<div id="{log_id}" class="text-[10px] bg-base-300 p-1 mt-1 max-h-80 overflow-auto font-mono" hx-swap-oob="true"><pre class="whitespace-pre-wrap">{tail}</pre></div>"#,
            log_id = kind.log_div_id(),
            tail = escape_html(&tail),
        ));
    }

    html
}

fn build_monitor_fragment_for(kind: OpKind, id: &str) -> String {
    // GC intentionally not called here — see gc_old_activations docs.
    if !activation_id_ok(id) {
        return invalid_id_fragment(id);
    }
    let (status, _phase, branch, err) = state_fields(id);
    let mut html = String::new();
    // OOB: put the op timestamp in the dialog title (fine text, not bold).
    html.push_str(&format!(
        r#"<h3 id="changes-modal-title" class="font-bold text-lg mb-2" hx-swap-oob="true">{}</h3>"#,
        dialog_title_html(kind, id),
    ));
    html.push_str(&format!(
        r#"<div id="{}" data-id="{}" class="p-2 bg-base-200 rounded">"#,
        kind.monitor_id(),
        escape_html(id),
    ));
    match (kind, status.as_str()) {
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
    // Status polling is nested inside build_status_fragment_for (every 1s while in_progress).
    html.push_str(&build_status_fragment_for(kind, id, false));
    // Log: poll only while the op is running (innerHTML keeps the element stable, so
    // `load` is safe here). Terminal opens get a static snapshot — no interval.
    // When a live status poll hits terminal, it OOB-replaces this panel (stop_log_oob).
    if status == "in_progress" {
        html.push_str(&format!(
            r#"<div id="{}" class="text-[10px] bg-base-300 p-1 mt-1 max-h-80 overflow-auto font-mono" hx-get="{}" hx-trigger="load, every 1s" hx-swap="innerHTML"></div>"#,
            kind.log_div_id(),
            kind.log_path(id)
        ));
    } else {
        let tail = load_log_tail(id, 300);
        html.push_str(&format!(
            r#"<div id="{}" class="text-[10px] bg-base-300 p-1 mt-1 max-h-80 overflow-auto font-mono"><pre class="whitespace-pre-wrap">{}</pre></div>"#,
            kind.log_div_id(),
            escape_html(&tail)
        ));
    }
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
    build_status_fragment_for(OpKind::Activation, id, true)
}

pub fn build_update_status_fragment(id: &str) -> String {
    build_status_fragment_for(OpKind::Update, id, true)
}

pub fn build_log_fragment(id: &str) -> String {
    if !activation_id_ok(id) {
        return invalid_id_fragment(id);
    }
    let tail = load_log_tail(id, 300);
    format!(
        "<pre class=\"whitespace-pre-wrap\">{}</pre>",
        escape_html(&tail)
    )
}

pub fn is_activation_in_progress() -> bool {
    find_recent_in_progress_activation().is_some()
        || find_recent_in_progress_genswitch().is_some()
}
