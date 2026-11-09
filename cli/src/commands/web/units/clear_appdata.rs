//! Clear service appdata: stop units → rm -rf → restart previously-running units.
use std::path::{Component, Path};
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::super::locks::{lock_attr, LockMode, Scope};
use super::super::schema_cache::extract_pane;
use super::super::types::{AppConfig, OptionPaneContext};
use super::super::util::{
    escape_attr, escape_html, proc, status_err, status_ok, status_pulling, status_slot_oob,
};
use super::control::{
    broadcast_unit_update, schedule_unit_refresh_burst, systemctl, unit_name_valid, UnitAction,
};
use super::status::query_unit_states;

/// Mark service as clearing appdata. Returns false if already in flight.
pub fn try_begin_clear_appdata(config: &AppConfig, service: &str) -> bool {
    config.clear_appdata_in_flight.try_begin(service)
}

pub fn end_clear_appdata(config: &AppConfig, service: &str) {
    config.clear_appdata_in_flight.end(service);
}

const CLEAR_APPDATA_OUT_CLASSES: &str =
    "clear-appdata-out text-xs flex-shrink-0 max-w-full truncate";

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
            r#"<button id="clear-appdata-btn-{svc}" class="btn btn-error btn-outline btn-sm btn-disabled shrink-0" disabled title="{path}" hx-swap-oob="true"><span class="loading loading-spinner loading-xs"></span> Clearing…</button>"#,
            svc = svc,
            path = path,
        )
    } else {
        let lk = lock_attr(&[
            (Scope::System, LockMode::Shared),
            (Scope::service(service), LockMode::Exclusive),
        ]);
        format!(
            r##"<button id="clear-appdata-btn-{svc}" class="btn btn-error btn-outline btn-sm shrink-0" title="Delete {path}" hx-post="/service/{svc}/clear-appdata"{lk} hx-swap="none" hx-confirm="{confirm}" hx-disabled-elt="this" hx-swap-oob="true">Clear appdata</button>"##,
            svc = svc,
            path = path,
            confirm = confirm,
        )
    }
}

/// A service's pane plus its appdata path, both from trusted flake evaluation
/// (never the client). Errors when evaluation fails or the path is missing / unsafe.
pub async fn trusted_appdata(
    config: &AppConfig,
    service: &str,
) -> Result<(OptionPaneContext, String), String> {
    let pane = extract_pane(config, false, service).await;
    if let Some(err) = pane.eval_error.error.as_ref() {
        return Err(format!("eval failed: {err}"));
    }
    let appdata = match pane.appdata.as_ref() {
        Some(p) if !p.is_empty() => p.clone(),
        _ => return Err("no appdata path declared for this service".to_string()),
    };
    if !is_safe_appdata_path(&appdata, pane.appdata_root.as_deref()) {
        return Err(format!("refusing unsafe appdata path: {appdata}"));
    }
    Ok((pane, appdata))
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
pub async fn units_currently_running(units: &[String]) -> Vec<String> {
    let states = query_unit_states(units).await;
    units
        .iter()
        .filter(|u| states.get(*u).is_some_and(|s| unit_was_running(&s.active)))
        .cloned()
        .collect()
}

pub async fn wait_units_stopped(units: &[String], timeout: Duration) -> Result<(), String> {
    if units.is_empty() {
        return Ok(());
    }
    let deadline = Instant::now() + timeout;
    loop {
        let states = query_unit_states(units).await;
        let pending: Vec<String> = states
            .iter()
            .filter(|(_, s)| !unit_is_stopped(&s.active))
            .map(|(u, s)| format!("{u}={}", s.active))
            .collect();
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

pub async fn start_units_best_effort(units: &[String], config: &Arc<AppConfig>) {
    for u in units {
        if let Err(e) = systemctl(UnitAction::Start, u).await {
            eprintln!("web: clear-appdata start {}: {}", u, e);
        }
        broadcast_unit_update(u, config).await;
    }
}

async fn rm_rf_path(path: &str) -> Result<(), String> {
    let p = Path::new(path);
    if !p.exists() {
        return Ok(());
    }
    proc::run_ok(proc::sudo().args(["rm", "-rf", "--", path]), "rm -rf").await?;
    Ok(())
}

async fn finish_clear_appdata(
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
        broadcast_unit_update(u, config).await;
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
        let (inner, title) = status_pulling("stopping units…");
        push(inner, title);
    }

    for u in &units {
        if let Err(e) = systemctl(UnitAction::Stop, u).await {
            // Continue stopping others; wait_units_stopped surfaces stuck units.
            eprintln!("web: clear-appdata stop {}: {}", u, e);
        }
        broadcast_unit_update(u, &config).await;
    }

    if let Err(e) = wait_units_stopped(&units, Duration::from_secs(90)).await {
        let (inner, title) = status_err(&e);
        // Best-effort: restore only units we stopped that were previously running.
        if !to_restart.is_empty() {
            start_units_best_effort(&to_restart, &config).await;
        }
        finish_clear_appdata(&service, &appdata, &units, &config, inner, title).await;
        return;
    }

    {
        let (inner, title) = status_pulling("removing appdata…");
        push(inner, title);
    }

    if let Err(e) = rm_rf_path(&appdata).await {
        let (inner, title) = status_err(&e);
        // Best-effort restart so previously-running services are not left down.
        if !to_restart.is_empty() {
            start_units_best_effort(&to_restart, &config).await;
        }
        finish_clear_appdata(&service, &appdata, &units, &config, inner, title).await;
        return;
    }

    if !to_restart.is_empty() {
        let (inner, title) = status_pulling("starting units…");
        push(inner, title);
        start_units_best_effort(&to_restart, &config).await;
    }

    let (inner, title) = status_ok("appdata cleared");
    finish_clear_appdata(&service, &appdata, &units, &config, inner, title).await;
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
