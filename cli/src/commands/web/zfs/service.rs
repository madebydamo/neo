//! Per-service appdata snapshots (option pane).
//!
//! The appdata folder usually shares a dataset with every other service
//! (`zroot/neo` → /var/neo), so `zfs rollback` is not an option: it would
//! roll back all services and destroy newer snapshots. Restore instead:
//! pre-restore snapshot → stop `neo-<svc>.target` + units → rsync the folder
//! back from `.zfs/snapshot/<snap>/<rel>` → start what was running.
//!
//! The job holds the service's operation lock (system shared, service + every
//! unit exclusive) and a **start guard** on its units while they are down, so
//! neither the UI, the CLI, a timer, a dependency, an activation, nor a manual
//! `systemctl start` can bring the service up on half-restored data.
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use super::super::locks::{lock_attr, LockMode, Scope};
use super::super::types::AppConfig;
use super::super::units::{
    broadcast_unit_update, end_clear_appdata, quiesce, schedule_unit_refresh_burst,
};
use super::super::util::{
    escape_attr, escape_html, proc, rsync_bin, status_err, status_ok, status_pulling,
    status_slot_oob,
};
use super::render::{self, ManageCtx, BTN_SECONDARY};
use super::{
    create_snapshot, dataset_for_path, format_epoch_utc, list_snapshots, now_epoch, now_ts,
    snapshot_action, snapshot_has_path, zfs_mounts, SnapAction, Snapshot, ZfsMount,
};
use crate::utils::locks::LockGuard;

const OUT_CLASSES: &str = "snapshot-out text-xs max-w-full truncate empty:hidden";

/// Most rows shown in the pane (newest first).
const MAX_ROWS: usize = 60;

pub fn snapshot_out_oob(service: &str, inner: &str, title: &str) -> String {
    status_slot_oob("snapshot-out", service, OUT_CLASSES, inner, title)
}

/// Where a service's appdata lives on ZFS.
pub struct AppdataLocation {
    pub mount: ZfsMount,
    /// Path of the appdata folder relative to the dataset mountpoint.
    pub rel: String,
}

pub fn locate_appdata(appdata: &str) -> Option<AppdataLocation> {
    let (mount, rel) = dataset_for_path(&zfs_mounts(), appdata)?;
    Some(AppdataLocation { mount, rel })
}

/// Snapshot name for a manual per-service snapshot.
pub fn manual_snapshot_name(service: &str) -> String {
    format!("neo-snap-{}-{}", service, now_ts())
}

fn prerestore_snapshot_name(service: &str) -> String {
    format!("neo-prerestore-{}-{}", service, now_ts())
}

