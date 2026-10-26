//! Restore of all Neo data (versioning tab).
//!
//! Only the dataset behind `neo.core.volumes.root` (`zroot/neo`) is restored;
//! the system (root, /nix) is never snapshotted. The swap runs at the next
//! boot in the initrd (`nix/modules/disko/zfs-restore.sh`), because every
//! service, the config repo, and this web UI live on that dataset. The web UI
//! schedules it with `neo:restore` on the pool and reboots.
//!
//! Optionally the boot generation is switched to the one that was running when
//! the snapshot was taken ([`GenerationTimeline`]), so restored settings and
//! system match.
use super::super::power::{self, PowerAction};
use super::super::util::{escape_attr, escape_html, sudo_cmd};
use super::render::{self, ManageCtx, BTN_SECONDARY};
use super::{
    create_snapshot, dataset_name_ok, format_epoch_utc, get_property, inherit_property,
    list_filesystems, list_snapshots, now_epoch, now_ts, set_property, snapshot_action,
    snapshot_name_ok, snapshot_ref, SnapAction, Snapshot,
};
use crate::utils::{
    current_generation_number, running_generation_number, switch_system_generation, GenerationMode,
    GenerationTimeline,
};

const PROP_RESTORE: &str = "neo:restore";
const PROP_RESULT: &str = "neo:restore-result";
const CARD_ID: &str = "versioning-zfs";
/// Most snapshots shown per dataset (newest first).
const MAX_ROWS: usize = 60;

/// Pool and data dataset, set on neo-web by the disko module when the initrd
/// hook exists. None: the feature is not available on this machine.
pub fn restore_target() -> Option<(String, String)> {
    let pool = std::env::var("NEO_ZFS_RESTORE_POOL").ok()?;
    let ds = std::env::var("NEO_ZFS_RESTORE_DATASET").ok()?;
    if pool.is_empty() || !dataset_name_ok(&pool) || !dataset_name_ok(&ds) {
        return None;
    }
    if !ds.starts_with(&format!("{pool}/")) {
        return None;
    }
    Some((pool, ds))
}

/// `<ds>.prev-YYYYMMDD-HHMMSS`: the state a restore replaced.
fn prev_suffix<'a>(live: &str, name: &'a str) -> Option<&'a str> {
    let ts = name.strip_prefix(live)?.strip_prefix(".prev-")?;
    let ok = ts.len() == 15
        && ts.as_bytes()[8] == b'-'
        && ts
            .char_indices()
            .all(|(i, c)| if i == 8 { c == '-' } else { c.is_ascii_digit() });
    ok.then_some(ts)
}

fn ts_display(ts: &str) -> String {
    // 20260928-101500 → 2026-09-28 10:15 UTC
    if ts.len() == 15 {
        format!(
            "{}-{}-{} {}:{} UTC",
            &ts[0..4],
            &ts[4..6],
            &ts[6..8],
            &ts[9..11],
            &ts[11..13]
        )
    } else {
        ts.to_string()
    }
}

struct Banner {
    tone: &'static str,
    html: String,
}

