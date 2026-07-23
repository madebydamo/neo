//! Systemd unit control: active state, start/stop/restart, control OOB fragments.
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;

use tokio::process::Command as AsyncCommand;

use super::super::types::AppConfig;
use super::super::util::{escape_attr, escape_html, status_slot_oob, sudo_cmd};

pub use super::super::util::unit_name_valid;

/// systemctl action allowed from the web UI.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnitAction {
    Start,
    Stop,
    Restart,
}

impl UnitAction {
    pub fn as_str(self) -> &'static str {
        match self {
            UnitAction::Start => "start",
            UnitAction::Stop => "stop",
            UnitAction::Restart => "restart",
        }
    }
}

pub fn is_pull_in_flight(config: &AppConfig, unit: &str) -> bool {
    config.pulls_in_flight.contains(unit)
}

/// Mark unit as pulling. Returns false if a pull is already in flight for this unit.
pub fn try_begin_pull(config: &AppConfig, unit: &str) -> bool {
    config.pulls_in_flight.try_begin(unit)
}

pub fn end_pull(config: &AppConfig, unit: &str) {
    config.pulls_in_flight.end(unit);
}

/// Normalize systemctl is-active stdout into a state string.
fn parse_active_state_stdout(stdout: &[u8]) -> String {
    let s = String::from_utf8_lossy(stdout).trim().to_string();
    if s.is_empty() {
        "unknown".into()
    } else {
        s
    }
}

/// Query systemctl is-active for a unit (sync; used when building OOB control fragments).
pub fn unit_active_state(unit: &str) -> String {
    let sudo = sudo_cmd();
    Command::new(&sudo)
        .args(["systemctl", "is-active", unit])
        .output()
        .map(|o| parse_active_state_stdout(&o.stdout))
        .unwrap_or_else(|_| "unknown".into())
}

pub async fn unit_active_state_async(unit: &str) -> String {
    let sudo = sudo_cmd();
    match AsyncCommand::new(&sudo)
        .args(["systemctl", "is-active", unit])
        .output()
        .await
    {
        Ok(o) => parse_active_state_stdout(&o.stdout),
        Err(_) => "unknown".into(),
    }
}

const UPDATE_OUT_CLASSES: &str =
    "update-out update-out-inline text-[10px] ml-1 flex-shrink-0 max-w-[16rem] truncate";

/// OOB fragment for the per-row pull status slot (`#update-out-{unit}`).
pub fn update_out_oob(unit: &str, inner: &str, title: &str) -> String {
    status_slot_oob("update-out", unit, UPDATE_OUT_CLASSES, inner, title)
}

pub fn broadcast_update_out(unit: &str, inner: &str, title: &str, config: &AppConfig) {
    let _ = config.unit_updates.send(update_out_oob(unit, inner, title));
}

/// Build the inner content (dot + state + buttons) for a unit controls area.
/// Used for OOB WS pushes and composed into full divs.
///
/// Buttons stay stable across transitional states so restart/stop never "vanish"
/// while systemctl --no-block is still settling (the live WS watcher re-renders
/// as soon as ActiveState changes).
pub fn render_unit_controls_content_with_state(unit: &str, active: &str, pulling: bool) -> String {
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
        dot_cls,
        escape_attr(unit)
    ));
    inner.push_str(&format!(
        r#"<span class="text-[10px] opacity-60 font-mono min-w-[4.5rem]" title="ActiveState">{}</span>"#,
        state_label
    ));

    let start_btn = format!(
        r##"<button class="btn btn-ghost btn-xs h-5 min-h-0 px-1.5" hx-post="/unit/start/{u}" hx-swap="none" title="systemctl start">▶</button>"##,
        u = u
    );
    let restart_btn = format!(
        r##"<button class="btn btn-ghost btn-xs h-5 min-h-0 px-1.5" hx-post="/unit/restart/{u}" hx-swap="none" title="systemctl restart">⟳</button>"##,
        u = u
    );
    let stop_btn = format!(
        r##"<button class="btn btn-ghost btn-xs h-5 min-h-0 px-1.5" hx-post="/unit/stop/{u}" hx-swap="none" title="systemctl stop">⏹</button>"##,
        u = u
    );

    // Stable control set: inactive/failed → start; anything running/transitional → stop+restart.
    // failed also keeps restart so a retry is one click.
    match active {
        "inactive" => {
            inner.push_str(&start_btn);
        }
        "failed" => {
            inner.push_str(&start_btn);
            inner.push_str(&restart_btn);
        }
        _ => {
            // active | activating | deactivating | reloading | unknown
            inner.push_str(&stop_btn);
            inner.push_str(&restart_btn);
        }
    }

    // logs always opens dialog (live via SSE)
    inner.push_str(&format!(
        r#"<button class="btn btn-ghost btn-xs h-5 min-h-0 px-1.5" onclick="openUnitLogs('{}')" title="open live logs dialog (infinitely scrollable)">logs</button>"#,
        u_js
    ));

    if is_container {
        if pulling {
            inner.push_str(
                r#"<button class="btn btn-accent btn-xs h-5 min-h-0 px-1.5 btn-disabled" disabled title="docker pull in progress"><span class="loading loading-spinner loading-xs"></span></button>"#,
            );
        } else {
            // hx-swap=none: immediate OOB (update-out + controls) comes from the response;
            // long pull progress is pushed over /ws/status.
            inner.push_str(&format!(
                r##"<button class="btn btn-accent btn-xs h-5 min-h-0 px-1.5" hx-post="/container/update/{u}" hx-swap="none" hx-disabled-elt="this" title="docker pull (current running image) + restart">↻</button>"##,
                u = u
            ));
        }
    }

    inner
}

/// OOB fragment for htmx ws (and action HTTP responses).
pub fn unit_controls_oob_fragment(unit: &str, config: &AppConfig) -> String {
    let active = unit_active_state(unit);
    let pulling = is_pull_in_flight(config, unit);
    unit_controls_oob_fragment_with_state(unit, &active, pulling)
}

pub fn unit_controls_oob_fragment_with_state(unit: &str, active: &str, pulling: bool) -> String {
    format!(
        r#"<div id="unit-controls-{}" class="unit-controls flex items-center gap-1 flex-shrink-0" data-active-state="{}" hx-swap-oob="true">{}</div>"#,
        escape_html(unit),
        escape_attr(active),
        render_unit_controls_content_with_state(unit, active, pulling)
    )
}

/// Broadcast an OOB swap fragment for a unit's controls to all connected WS clients.
pub fn broadcast_unit_update(unit: &str, config: &AppConfig) {
    let _ = config
        .unit_updates
        .send(unit_controls_oob_fragment(unit, config));
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

pub fn perform_unit_action(action: UnitAction, unit: &str) {
    if !unit_name_valid(unit) {
        return;
    }
    let sudo = sudo_cmd();
    let _ = Command::new(&sudo)
        .args([
            "systemctl",
            action.as_str(),
            unit,
            "--no-block",
            "--no-ask-password",
        ])
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

/// Normalize path param to (systemd unit name, bare docker container name).
pub fn normalize_container_unit(container: &str) -> (String, String) {
    if container.starts_with("docker-") {
        let bare = container[7..].to_string();
        (container.to_string(), bare)
    } else {
        (format!("docker-{}", container), container.to_string())
    }
}
