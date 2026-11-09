//! ZFS snapshot routes: per-service appdata (option pane) and all Neo data (versioning tab).
use std::sync::Arc;

use rocket::form::Form;
use rocket::response::content::RawHtml;
use rocket::{get, post, FromForm, State};

use crate::commands::web::locks::{try_lock, Blocked};
use crate::commands::web::types::AppConfig;
use crate::commands::web::units::{begin_clear_appdata, pane_lock_spec, trusted_appdata};
use crate::commands::web::util::{
    escape_html, service_name_ok, status_err, status_ok, status_pulling,
};
use crate::commands::web::zfs::service as service_zfs;
use crate::commands::web::zfs::service::{
    locate_appdata, manual_snapshot_name, render_service_snapshots, run_restore, snapshot_out_oob,
    RestoreJob,
};
use crate::commands::web::zfs::{create_snapshot, data, snapshot_name_ok, SnapAction};
use crate::utils::locks::{LockMode, LockSpec, OpInfo, Scope};

fn out_err(service: &str, msg: &str) -> RawHtml<String> {
    let (inner, title) = status_err(msg);
    RawHtml(snapshot_out_oob(service, &inner, &title))
}

/// Snapshot card for the option pane (lazy-loaded).
#[get("/service/<service>/snapshots")]
pub async fn service_snapshots(service: &str, config: &State<Arc<AppConfig>>) -> RawHtml<String> {
    if !service_name_ok(service) {
        return RawHtml(String::new());
    }
    match trusted_appdata(config, service).await {
        Ok((_, appdata)) => {
            let busy = config.clear_appdata_in_flight.contains(service);
            RawHtml(render_service_snapshots(service, &appdata, busy, false).await)
        }
        Err(e) => RawHtml(format!(
            r#"<div id="snapshots-{}" class="text-[10px] text-error mb-2">{}</div>"#,
            escape_html(service),
            escape_html(&e)
        )),
    }
}

/// Manual snapshot of the dataset holding the service's appdata.
#[post("/service/<service>/snapshots/create")]
pub async fn service_snapshot_create(
    service: &str,
    config: &State<Arc<AppConfig>>,
) -> Result<RawHtml<String>, Blocked> {
    if !service_name_ok(service) {
        return Ok(RawHtml(String::new()));
    }
    let appdata = match trusted_appdata(config, service).await {
        Ok((_, a)) => a,
        Err(e) => return Ok(out_err(service, &e)),
    };
    let Some(loc) = locate_appdata(&appdata) else {
        return Ok(out_err(service, "appdata is not on a ZFS dataset"));
    };
    // Shared: never a snapshot of half-restored / half-cleared data.
    let _lock = try_lock(
        &LockSpec::new().with(Scope::service(service), LockMode::Shared),
        OpInfo::new("snapshot", format!("Snapshot of {service}")),
    )?;
    Ok(
        match create_snapshot(&loc.mount.dataset, &manual_snapshot_name(service)).await {
            Ok(full) => {
                let busy = config.clear_appdata_in_flight.contains(service);
                let card = render_service_snapshots(service, &appdata, busy, true).await;
                let (inner, title) = status_ok(&format!("created {full}"));
                RawHtml(format!(
                    "{card}{}",
                    snapshot_out_oob(service, &inner, &title)
                ))
            }
            Err(e) => out_err(service, &e),
        },
    )
}

