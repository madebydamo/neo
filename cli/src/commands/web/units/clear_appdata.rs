//! Clear service appdata: stop units → rm -rf → restart previously-running units.
use std::path::{Component, Path};
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::process::Command as AsyncCommand;

use super::super::types::AppConfig;
use super::super::util::{
    escape_attr, escape_html, status_err, status_ok, status_pulling, status_slot_oob, sudo_cmd,
};
use super::control::{
    broadcast_unit_update, schedule_unit_refresh_burst, unit_active_state_async, unit_name_valid,
};

pub fn is_clear_appdata_in_flight(config: &AppConfig, service: &str) -> bool {
    config.clear_appdata_in_flight.contains(service)
}

/// Mark service as clearing appdata. Returns false if already in flight.
pub fn try_begin_clear_appdata(config: &AppConfig, service: &str) -> bool {
    config.clear_appdata_in_flight.try_begin(service)
}

pub fn end_clear_appdata(config: &AppConfig, service: &str) {
    config.clear_appdata_in_flight.end(service);
}

const CLEAR_APPDATA_OUT_CLASSES: &str =
    "clear-appdata-out text-[10px] ml-1 flex-shrink-0 max-w-[18rem] truncate";

/// OOB fragment for the per-service clear-appdata status slot.
pub fn clear_appdata_out_oob(service: &str, inner: &str, title: &str) -> String {
    status_slot_oob(
        "clear-appdata-out",
        service,
        CLEAR_APPDATA_OUT_CLASSES,
        inner,
        title,
    )
}

/// OOB fragment for the Clear appdata button (disabled while in flight).
pub fn clear_appdata_btn_oob(service: &str, appdata: &str, busy: bool) -> String {
    let svc = escape_html(service);
    let path = escape_attr(appdata);
    let confirm = escape_attr(&format!(
        "Stop all related units, permanently delete {} and all contents, then restart only units that were running? This cannot be undone.",
        appdata
    ));
    if busy {
        format!(
            r#"<button id="clear-appdata-btn-{svc}" class="btn btn-error btn-xs btn-disabled" disabled title="{path}" hx-swap-oob="true"><span class="loading loading-spinner loading-xs"></span> Clearing…</button>"#,
            svc = svc,
            path = path,
        )
    } else {
        format!(
            r##"<button id="clear-appdata-btn-{svc}" class="btn btn-error btn-xs" title="Delete {path}" hx-post="/service/{svc}/clear-appdata" hx-swap="none" hx-confirm="{confirm}" hx-disabled-elt="this" hx-swap-oob="true">Clear appdata</button>"##,
            svc = svc,
            path = path,
            confirm = confirm,
        )
    }
}

fn clear_status_pulling(msg: &str) -> (String, String) {
    status_pulling(msg)
}
fn clear_status_ok(msg: &str) -> (String, String) {
    status_ok(msg)
}
fn clear_status_err(msg: &str) -> (String, String) {
    status_err(msg)
}

/// Whether `path` is safe to recursively delete as service appdata.
/// Path must come from trusted nix evaluation; this is defense-in-depth.
pub fn is_safe_appdata_path(path: &str, appdata_root: Option<&str>) -> bool {
    if path.is_empty() || path.contains('\0') || !path.starts_with('/') {
        return false;
    }
    let p = Path::new(path);
    if p.components()
        .any(|c| matches!(c, Component::ParentDir | Component::CurDir))
    {
        return false;
    }
    // At least three normal components: /var/lib/example or /var/neo/DATA/AppData/foo
    let depth = p
        .components()
        .filter(|c| matches!(c, Component::Normal(_)))
        .count();
    if depth < 3 {
        return false;
    }
    const FORBIDDEN: &[&str] = &[
        "/", "/bin", "/boot", "/dev", "/etc", "/home", "/lib", "/nix", "/proc", "/root", "/run",
        "/sys", "/tmp", "/usr", "/var", "/var/lib", "/var/log", "/var/neo",
    ];
    if FORBIDDEN.contains(&path) {
        return false;
    }
    if let Some(root) = appdata_root {
        if path == root {
            // Never wipe the entire AppData volume.
            return false;
        }
        // Preferred: strict child of the AppData volume.
        let prefix = if root.ends_with('/') {
            root.to_string()
        } else {
            format!("{}/", root)
        };
        if path.starts_with(&prefix) {
            return true;
        }
    }
    // Paths outside the volume (e.g. /var/lib/example) still allowed when
    // declared by the service option and deep enough (checked above).
    true
}

fn unit_is_stopped(state: &str) -> bool {
    matches!(state, "inactive" | "failed" | "dead" | "not-found")
}

/// Whether a unit should be restarted after clear-appdata (was up before stop).
/// Inactive / failed / not-found units are left stopped after the delete.
fn unit_was_running(state: &str) -> bool {
    matches!(state, "active" | "activating" | "reloading")
}

/// Snapshot units that are currently running (should be restarted after clear).
async fn units_currently_running(units: &[String]) -> Vec<String> {
    let mut running = Vec::new();
    for u in units {
        let state = unit_active_state_async(u).await;
        if unit_was_running(&state) {
            running.push(u.clone());
        }
    }
    running
}

