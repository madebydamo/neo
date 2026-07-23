//! Activation / update progress UI (timeline, monitor, status, log fragments).
use crate::commands::web::ops::store::{
    find_recent_in_progress, gc_old_ops, load_state, ops_dir, state_fields,
};
use crate::commands::web::ops::timeline::{build_timeline_html, OpKind};
use crate::commands::web::util::{activation_id_ok, escape_html};

// Re-export genswitch builders so callers can use `activation::build_genswitch_*`.
pub use super::ops::genswitch::{
    build_genswitch_log_fragment, build_genswitch_monitor_fragment, build_genswitch_status_fragment,
};

pub fn activation_dir() -> std::path::PathBuf {
    ops_dir()
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
    load_state(id)
}

pub fn load_log_tail(id: &str, n: usize) -> String {
    super::ops::store::load_log_tail(id, n)
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

pub fn gc_old_activations() {
    gc_old_ops()
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
    find_recent_in_progress_activation().is_some() || find_recent_in_progress_genswitch().is_some()
}