/// Restore the service's appdata folder from a snapshot (background job, WS progress).
#[post("/service/<service>/snapshots/restore?<snap>")]
pub async fn service_snapshot_restore(
    service: &str,
    snap: &str,
    config: &State<Arc<AppConfig>>,
) -> Result<RawHtml<String>, Blocked> {
    if !service_name_ok(service) {
        return Ok(RawHtml(String::new()));
    }
    if !snapshot_name_ok(snap) {
        return Ok(out_err(service, "invalid snapshot name"));
    }
    let (pane, appdata) = match trusted_appdata(config, service).await {
        Ok(v) => v,
        Err(e) => return Ok(out_err(service, &e)),
    };
    // Exclusive on the service and all its units, system shared: no unit control,
    // pull, clear, other restore, activation, update or generation switch meanwhile.
    let lock = try_lock(
        &pane_lock_spec(service, &pane),
        OpInfo::new("restore", format!("Snapshot restore of {service}")),
    )?;
    begin_clear_appdata(config, service);

    let job = RestoreJob {
        service: service.to_string(),
        appdata: appdata.clone(),
        snap: snap.to_string(),
        units: pane.units.iter().map(|u| u.name.clone()).collect(),
        group_unit: pane.group_unit.clone(),
    };
    let card = render_service_snapshots(service, &appdata, true, true).await;
    let (inner, title) = status_pulling("starting restore…");
    let out = snapshot_out_oob(service, &inner, &title);
    let _ = config.unit_updates.send(card.clone());
    let _ = config.unit_updates.send(out.clone());

    let cfg = Arc::clone(config);
    tokio::spawn(async move {
        run_restore(job, lock, cfg).await;
    });
    Ok(RawHtml(format!("{card}{out}")))
}

/// Comment editor body (`comment` field; empty clears the comment).
#[derive(FromForm)]
pub struct CommentForm {
    comment: String,
}

/// Pin / unpin / delete / comment a snapshot of the service's appdata dataset:
/// the re-rendered card (OOB) plus the status slot.
async fn service_manage(
    service: &str,
    snap: &str,
    action: SnapAction,
    config: &State<Arc<AppConfig>>,
) -> RawHtml<String> {
    if !service_name_ok(service) {
        return RawHtml(String::new());
    }
    if !snapshot_name_ok(snap) {
        return out_err(service, "invalid snapshot name");
    }
    let appdata = match trusted_appdata(config, service).await {
        Ok((_, a)) => a,
        Err(e) => return out_err(service, &e),
    };
    // Shared, like create: refused while a restore or clear holds the service.
    let _lock = match try_lock(
        &LockSpec::new().with(Scope::service(service), LockMode::Shared),
        OpInfo::new("snapshot", format!("Snapshot change of {service}")),
    ) {
        Ok(l) => l,
        Err(Blocked(msg)) => return out_err(service, &msg),
    };
    let (inner, title) = match service_zfs::manage(&appdata, snap, action).await {
        Ok(msg) => status_ok(&msg),
        Err(e) => status_err(&e),
    };
    let busy = config.clear_appdata_in_flight.contains(service);
    let card = render_service_snapshots(service, &appdata, busy, true).await;
    RawHtml(format!(
        "{card}{}",
        snapshot_out_oob(service, &inner, &title)
    ))
}

#[post("/service/<service>/snapshots/delete?<snap>")]
pub async fn service_snapshot_delete(
    service: &str,
    snap: &str,
    config: &State<Arc<AppConfig>>,
) -> RawHtml<String> {
    service_manage(service, snap, SnapAction::Delete, config).await
}

#[post("/service/<service>/snapshots/pin?<snap>")]
pub async fn service_snapshot_pin(
    service: &str,
    snap: &str,
    config: &State<Arc<AppConfig>>,
) -> RawHtml<String> {
    service_manage(service, snap, SnapAction::Pin, config).await
}

#[post("/service/<service>/snapshots/unpin?<snap>")]
pub async fn service_snapshot_unpin(
    service: &str,
    snap: &str,
    config: &State<Arc<AppConfig>>,
) -> RawHtml<String> {
    service_manage(service, snap, SnapAction::Unpin, config).await
}

#[get("/service/<service>/snapshots/comment?<snap>")]
pub async fn service_snapshot_comment_form(
    service: &str,
    snap: &str,
    config: &State<Arc<AppConfig>>,
) -> RawHtml<String> {
    if !service_name_ok(service) || !snapshot_name_ok(snap) {
        return RawHtml(String::new());
    }
    match trusted_appdata(config, service).await {
        Ok((_, appdata)) => RawHtml(service_zfs::comment_editor(service, &appdata, snap).await),
        Err(_) => RawHtml(String::new()),
    }
}