async fn wait_units_stopped(units: &[String], timeout: Duration) -> Result<(), String> {
    if units.is_empty() {
        return Ok(());
    }
    let deadline = Instant::now() + timeout;
    loop {
        let mut pending = Vec::new();
        for u in units {
            let state = unit_active_state_async(u).await;
            if !unit_is_stopped(&state) {
                pending.push(format!("{}={}", u, state));
            }
        }
        if pending.is_empty() {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "timeout waiting for units to stop ({})",
                pending.join(", ")
            ));
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

async fn start_units_best_effort(units: &[String], config: &Arc<AppConfig>) {
    for u in units {
        if let Err(e) = systemctl_action_blocking("start", u).await {
            eprintln!("web: clear-appdata start {}: {}", u, e);
        }
        broadcast_unit_update(u, config);
    }
}

async fn systemctl_action_blocking(action: &str, unit: &str) -> Result<(), String> {
    let sudo = sudo_cmd();
    let out = AsyncCommand::new(&sudo)
        .args(["systemctl", action, unit, "--no-ask-password"])
        .output()
        .await
        .map_err(|e| format!("systemctl {} {}: {}", action, unit, e))?;
    if out.status.success() {
        Ok(())
    } else {
        let err = String::from_utf8_lossy(&out.stderr);
        // stop of an already-inactive unit is fine
        if action == "stop" && err.contains("not loaded") {
            return Ok(());
        }
        Err(format!(
            "systemctl {} {} failed: {}",
            action,
            unit,
            err.trim()
        ))
    }
}

async fn rm_rf_path(path: &str) -> Result<(), String> {
    let p = Path::new(path);
    if !p.exists() {
        return Ok(());
    }
    let sudo = sudo_cmd();
    let out = AsyncCommand::new(&sudo)
        .args(["rm", "-rf", "--", path])
        .output()
        .await
        .map_err(|e| format!("rm -rf: {}", e))?;
    if out.status.success() {
        Ok(())
    } else {
        let err = String::from_utf8_lossy(&out.stderr);
        Err(format!("rm -rf failed: {}", err.trim()))
    }
}

fn finish_clear_appdata(
    service: &str,
    appdata: &str,
    units: &[String],
    config: &Arc<AppConfig>,
    inner: String,
    title: String,
) {
    let frag = clear_appdata_out_oob(service, &inner, &title);
    let _ = config.unit_updates.send(frag);
    let btn = clear_appdata_btn_oob(service, appdata, false);
    let _ = config.unit_updates.send(btn);
    end_clear_appdata(config, service);
    for u in units {
        broadcast_unit_update(u, config);
        schedule_unit_refresh_burst(u.clone(), Arc::clone(config));
    }
}

/// Background: stop all service units, wait until stopped, rm -rf appdata,
/// then start only units that were running before the clear.
pub async fn run_clear_appdata(
    service: String,
    appdata: String,
    units: Vec<String>,
    config: Arc<AppConfig>,
) {
    let units: Vec<String> = units.into_iter().filter(|u| unit_name_valid(u)).collect();

    let push = |inner: String, title: String| {
        let frag = clear_appdata_out_oob(&service, &inner, &title);
        let _ = config.unit_updates.send(frag);
    };

    // Only restart units that were up; stopped / missing units stay down.
    let to_restart = units_currently_running(&units).await;

    {
        let (inner, title) = clear_status_pulling("stopping units…");
        push(inner, title);
    }

    for u in &units {
        if let Err(e) = systemctl_action_blocking("stop", u).await {
            // Continue stopping others; wait_units_stopped surfaces stuck units.
            eprintln!("web: clear-appdata stop {}: {}", u, e);
        }
        broadcast_unit_update(u, &config);
    }

    if let Err(e) = wait_units_stopped(&units, Duration::from_secs(90)).await {
        let (inner, title) = clear_status_err(&e);
        // Best-effort: restore only units we stopped that were previously running.
        if !to_restart.is_empty() {
            start_units_best_effort(&to_restart, &config).await;
        }
        finish_clear_appdata(&service, &appdata, &units, &config, inner, title);
        return;
    }

    {
        let (inner, title) = clear_status_pulling("removing appdata…");
        push(inner, title);
    }

    if let Err(e) = rm_rf_path(&appdata).await {
        let (inner, title) = clear_status_err(&e);
        // Best-effort restart so previously-running services are not left down.
        if !to_restart.is_empty() {
            start_units_best_effort(&to_restart, &config).await;
        }
        finish_clear_appdata(&service, &appdata, &units, &config, inner, title);
        return;
    }

    if !to_restart.is_empty() {
        let (inner, title) = clear_status_pulling("starting units…");
        push(inner, title);
        start_units_best_effort(&to_restart, &config).await;
    }

    let (inner, title) = clear_status_ok("appdata cleared");
    finish_clear_appdata(&service, &appdata, &units, &config, inner, title);
}

#[cfg(test)]
mod tests {
    use super::{is_safe_appdata_path, unit_was_running};

    #[test]
    fn appdata_path_under_volume() {
        let root = "/var/neo/DATA/AppData";
        assert!(is_safe_appdata_path(
            "/var/neo/DATA/AppData/vaultwarden",
            Some(root)
        ));
        assert!(!is_safe_appdata_path(root, Some(root)));
        assert!(!is_safe_appdata_path(
            "/var/neo/DATA/AppData/../etc",
            Some(root)
        ));
        assert!(!is_safe_appdata_path("/", Some(root)));
        assert!(!is_safe_appdata_path("/var/lib", Some(root)));
    }

    #[test]
    fn appdata_path_outside_volume_deep() {
        assert!(is_safe_appdata_path("/var/lib/example", None));
        assert!(!is_safe_appdata_path("/var/lib", None));
    }

    #[test]
    fn unit_was_running_only_up_states() {
        assert!(unit_was_running("active"));
        assert!(unit_was_running("activating"));
        assert!(unit_was_running("reloading"));
        assert!(!unit_was_running("inactive"));
        assert!(!unit_was_running("failed"));
        assert!(!unit_was_running("dead"));
        assert!(!unit_was_running("not-found"));
        assert!(!unit_was_running("deactivating"));
        assert!(!unit_was_running("unknown"));
    }
}