pub async fn render_card(notice: Option<(&'static str, String)>) -> String {
    let Some((pool, live)) = restore_target() else {
        // Not a disko/ZFS machine: nothing to show.
        return format!(r#"<div id="{CARD_ID}" class="hidden"></div>"#);
    };

    let mut banners: Vec<Banner> = Vec::new();
    if let Some((tone, msg)) = notice {
        banners.push(Banner { tone, html: msg });
    }

    let pending = get_property(&pool, PROP_RESTORE).await.ok().flatten();
    if let Some(p) = &pending {
        let target = p
            .split('|')
            .nth(1)
            .and_then(|m| m.split('=').nth(1))
            .unwrap_or(p);
        banners.push(Banner {
            tone: "alert-warning",
            html: format!(
                r##"<span>Restore to <span class="font-mono">{}</span> runs at the next boot.</span>
<span class="flex gap-1 ml-auto"><button type="button" class="btn btn-xs btn-error" hx-post="/versioning/zfs/reboot" data-neo-lock="system" hx-target="#{CARD_ID}" hx-swap="outerHTML" hx-confirm="Reboot the server now and restore all Neo data?">Reboot now</button>
<button type="button" class="btn btn-xs" hx-post="/versioning/zfs/cancel" hx-target="#{CARD_ID}" hx-swap="outerHTML">Cancel</button></span>"##,
                escape_html(target)
            ),
        });
    }
    if let Some(r) = get_property(&pool, PROP_RESULT).await.ok().flatten() {
        let mut parts = r.splitn(3, '|');
        let status = parts.next().unwrap_or("");
        let ts = parts.next().unwrap_or("");
        let detail = parts.next().unwrap_or("");
        let (tone, head) = if status == "ok" {
            ("alert-success", "Last restore succeeded")
        } else {
            ("alert-error", "Last restore failed")
        };
        banners.push(Banner {
            tone,
            html: format!(
                r##"<span>{head} ({when}): <span class="font-mono">{detail}</span></span>
<button type="button" class="btn btn-xs btn-ghost ml-auto" hx-post="/versioning/zfs/dismiss" hx-target="#{CARD_ID}" hx-swap="outerHTML">Dismiss</button>"##,
                when = escape_html(&ts_display(ts)),
                detail = escape_html(detail),
            ),
        });
    }

    let sudo = sudo_cmd();
    let (timeline, running_gen, boot_gen) = tokio::task::spawn_blocking(move || {
        (
            GenerationTimeline::load(&sudo),
            running_generation_number(),
            current_generation_number(),
        )
    })
    .await
    .unwrap_or_default();
    let now = now_epoch();
    let busy = pending.is_some();

    let mut html = format!(
        r##"<div id="{CARD_ID}" class="space-y-3">
<div class="flex flex-wrap items-center gap-2">
  <p class="text-xs sm:text-sm text-base-content/60 min-w-0 flex-1">Snapshots of all Neo data — every service's app data and the configuration — on <span class="font-mono">{live}</span>.</p>
  <span class="flex gap-1.5">
    <button type="button" class="{BTN_SECONDARY}" hx-get="/versioning/zfs" hx-target="#{CARD_ID}" hx-swap="outerHTML">Refresh</button>
    <button type="button" class="btn btn-sm btn-primary" hx-post="/versioning/zfs/snapshot" hx-target="#{CARD_ID}" hx-swap="outerHTML" hx-disabled-elt="this" title="zfs snapshot {live}@neo-data-…">Snapshot now</button>
  </span>
</div>
{how}"##,
        live = escape_html(&live),
        how = render::how_it_works(&[
            "The server <b>reboots</b> and swaps the data during boot; services are down until it is back.",
            &format!("<b>Files only:</b> the NixOS system is not part of the snapshot and boots its default generation{}. Activate afterwards to rebuild from the restored settings, or pick <i>Restore + boot gen</i> to also boot the generation that was running back then.", boot_gen.map(|g| format!(" ({g})")).unwrap_or_default()),
            "The replaced data is kept as <i>Before restore</i> below, so a restore can be undone.",
        ]),
    );

    for b in &banners {
        html.push_str(&format!(
            r#"<div role="alert" class="alert alert-soft {} py-2 px-3 text-xs flex flex-wrap gap-2 items-center">{}</div>"#,
            b.tone, b.html
        ));
    }

    if let (Some(run), Some(boot)) = (running_gen, boot_gen) {
        if run != boot {
            banners.push(Banner {
                tone: "alert-info",
                html: format!("<span>Running system generation {run}; the next boot starts generation {boot}.</span>"),
            });
        }
    }

    let ctx = RowCtx {
        live: &live,
        timeline: &timeline,
        running_gen,
        boot_gen,
        now,
        busy,
    };

    match list_snapshots(&live, false).await {
        Err(e) => html.push_str(&format!(
            r#"<div class="text-error text-xs">{}</div>"#,
            escape_html(&e)
        )),
        Ok(list) => html.push_str(&snapshot_table("Current data", &live, &list, &ctx)),
    }

    // States replaced by earlier restores, newest restore first.
    let mut prevs: Vec<(String, String)> = list_filesystems(&pool)
        .await
        .unwrap_or_default()
        .into_iter()
        .filter_map(|d| prev_suffix(&live, &d).map(|ts| (ts.to_string(), d.clone())))
        .collect();
    prevs.sort();
    prevs.reverse();
    for (ts, ds) in prevs {
        if let Ok(list) = list_snapshots(&ds, false).await {
            let title = format!("Before restore at {}", ts_display(&ts));
            html.push_str(&snapshot_table(&title, &ds, &list, &ctx));
        }
    }

    html.push_str("</div>");
    html
}

struct RowCtx<'a> {
    live: &'a str,
    timeline: &'a GenerationTimeline,
    /// Generation running now (`/run/current-system`).
    running_gen: Option<u64>,
    /// Boot default (system profile): what a reboot starts without `+ boot gen`.
    boot_gen: Option<u64>,
    now: i64,
    busy: bool,
}

fn snapshot_table(title: &str, ds: &str, list: &[Snapshot], ctx: &RowCtx) -> String {
    let rows: Vec<String> = list
        .iter()
        .rev()
        .take(MAX_ROWS)
        .map(|s| data_row(s, ctx))
        .collect();
    render::snapshot_list(title, ds, ds == ctx.live, list.len(), &rows, MAX_ROWS)
}

fn data_row(s: &Snapshot, ctx: &RowCtx) -> String {
    let when = format_epoch_utc(s.creation);
    let gen = ctx.timeline.at(s.creation);

    let base_warning = format!(
        "RESTORE ALL NEO DATA to {full} ({when})?\n\n\
         • The server REBOOTS NOW. The swap happens during boot; every service is down until the boot finishes.\n\
         • {live} (appdata of ALL services, the configuration repo, …) is replaced: everything written after {when} is gone from the live data.\n\
         • The current state is kept as {live}.prev-… and listed here, so you can switch back.",
        full = s.full,
        live = ctx.live,
    );

    let q = format!(
        "ds={}&snap={}",
        escape_attr(&s.dataset),
        escape_attr(&s.name)
    );
    let mut actions = String::new();
    if ctx.busy {
        actions.push_str(r#"<button type="button" class="btn btn-sm btn-disabled" disabled title="A restore is already scheduled">Restore</button>"#);
    } else {
        let data_only = format!(
            "{base_warning}\n• FILES ONLY: the system boots its default generation {cur}. Run Activate afterwards if the restored settings should be applied.",
            cur = ctx
                .boot_gen
                .map(|g| g.to_string())
                .unwrap_or_else(|| "(current)".into()),
        );
        actions.push_str(&format!(
            r##"<button type="button" class="{BTN_SECONDARY}" hx-post="/versioning/zfs/restore?{q}" data-neo-lock="system" hx-target="#{CARD_ID}" hx-swap="outerHTML" hx-disabled-elt="this" hx-confirm="{c}" title="Restore data only; boot the default system generation">Restore</button>"##,
            c = escape_attr(&data_only),
        ));
        if let Some(g) = gen.filter(|g| Some(*g) != ctx.boot_gen) {
            let with_gen = format!(
                "{base_warning}\n• The boot default is set to system generation {g} (running when the snapshot was taken) before the reboot."
            );
            actions.push_str(&format!(
                r##"<button type="button" class="{BTN_SECONDARY}" hx-post="/versioning/zfs/restore?{q}&gen={g}" data-neo-lock="system" hx-target="#{CARD_ID}" hx-swap="outerHTML" hx-disabled-elt="this" hx-confirm="{c}" title="Restore data and boot system generation {g}">Restore + boot gen {g}</button>"##,
                c = escape_attr(&with_gen),
            ));
        }
    }

    let gen_cell = match gen {
        // No system profile (dev VM, non-NixOS): nothing to match against.
        _ if ctx.timeline.is_empty() => String::new(),
        Some(g) if Some(g) == ctx.running_gen => {
            format!(" · generation {g} <span class=\"opacity-60\">(running)</span>")
        }
        Some(g) => format!(" · generation {g}"),
        None => r#" · <span title="The system running at that time has no surviving generation">generation ?</span>"#.to_string(),
    };

    let query = render::snap_query(Some(&s.dataset), &s.name);
    actions.push_str(&render::manage_actions(s, &query, &manage_ctx(ctx.busy)));
    render::snapshot_row(s, ctx.now, &gen_cell, &actions, false)
}

fn manage_ctx(busy: bool) -> ManageCtx<'static> {
    ManageCtx {
        base: "/versioning/zfs",
        hx: MANAGE_HX,
        busy,
    }
}

const MANAGE_HX: &str = r##"hx-target="#versioning-zfs" hx-swap="outerHTML""##;

/// Only snapshots of the Neo data dataset and of the states earlier restores
/// replaced (`<ds>.prev-…`) can be managed from the versioning tab.
fn managed_dataset(live: &str, ds: &str) -> bool {
    ds == live || prev_suffix(live, ds).is_some()
}

/// The card with an error notice (e.g. an operation lock refused the change).
pub async fn blocked_card(msg: &str) -> String {
    render_card(err_notice(msg)).await
}

/// Delete / pin / unpin / comment a data snapshot, then re-render the card.
pub async fn manage(ds: &str, snap: &str, action: SnapAction) -> String {
    let Some((_, live)) = restore_target() else {
        return render_card(err_notice(
            "ZFS data snapshots are not available on this machine",
        ))
        .await;
    };
    if !managed_dataset(&live, ds) {
        return render_card(err_notice("snapshot is not of the Neo data dataset")).await;
    }
    let notice = match snapshot_action(ds, snap, action).await {
        Ok(msg) => ok_notice(&msg),
        Err(e) => err_notice(&e),
    };
    render_card(notice).await
}

/// Inline comment editor for one data snapshot (empty when not found).
pub async fn comment_editor(ds: &str, snap: &str) -> String {
    let Some((_, live)) = restore_target() else {
        return String::new();
    };
    if !managed_dataset(&live, ds) || snapshot_ref(ds, snap).is_err() {
        return String::new();
    }
    match list_snapshots(ds, false).await {
        Ok(list) => list
            .iter()
            .find(|s| s.name == snap)
            .map(|s| {
                render::comment_form(s, &render::snap_query(Some(ds), snap), &manage_ctx(false))
            })
            .unwrap_or_default(),
        Err(_) => String::new(),
    }
}

fn ok_notice(msg: &str) -> Option<(&'static str, String)> {
    Some(("alert-success", escape_html(msg)))
}

fn err_notice(msg: &str) -> Option<(&'static str, String)> {
    Some(("alert-error", escape_html(msg)))
}

pub async fn snapshot_now() -> String {
    let Some((_, live)) = restore_target() else {
        return render_card(None).await;
    };
    let name = format!("neo-data-{}", now_ts());
    let notice = match create_snapshot(&live, &name).await {
        Ok(full) => ok_notice(&format!("created {full}")),
        Err(e) => err_notice(&e),
    };
    render_card(notice).await
}

pub async fn cancel() -> String {
    let notice = match restore_target() {
        Some((pool, _)) => match inherit_property(&pool, PROP_RESTORE).await {
            Ok(()) => ok_notice("scheduled restore cancelled"),
            Err(e) => err_notice(&e),
        },
        None => None,
    };
    render_card(notice).await
}

pub async fn dismiss() -> String {
    if let Some((pool, _)) = restore_target() {
        let _ = inherit_property(&pool, PROP_RESULT).await;
    }
    render_card(None).await
}

/// A data restore is scheduled for the next boot.
pub async fn restore_scheduled() -> bool {
    match restore_target() {
        Some((pool, _)) => get_property(&pool, PROP_RESTORE)
            .await
            .ok()
            .flatten()
            .is_some(),
        None => false,
    }
}

fn rebooting_card(msg: &str) -> String {
    format!(
        r#"<div id="{CARD_ID}" class="rounded-box border border-base-300 bg-base-100 p-4 space-y-2"><div role="alert" class="alert alert-warning text-sm"><span class="loading loading-spinner loading-sm"></span><span>{}</span></div><p class="text-xs opacity-70">The UI reconnects once the server is back. Check the result banner in this tab afterwards.</p></div>"#,
        escape_html(msg)
    )
}

pub async fn reboot_now() -> String {
    let Some((pool, _)) = restore_target() else {
        return render_card(None).await;
    };
    if get_property(&pool, PROP_RESTORE)
        .await
        .ok()
        .flatten()
        .is_none()
    {
        return render_card(err_notice("no restore is scheduled")).await;
    }
    power::schedule(PowerAction::Reboot);
    rebooting_card("Rebooting to restore Neo data…")
}

/// Validate, optionally switch the boot generation, schedule the swap, reboot.
pub async fn restore(ds: &str, snap: &str, gen: Option<u64>) -> String {
    let Some((pool, live)) = restore_target() else {
        return render_card(err_notice("data restore is not available on this machine")).await;
    };
    if !dataset_name_ok(ds) || !snapshot_name_ok(snap) {
        return render_card(err_notice("invalid snapshot")).await;
    }
    if ds != live && prev_suffix(&live, ds).is_none() {
        return render_card(err_notice("snapshot is not of the Neo data dataset")).await;
    }
    if get_property(&pool, PROP_RESTORE)
        .await
        .ok()
        .flatten()
        .is_some()
    {
        return render_card(err_notice("a restore is already scheduled")).await;
    }
    match list_snapshots(ds, false).await {
        Ok(list) if list.iter().any(|s| s.name == snap) => {}
        Ok(_) => return render_card(err_notice(&format!("{ds}@{snap} not found"))).await,
        Err(e) => return render_card(err_notice(&e)).await,
    }

    if let Some(n) = gen {
        let sudo = sudo_cmd();
        let res = tokio::task::spawn_blocking(move || {
            switch_system_generation(n, GenerationMode::Boot, &sudo)
        })
        .await
        .unwrap_or_else(|e| Err(e.to_string()));
        if let Err(e) = res {
            return render_card(err_notice(&format!(
                "setting boot generation {n} failed, nothing scheduled: {e}"
            )))
            .await;
        }
    }

    let value = format!("{}|{}={}@{}", now_ts(), live, ds, snap);
    if let Err(e) = set_property(&pool, PROP_RESTORE, &value).await {
        return render_card(err_notice(&format!("scheduling failed: {e}"))).await;
    }
    // A stale result would read like the outcome of this restore.
    let _ = inherit_property(&pool, PROP_RESULT).await;

    power::schedule(PowerAction::Reboot);
    let gen_msg = gen
        .map(|g| format!(" and booting generation {g}"))
        .unwrap_or_default();
    rebooting_card(&format!(
        "Rebooting to restore Neo data to {ds}@{snap}{gen_msg}…"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prev_dataset_names() {
        assert_eq!(
            prev_suffix("zroot/neo", "zroot/neo.prev-20260928-101500"),
            Some("20260928-101500")
        );
        assert_eq!(prev_suffix("zroot/neo", "zroot/neo"), None);
        assert_eq!(prev_suffix("zroot/neo", "zroot/neo.prev-2026"), None);
        assert_eq!(
            prev_suffix("zroot/neo", "zroot/neox.prev-20260928-101500"),
            None
        );
    }

    #[test]
    fn managed_datasets() {
        assert!(managed_dataset("zroot/neo", "zroot/neo"));
        assert!(managed_dataset(
            "zroot/neo",
            "zroot/neo.prev-20260928-101500"
        ));
        assert!(!managed_dataset("zroot/neo", "zroot"));
        assert!(!managed_dataset("zroot/neo", "zroot/root"));
        assert!(!managed_dataset("zroot/neo", "zroot/neo/child"));
        assert!(!managed_dataset("zroot/neo", "zroot/neo.prev-x"));
    }

    #[test]
    fn ts_format() {
        assert_eq!(ts_display("20260928-101500"), "2026-09-28 10:15 UTC");
    }
}