/// The whole snapshots card for a service. `oob` wraps it for a WS push.
/// Same layout and rows as the versioning tab's data snapshots ([`super::render`]).
pub async fn render_service_snapshots(
    service: &str,
    appdata: &str,
    busy: bool,
    oob: bool,
) -> String {
    let svc = escape_html(service);
    let oob_attr = if oob { r#" hx-swap-oob="true""# } else { "" };
    let open = format!(r#"<div id="snapshots-{svc}" class="space-y-3"{oob_attr}>"#);
    let unavailable = |msg: &str| {
        format!(
            r#"{open}<div class="rounded-box border border-base-300 bg-base-100 px-4 py-6 text-center text-xs text-base-content/50">{}</div></div>"#,
            escape_html(msg)
        )
    };

    let Some(loc) = locate_appdata(appdata) else {
        return unavailable(
            "Snapshots are unavailable: this service's app data is not on a ZFS dataset.",
        );
    };
    if loc.rel.is_empty() {
        return unavailable("This service's app data is a whole dataset — use Data snapshots in the versioning tab.");
    }

    let snaps = list_snapshots(&loc.mount.dataset, false).await;
    let ds = escape_html(&loc.mount.dataset);
    let (dis_cls, dis_attr) = if busy {
        (" btn-disabled", " disabled")
    } else {
        ("", "")
    };

    let snap_lock = lock_attr(&[(Scope::service(service), LockMode::Shared)]);
    let mut html = open;
    html.push_str(&format!(
        r##"<div class="flex flex-wrap items-center gap-2">
  <p class="text-xs sm:text-sm text-base-content/60 min-w-0 flex-1">Snapshots of this service's app data <span class="font-mono break-all">{appdata_html}</span>.</p>
  <span class="flex gap-1.5">
    <button type="button" class="{BTN_SECONDARY}" hx-get="/service/{svc}/snapshots" hx-target="#snapshots-{svc}" hx-swap="outerHTML">Refresh</button>
    <button type="button" class="btn btn-sm btn-primary{dis_cls}"{dis_attr} hx-post="/service/{svc}/snapshots/create"{snap_lock} hx-swap="none" hx-disabled-elt="this" title="zfs snapshot {ds}@neo-snap-{svc}-…">Snapshot now</button>
  </span>
</div>
<div id="snapshot-out-{svc}" class="{OUT_CLASSES}" title=""></div>
{how}"##,
        appdata_html = escape_html(appdata),
        how = render::how_it_works(&[
            "The service is <b>stopped</b> while its app data folder is copied back from the snapshot, then started again.",
            "Only this service's folder is replaced; other services and the activated system are not touched.",
            "A <span class=\"badge badge-warning badge-xs\">pre-restore</span> snapshot is taken first, so a restore can be undone from this list.",
        ]),
    ));

    match snaps {
        Err(e) => {
            html.push_str(&format!(
                r#"<div role="alert" class="alert alert-soft alert-error py-2 px-3 text-xs">{}</div>"#,
                escape_html(&e)
            ));
        }
        Ok(list) => {
            let now = now_epoch();
            let rows: Vec<String> = list
                .iter()
                .rev()
                .take(MAX_ROWS)
                .map(|s| snapshot_row(service, appdata, &loc, s, now, busy))
                .collect();
            html.push_str(&render::snapshot_list(
                "Snapshots",
                &loc.mount.dataset,
                true,
                list.len(),
                &rows,
                MAX_ROWS,
            ));
        }
    }
    html.push_str("</div>");
    html
}

fn snapshot_row(
    service: &str,
    appdata: &str,
    loc: &AppdataLocation,
    s: &Snapshot,
    now: i64,
    busy: bool,
) -> String {
    let when = format_epoch_utc(s.creation);
    let confirm = format!(
        "Restore {service} appdata to snapshot {name} ({when})?\n\n\
         • All units of {service} are stopped.\n\
         • {appdata} is REPLACED by the snapshot copy: changes and files newer than the snapshot are lost.\n\
         • A pre-restore snapshot of {ds} is taken first, so this can be undone from this list.\n\
         • Other services and the activated system are not touched.",
        name = s.name,
        ds = loc.mount.dataset,
    );
    let action = if busy {
        r#"<button type="button" class="btn btn-sm btn-disabled" disabled title="Another app data operation is running">Restore</button>"#
            .to_string()
    } else {
        format!(
            r#"<button type="button" class="{BTN_SECONDARY}" hx-post="/service/{svc}/snapshots/restore?snap={snap_q}"{lk} hx-swap="none" hx-disabled-elt="this" hx-confirm="{confirm}" title="Replace the app data folder with this snapshot">Restore</button>"#,
            svc = escape_attr(service),
            snap_q = escape_attr(&render::urlencode(&s.name)),
            confirm = escape_attr(&confirm),
            lk = lock_attr(&[
                (Scope::System, LockMode::Shared),
                (Scope::service(service), LockMode::Exclusive),
            ]),
        )
    };
    let manual_mine = s.name.starts_with(&format!("neo-snap-{service}-"))
        || s.name.starts_with(&format!("neo-prerestore-{service}-"));
    let base = format!("/service/{}/snapshots", escape_attr(service));
    let manage = render::manage_actions(
        s,
        &render::snap_query(None, &s.name),
        &ManageCtx {
            base: &base,
            hx: MANAGE_HX,
            busy,
        },
    );
    render::snapshot_row(s, now, "", &format!("{action}{manage}"), manual_mine)
}

/// Row actions answer with the card (OOB) and the status slot.
const MANAGE_HX: &str = r#"hx-swap="none""#;

/// Inline comment editor for one snapshot of the service's dataset.
pub async fn comment_editor(service: &str, appdata: &str, snap: &str) -> String {
    let Some(loc) = locate_appdata(appdata) else {
        return String::new();
    };
    let Ok(list) = list_snapshots(&loc.mount.dataset, false).await else {
        return String::new();
    };
    let base = format!("/service/{}/snapshots", escape_attr(service));
    list.iter()
        .find(|s| s.name == snap)
        .map(|s| {
            render::comment_form(
                s,
                &render::snap_query(None, snap),
                &ManageCtx {
                    base: &base,
                    hx: MANAGE_HX,
                    busy: false,
                },
            )
        })
        .unwrap_or_default()
}

/// Delete / pin / unpin / comment a snapshot of the dataset holding the
/// service's appdata (never a user-supplied dataset).
pub async fn manage(appdata: &str, snap: &str, action: SnapAction) -> Result<String, String> {
    let loc = locate_appdata(appdata).ok_or("appdata is not on a ZFS dataset")?;
    snapshot_action(&loc.mount.dataset, snap, action).await
}

async fn rsync_restore(src: &Path, dst: &str) -> Result<(), String> {
    let mut src_s = src.to_string_lossy().into_owned();
    src_s.push('/');
    let mut dst_s = dst.trim_end_matches('/').to_string();
    dst_s.push('/');
    let out = proc::sudo()
        .args([
            &rsync_bin(),
            "-aHX",
            "--delete",
            "--numeric-ids",
            "--",
            &src_s,
            &dst_s,
        ])
        .output()
        .await
        .map_err(|e| format!("rsync: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        let err = String::from_utf8_lossy(&out.stderr);
        let last = err
            .lines()
            .rev()
            .find(|l| !l.trim().is_empty())
            .unwrap_or("");
        Err(format!("rsync failed: {}", last.trim()))
    }
}

pub struct RestoreJob {
    pub service: String,
    pub appdata: String,
    pub snap: String,
    pub units: Vec<String>,
    pub group_unit: Option<String>,
}

/// Background restore. The caller holds the service's in-flight slot
/// (shared with clear-appdata) and its operation lock; this releases both.
pub async fn run_restore(job: RestoreJob, mut lock: LockGuard, config: Arc<AppConfig>) {
    let result = restore_inner(&job, &mut lock, &config).await;
    // Guards are gone on every path of restore_inner; drop them again to be sure.
    lock.release_unit_guards();
    drop(lock);
    let (inner, title) = match &result {
        Ok(msg) => status_ok(msg),
        Err(e) => status_err(e),
    };
    let _ = config
        .unit_updates
        .send(snapshot_out_oob(&job.service, &inner, &title));
    end_clear_appdata(&config, &job.service);
    let card = render_service_snapshots(&job.service, &job.appdata, false, true).await;
    let _ = config.unit_updates.send(card);
    // The card re-render clears the status slot; push the result again after it.
    let _ = config
        .unit_updates
        .send(snapshot_out_oob(&job.service, &inner, &title));
    for u in &job.units {
        broadcast_unit_update(u, &config).await;
        schedule_unit_refresh_burst(u.clone(), Arc::clone(&config));
    }
}

async fn restore_inner(
    job: &RestoreJob,
    lock: &mut LockGuard,
    config: &Arc<AppConfig>,
) -> Result<String, String> {
    let push = |msg: &str| {
        let (inner, title) = status_pulling(msg);
        let _ = config
            .unit_updates
            .send(snapshot_out_oob(&job.service, &inner, &title));
    };

    let loc = locate_appdata(&job.appdata).ok_or("appdata is not on a ZFS dataset")?;
    if loc.rel.is_empty() {
        return Err("appdata is a whole dataset; refusing".to_string());
    }
    let snaps = list_snapshots(&loc.mount.dataset, false).await?;
    if !snaps.iter().any(|s| s.name == job.snap) {
        return Err(format!("snapshot {} not found", job.snap));
    }
    if snapshot_has_path(&loc.mount, &job.snap, &loc.rel) == Some(false) {
        return Err(format!(
            "{} did not exist yet in {} (use Clear appdata instead)",
            job.appdata, job.snap
        ));
    }
    let src = Path::new(&loc.mount.mountpoint)
        .join(".zfs/snapshot")
        .join(&job.snap)
        .join(&loc.rel);

    push("taking pre-restore snapshot…");
    let safety =
        create_snapshot(&loc.mount.dataset, &prerestore_snapshot_name(&job.service)).await?;

    push("stopping service…");
    let quiesced = quiesce(
        &job.units,
        job.group_unit.as_deref(),
        Duration::from_secs(120),
        lock,
        config,
    )
    .await?;

    push(&format!("restoring files from {}…", job.snap));
    if let Err(e) = rsync_restore(&src, &job.appdata).await {
        quiesced.restart(lock, config).await;
        return Err(format!(
            "{e} — appdata may be partial; restore {safety} to undo"
        ));
    }

    if quiesced.will_start() {
        push("starting service…");
    }
    quiesced.restart(lock, config).await;

    Ok(format!(
        "restored {} (undo: pre-restore snapshot)",
        job.snap
    ))
}
