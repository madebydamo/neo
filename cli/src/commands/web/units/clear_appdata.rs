//! Clear service appdata: stop units (see [`super::quiesce`]) → rm -rf → restart
//! previously-running units.
use std::path::{Component, Path};
use std::sync::Arc;
use std::time::Duration;

use super::super::locks::{lock_attr, LockMode, Scope};
use super::super::panes::extract_pane;
use super::super::types::{AppConfig, OptionPaneContext};
use super::super::util::{
    escape_attr, escape_html, proc, status_err, status_ok, status_pulling, status_slot_oob,
};
use super::control::{broadcast_unit_update, schedule_unit_refresh_burst, unit_name_valid};
use super::quiesce::quiesce;
use crate::utils::locks::{LockGuard, LockSpec};

/// Mark the service's data as being worked on (disables its buttons). The operation
/// lock is what keeps two such jobs apart.
pub fn begin_clear_appdata(config: &AppConfig, service: &str) {
    config.clear_appdata_in_flight.begin(service);
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

/// Lock for work on a service's data: system shared (no activation restarting
/// units meanwhile), the service and each of its units exclusive.
pub fn pane_lock_spec(service: &str, pane: &OptionPaneContext) -> LockSpec {
    LockSpec::service_data(
        service,
        pane.units
            .iter()
            .map(|u| u.name.as_str())
            .chain(pane.group_unit.as_deref()),
    )
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
    config.broadcast(frag);
    let btn = clear_appdata_btn_oob(service, appdata, false);
    config.broadcast(btn);
    end_clear_appdata(config, service);
    for u in units {
        broadcast_unit_update(u, config).await;
        schedule_unit_refresh_burst(u.clone(), Arc::clone(config));
    }
}

pub struct ClearJob {
    pub service: String,
    pub appdata: String,
    pub units: Vec<String>,
    pub group_unit: Option<String>,
}

/// Background: take the service down like a snapshot restore, rm -rf appdata, then
/// start only what was running before the clear. Holds and releases `lock`.
pub async fn run_clear_appdata(job: ClearJob, mut lock: LockGuard, config: Arc<AppConfig>) {
    let result = clear_inner(&job, &mut lock, &config).await;
    drop(lock);
    let (inner, title) = match result {
        Ok(()) => status_ok("appdata cleared"),
        Err(e) => status_err(&e),
    };
    let frag = clear_appdata_out_oob(&job.service, &inner, &title);
    config.broadcast(frag);
    let btn = clear_appdata_btn_oob(&job.service, &job.appdata, false);
    config.broadcast(btn);
    end_clear_appdata(&config, &job.service);
    for u in &job.units {
        broadcast_unit_update(u, &config).await;
        schedule_unit_refresh_burst(u.clone(), Arc::clone(&config));
    }
}

async fn clear_inner(
    job: &ClearJob,
    lock: &mut LockGuard,
    config: &Arc<AppConfig>,
) -> Result<(), String> {
    let push = |msg: &str| {
        let (inner, title) = status_pulling(msg);
        let frag = clear_appdata_out_oob(&job.service, &inner, &title);
        config.broadcast(frag);
    };

    let units: Vec<String> = job
        .units
        .iter()
        .filter(|u| unit_name_valid(u))
        .cloned()
        .collect();

    push("stopping units…");
    let quiesced = quiesce(
        &units,
        job.group_unit.as_deref(),
        Duration::from_secs(90),
        lock,
        config,
    )
    .await?;

    push("removing appdata…");
    let removed = rm_rf_path(&job.appdata).await;
    if removed.is_ok() && quiesced.will_start() {
        push("starting units…");
    }
    // Also after a failed delete, so previously-running services are not left down.
    quiesced.restart(lock, config).await;
    removed
}

#[cfg(test)]
mod tests {
    use super::is_safe_appdata_path;

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
}
