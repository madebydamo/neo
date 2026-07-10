use std::process::Command;
use std::sync::Arc;
use std::time::Duration;

use rocket::response::content::RawHtml;
use tokio::process::Command as AsyncCommand;

use super::structs::AppConfig;
use super::util::{escape_html, sudo_cmd};

pub fn unit_name_valid(unit: &str) -> bool {
    !unit.is_empty()
        && unit.len() <= 256
        && unit
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-@._".contains(c))
}

/// Query systemctl is-active for a unit (sync; used from HTTP handlers and render).
pub fn unit_active_state(unit: &str) -> String {
    let sudo = sudo_cmd();
    Command::new(&sudo)
        .args(["systemctl", "is-active", unit])
        .output()
        .map(|o| {
            let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
            if s.is_empty() {
                "unknown".into()
            } else {
                s
            }
        })
        .unwrap_or_else(|_| "unknown".into())
}

pub async fn unit_active_state_async(unit: &str) -> String {
    let sudo = sudo_cmd();
    match AsyncCommand::new(&sudo)
        .args(["systemctl", "is-active", unit])
        .output()
        .await
    {
        Ok(o) => {
            let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
            if s.is_empty() {
                "unknown".into()
            } else {
                s
            }
        }
        Err(_) => "unknown".into(),
    }
}

/// Build the inner content (dot + state + buttons) for a unit controls area.
/// Used for OOB WS pushes and composed into full divs.
///
/// Buttons stay stable across transitional states so restart/stop never "vanish"
/// while systemctl --no-block is still settling (the live WS watcher re-renders
/// as soon as ActiveState changes).
pub fn render_unit_controls_content_with_state(unit: &str, active: &str) -> String {
    let is_container = unit.starts_with("docker-");

    let dot_cls = match active {
        "active" => "bg-success",
        "inactive" => "bg-base-300",
        "activating" | "deactivating" | "reloading" => "bg-info animate-pulse",
        "failed" => "bg-error",
        _ => "bg-warning",
    };

    let u = escape_html(unit);
    let state_label = escape_html(active);
    // Basic JS string escape for onclick arg (single quotes in unit names are rare for units)
    let u_js = u.replace('\'', "\\'");

    let mut inner = String::new();
    inner.push_str(&format!(
        r#"<span class="inline-block w-2 h-2 rounded-full flex-shrink-0 {}" title="{}"></span>"#,
        dot_cls, u
    ));
    inner.push_str(&format!(
        r#"<span class="text-[10px] opacity-60 font-mono min-w-[4.5rem]" title="ActiveState">{}</span>"#,
        state_label
    ));

    // Stable control set: inactive/failed → start; anything running/transitional → stop+restart.
    // failed also keeps restart so a retry is one click.
    match active {
        "inactive" => {
            inner.push_str(&format!(
                r##"<button class="btn btn-ghost btn-xs h-5 min-h-0 px-1.5" hx-post="/unit/start/{u}" hx-swap="none" title="systemctl start">▶</button>"##,
                u = u
            ));
        }
        "failed" => {
            inner.push_str(&format!(
                r##"<button class="btn btn-ghost btn-xs h-5 min-h-0 px-1.5" hx-post="/unit/start/{u}" hx-swap="none" title="systemctl start">▶</button>"##,
                u = u
            ));
            inner.push_str(&format!(
                r##"<button class="btn btn-ghost btn-xs h-5 min-h-0 px-1.5" hx-post="/unit/restart/{u}" hx-swap="none" title="systemctl restart">⟳</button>"##,
                u = u
            ));
        }
        _ => {
            // active | activating | deactivating | reloading | unknown
            inner.push_str(&format!(
                r##"<button class="btn btn-ghost btn-xs h-5 min-h-0 px-1.5" hx-post="/unit/stop/{u}" hx-swap="none" title="systemctl stop">⏹</button>"##,
                u = u
            ));
            inner.push_str(&format!(
                r##"<button class="btn btn-ghost btn-xs h-5 min-h-0 px-1.5" hx-post="/unit/restart/{u}" hx-swap="none" title="systemctl restart">⟳</button>"##,
                u = u
            ));
        }
    }

    // logs always opens dialog (live via SSE)
    inner.push_str(&format!(
        r#"<button class="btn btn-ghost btn-xs h-5 min-h-0 px-1.5" onclick="openUnitLogs('{}')" title="open live logs dialog (infinitely scrollable)">logs</button>"#,
        u_js
    ));

    if is_container {
        inner.push_str(&format!(
            r##"<button class="btn btn-accent btn-xs h-5 min-h-0 px-1.5" hx-post="/container/update/{u}" hx-target="closest .unit-row .update-out-inline" hx-swap="innerHTML" title="docker pull (current running image) + restart">↻</button>"##,
            u = u
        ));
    }

    inner
}