#[post("/service/<service>/snapshots/comment?<snap>", data = "<form>")]
pub async fn service_snapshot_comment(
    service: &str,
    snap: &str,
    form: Form<CommentForm>,
    config: &State<Arc<AppConfig>>,
) -> RawHtml<String> {
    let comment = form.into_inner().comment;
    service_manage(service, snap, SnapAction::Comment(comment), config).await
}

/// Data snapshots card for the versioning tab (empty + hidden when unavailable).
#[get("/versioning/zfs")]
pub async fn versioning_zfs() -> RawHtml<String> {
    RawHtml(data::render_card(None).await)
}

#[post("/versioning/zfs/snapshot")]
pub async fn versioning_zfs_snapshot() -> RawHtml<String> {
    RawHtml(data::snapshot_now().await)
}

/// Data restore = reboot: nothing else may be running. The lock is kept until
/// the reboot (or briefly, when nothing was scheduled).
fn data_restore_lock() -> Result<crate::utils::locks::LockGuard, Blocked> {
    try_lock(
        &LockSpec::system_change(),
        OpInfo::new("data-restore", "Data restore (reboot)"),
    )
}

async fn hold_until_reboot(lock: crate::utils::locks::LockGuard) {
    if data::restore_scheduled().await {
        tokio::spawn(async move {
            let _lock = lock;
            tokio::time::sleep(std::time::Duration::from_secs(300)).await;
        });
    }
}

#[post("/versioning/zfs/restore?<ds>&<snap>&<gen>")]
pub async fn versioning_zfs_restore(
    ds: &str,
    snap: &str,
    gen: Option<u64>,
) -> Result<RawHtml<String>, Blocked> {
    let lock = data_restore_lock()?;
    let html = data::restore(ds, snap, gen).await;
    hold_until_reboot(lock).await;
    Ok(RawHtml(html))
}

#[post("/versioning/zfs/reboot")]
pub async fn versioning_zfs_reboot() -> Result<RawHtml<String>, Blocked> {
    let lock = data_restore_lock()?;
    let html = data::reboot_now().await;
    hold_until_reboot(lock).await;
    Ok(RawHtml(html))
}

#[post("/versioning/zfs/cancel")]
pub async fn versioning_zfs_cancel() -> RawHtml<String> {
    RawHtml(data::cancel().await)
}

#[post("/versioning/zfs/dismiss")]
pub async fn versioning_zfs_dismiss() -> RawHtml<String> {
    RawHtml(data::dismiss().await)
}

/// Data snapshot change; system shared so a pending data restore (system ex) blocks it.
async fn data_manage(ds: &str, snap: &str, action: SnapAction) -> String {
    let _lock = match try_lock(
        &LockSpec::system_shared(),
        OpInfo::new("snapshot", "Data snapshot change"),
    ) {
        Ok(l) => l,
        Err(Blocked(msg)) => return data::blocked_card(&msg).await,
    };
    data::manage(ds, snap, action).await
}

#[post("/versioning/zfs/delete?<ds>&<snap>")]
pub async fn versioning_zfs_delete(ds: &str, snap: &str) -> RawHtml<String> {
    RawHtml(data_manage(ds, snap, SnapAction::Delete).await)
}

#[post("/versioning/zfs/pin?<ds>&<snap>")]
pub async fn versioning_zfs_pin(ds: &str, snap: &str) -> RawHtml<String> {
    RawHtml(data_manage(ds, snap, SnapAction::Pin).await)
}

#[post("/versioning/zfs/unpin?<ds>&<snap>")]
pub async fn versioning_zfs_unpin(ds: &str, snap: &str) -> RawHtml<String> {
    RawHtml(data_manage(ds, snap, SnapAction::Unpin).await)
}

#[get("/versioning/zfs/comment?<ds>&<snap>")]
pub async fn versioning_zfs_comment_form(ds: &str, snap: &str) -> RawHtml<String> {
    RawHtml(data::comment_editor(ds, snap).await)
}

#[post("/versioning/zfs/comment?<ds>&<snap>", data = "<form>")]
pub async fn versioning_zfs_comment(
    ds: &str,
    snap: &str,
    form: Form<CommentForm>,
) -> RawHtml<String> {
    let comment = form.into_inner().comment;
    RawHtml(data_manage(ds, snap, SnapAction::Comment(comment)).await)
}