/// Full unit-controls div (with id) for bootstrap GET.
pub fn render_unit_controls(unit: &str) -> RawHtml<String> {
    let active = unit_active_state(unit);
    let content = render_unit_controls_content_with_state(unit, &active);
    let u = escape_html(unit);
    RawHtml(format!(
        r#"<div id="unit-controls-{u}" class="unit-controls flex items-center gap-1 flex-shrink-0" data-active-state="{}">{content}</div>"#,
        escape_html(&active)
    ))
}

/// OOB fragment for htmx ws (and action HTTP responses).
pub fn unit_controls_oob_fragment(unit: &str) -> String {
    let active = unit_active_state(unit);
    unit_controls_oob_fragment_with_state(unit, &active)
}

pub fn unit_controls_oob_fragment_with_state(unit: &str, active: &str) -> String {
    format!(
        r#"<div id="unit-controls-{}" class="unit-controls flex items-center gap-1 flex-shrink-0" data-active-state="{}" hx-swap-oob="true">{}</div>"#,
        escape_html(unit),
        escape_html(active),
        render_unit_controls_content_with_state(unit, active)
    )
}

/// Broadcast an OOB swap fragment for a unit's controls to all connected WS clients.
pub fn broadcast_unit_update(unit: &str, config: &AppConfig) {
    let _ = config.unit_updates.send(unit_controls_oob_fragment(unit));
}

/// After a non-blocking systemctl action, ActiveState may lag for a few seconds.
/// Push a short burst of refreshes so the UI settles without waiting for the next
/// watcher tick alone (and even if the pane only did a one-shot HTTP OOB).
pub fn schedule_unit_refresh_burst(unit: String, config: Arc<AppConfig>) {
    if !unit_name_valid(&unit) {
        return;
    }
    tokio::spawn(async move {
        for delay_ms in [150_u64, 400, 900, 1800, 3500] {
            tokio::time::sleep(Duration::from_millis(delay_ms)).await;
            broadcast_unit_update(&unit, &config);
        }
    });
}

pub fn perform_unit_action(action: &str, unit: &str) {
    if !unit_name_valid(unit) {
        return;
    }
    let sudo = sudo_cmd();
    let _ = Command::new(&sudo)
        .args(["systemctl", action, unit, "--no-block", "--no-ask-password"])
        .status();
}

/// Best-effort parse of `id="unit-controls-…"` + `data-active-state="…"` from an OOB fragment.
pub fn extract_unit_state_from_oob(fragment: &str) -> Option<(String, String)> {
    let id_marker = r#"id="unit-controls-"#;
    let state_marker = r#"data-active-state=""#;
    let id_start = fragment.find(id_marker)? + id_marker.len();
    let id_end = fragment[id_start..].find('"')? + id_start;
    let unit = fragment[id_start..id_end].to_string();
    let state_start = fragment.find(state_marker)? + state_marker.len();
    let state_end = fragment[state_start..].find('"')? + state_start;
    let state = fragment[state_start..state_end].to_string();
    if unit_name_valid(&unit) {
        Some((unit, state))
    } else {
        None
    }
}
