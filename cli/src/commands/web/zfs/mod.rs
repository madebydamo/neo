//! ZFS snapshots for the web UI.
//!
//! - Per service (option pane): snapshots of the dataset that holds the
//!   service's appdata; restore copies the appdata folder back from
//!   `<mount>/.zfs/snapshot/<snap>/…` (see [`service`]).
//! - Whole Neo data (versioning tab): the dataset behind `neo.core.volumes.root`
//!   is swapped to a snapshot at the next boot by the initrd hook in
//!   `nix/modules/disko/zfs-restore.sh` (see [`data`]).
//!
//! Listing runs unprivileged; snapshot / set / rsync go through `sudo -n`.
pub mod data;
pub mod render;
pub mod service;

use std::path::Path;

use tokio::process::Command as AsyncCommand;

use super::util::{proc, zfs_bin};
use crate::utils::command::now_secs;

/// A mounted ZFS filesystem (from /proc/self/mounts; disko uses legacy mounts,
/// so the `mountpoint` property alone would say "legacy").
#[derive(Clone, Debug, PartialEq)]
pub struct ZfsMount {
    pub dataset: String,
    pub mountpoint: String,
}

/// Decode the octal escapes (`\040` = space) used in /proc/self/mounts.
fn unescape_mount_field(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'\\' && i + 3 < b.len() {
            let oct = &b[i + 1..i + 4];
            if oct.iter().all(|c| (b'0'..=b'7').contains(c)) {
                let v = oct.iter().fold(0u32, |acc, c| acc * 8 + (c - b'0') as u32);
                if let Ok(v) = u8::try_from(v) {
                    out.push(v);
                    i += 4;
                    continue;
                }
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn parse_zfs_mounts(text: &str) -> Vec<ZfsMount> {
    text.lines()
        .filter_map(|l| {
            let mut f = l.split_whitespace();
            let dev = f.next()?;
            let mnt = f.next()?;
            let fstype = f.next()?;
            if fstype != "zfs" || dev.contains('@') {
                return None;
            }
            Some(ZfsMount {
                dataset: unescape_mount_field(dev),
                mountpoint: unescape_mount_field(mnt),
            })
        })
        .collect()
}

pub fn zfs_mounts() -> Vec<ZfsMount> {
    std::fs::read_to_string("/proc/self/mounts")
        .map(|t| parse_zfs_mounts(&t))
        .unwrap_or_default()
}

/// Mounted dataset that contains `path` (longest mountpoint prefix) and the
/// path relative to that mountpoint ("" when `path` is the mountpoint).
pub fn dataset_for_path(mounts: &[ZfsMount], path: &str) -> Option<(ZfsMount, String)> {
    let path = path.trim_end_matches('/');
    let mut best: Option<(ZfsMount, String)> = None;
    for m in mounts {
        let mp = m.mountpoint.trim_end_matches('/');
        let rel = if mp.is_empty() {
            // Mounted at "/"
            Some(path.trim_start_matches('/'))
        } else if path == mp {
            Some("")
        } else {
            path.strip_prefix(mp).and_then(|r| r.strip_prefix('/'))
        };
        if let Some(rel) = rel {
            let better = best
                .as_ref()
                .map(|(b, _)| b.mountpoint.len() < m.mountpoint.len())
                .unwrap_or(true);
            if better {
                best = Some((m.clone(), rel.to_string()));
            }
        }
    }
    best
}

/// `zfs hold` tag of a pinned snapshot. zfstools (services.zfs.autoSnapshot)
/// prunes with `zfs destroy -d`, which only marks a held snapshot for deferred
/// destruction: it survives until the hold is released.
pub const PIN_TAG: &str = "neo-pin";
/// User property with the free-text label shown for a snapshot.
pub const PROP_COMMENT: &str = "neo:comment";
/// Longest comment kept (characters).
pub const COMMENT_MAX: usize = 120;

#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    /// `dataset@name`
    pub full: String,
    pub dataset: String,
    pub name: String,
    /// Unix seconds.
    pub creation: i64,
    /// Bytes unique to this snapshot.
    pub used: u64,
    /// Number of `zfs hold`s (any tag). A held snapshot cannot be destroyed.
    pub userrefs: u64,
    /// Held with [`PIN_TAG`] (resolved by [`list_snapshots`]).
    pub pinned: bool,
    /// A pruner already ran `zfs destroy -d` on it: releasing the last hold destroys it.
    pub defer_destroy: bool,
    /// [`PROP_COMMENT`], if set.
    pub comment: Option<String>,
}

const LIST_PROPS: &str = "name,creation,used,userrefs,defer_destroy,neo:comment";

fn parse_snapshot_list(text: &str) -> Vec<Snapshot> {
    text.lines()
        .filter_map(|l| {
            let mut f = l.split('\t');
            let full = f.next()?.to_string();
            let creation = f.next()?.trim().parse().ok()?;
            let mut num = || f.next().and_then(|u| u.trim().parse().ok()).unwrap_or(0);
            let used = num();
            let userrefs = num();
            let defer_destroy = f.next().map(str::trim) == Some("on");
            let comment = f
                .next()
                .map(str::trim)
                .filter(|c| !c.is_empty() && *c != "-")
                .map(str::to_string);
            let (dataset, name) = full.split_once('@')?;
            Some(Snapshot {
                dataset: dataset.to_string(),
                name: name.to_string(),
                full: full.clone(),
                creation,
                used,
                userrefs,
                pinned: false,
                defer_destroy,
                comment,
            })
        })
        .collect()
}

/// `zfs holds -H` lines (`<snapshot>\t<tag>\t<time>`) → snapshots held with `tag`.
fn parse_holds(text: &str, tag: &str) -> Vec<String> {
    text.lines()
        .filter_map(|l| {
            let mut f = l.split('\t');
            let snap = f.next()?.trim();
            (f.next()?.trim() == tag).then(|| snap.to_string())
        })
        .collect()
}

async fn zfs_output(args: &[&str], sudo: bool) -> Result<String, String> {
    let zfs = zfs_bin();
    let mut cmd = if sudo {
        let mut c = proc::sudo();
        c.arg(&zfs);
        c
    } else {
        AsyncCommand::new(&zfs)
    };
    cmd.args(args);
    proc::run_ok(&mut cmd, &format!("zfs {}", args.first().unwrap_or(&""))).await
}

/// Snapshots of `dataset` (and its descendants when `recursive`), oldest first.
pub async fn list_snapshots(dataset: &str, recursive: bool) -> Result<Vec<Snapshot>, String> {
    let mut args = vec![
        "list", "-H", "-p", "-t", "snapshot", "-o", LIST_PROPS, "-s", "creation",
    ];
    if recursive {
        args.push("-r");
    } else {
        args.extend(["-d", "1"]);
    }
    args.push(dataset);
    let out = zfs_output(&args, false).await?;
    let mut snaps = parse_snapshot_list(&out);
    if !recursive {
        snaps.retain(|s| s.dataset == dataset);
    }
    resolve_pins(&mut snaps).await;
    Ok(snaps)
}

/// Mark snapshots held with [`PIN_TAG`]. Only held ones are queried (usually
/// none or a few). If `zfs holds` fails, any hold counts as a pin.
async fn resolve_pins(snaps: &mut [Snapshot]) {
    let held: Vec<String> = snaps
        .iter()
        .filter(|s| s.userrefs > 0)
        .map(|s| s.full.clone())
        .collect();
    if held.is_empty() {
        return;
    }
    let mut args = vec!["holds", "-H"];
    args.extend(held.iter().map(String::as_str));
    match zfs_output(&args, false).await {
        Ok(out) => {
            let pinned = parse_holds(&out, PIN_TAG);
            for s in snaps.iter_mut() {
                s.pinned = pinned.contains(&s.full);
            }
        }
        Err(e) => {
            eprintln!("web: {e}");
            for s in snaps.iter_mut() {
                s.pinned = s.userrefs > 0;
            }
        }
    }
}

/// Filesystem datasets under `root` (including `root`).
pub async fn list_filesystems(root: &str) -> Result<Vec<String>, String> {
    let out = zfs_output(
        &["list", "-H", "-o", "name", "-t", "filesystem", "-r", root],
        false,
    )
    .await?;
    Ok(out
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect())
}

pub async fn get_property(dataset: &str, prop: &str) -> Result<Option<String>, String> {
    let out = zfs_output(&["get", "-H", "-o", "value", prop, dataset], false).await?;
    let v = out.trim();
    Ok(if v.is_empty() || v == "-" {
        None
    } else {
        Some(v.to_string())
    })
}

pub async fn set_property(dataset: &str, prop: &str, value: &str) -> Result<(), String> {
    let kv = format!("{prop}={value}");
    zfs_output(&["set", &kv, dataset], true).await.map(|_| ())
}

pub async fn inherit_property(dataset: &str, prop: &str) -> Result<(), String> {
    zfs_output(&["inherit", prop, dataset], true)
        .await
        .map(|_| ())
}

/// `zfs snapshot dataset@name` (one atomic call).
pub async fn create_snapshot(dataset: &str, name: &str) -> Result<String, String> {
    if !dataset_name_ok(dataset) || !snapshot_name_ok(name) {
        return Err("invalid snapshot name".to_string());
    }
    let full = format!("{dataset}@{name}");
    zfs_output(&["snapshot", &full], true).await?;
    Ok(full)
}

pub fn dataset_name_ok(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 200
        && !s.starts_with('/')
        && !s.contains("..")
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || "_-.:/".contains(c))
}

/// Snapshot component (after `@`). Also guards the value passed back from the UI.
pub fn snapshot_name_ok(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 200
        && !s.starts_with('.')
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || "_-.:".contains(c))
}

/// `dataset@snap` for a snapshot the UI may act on, or why not. Rejects
/// anything `zfs destroy` could read as more than one snapshot (`%` ranges,
/// `,` lists), a bare dataset, and option-looking names.
pub fn snapshot_ref(dataset: &str, snap: &str) -> Result<String, String> {
    if !dataset_name_ok(dataset) || dataset.starts_with('-') || dataset.ends_with('/') {
        return Err("invalid dataset name".to_string());
    }
    if !snapshot_name_ok(snap) {
        return Err("invalid snapshot name".to_string());
    }
    Ok(format!("{dataset}@{snap}"))
}

/// Comment as stored: single line, no control characters, whitespace
/// squeezed, capped at [`COMMENT_MAX`] characters. `None` clears it.
pub fn sanitize_comment(s: &str) -> Option<String> {
    let squeezed = s
        .split(|c: char| c.is_whitespace() || c.is_control())
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let out: String = squeezed.chars().take(COMMENT_MAX).collect();
    let out = out.trim_end().to_string();
    (!out.is_empty()).then_some(out)
}

/// What a snapshot row action does ([`snapshot_action`]).
#[derive(Clone, Debug, PartialEq)]
pub enum SnapAction {
    Delete,
    Pin,
    Unpin,
    Comment(String),
}

/// Delete / pin / unpin / comment one snapshot of `dataset`. The caller has
/// already decided that `dataset` is one the UI manages; this checks that the
/// snapshot exists and applies the pin rules. Returns a short status message.
pub async fn snapshot_action(
    dataset: &str,
    snap: &str,
    action: SnapAction,
) -> Result<String, String> {
    let full = snapshot_ref(dataset, snap)?;
    let list = list_snapshots(dataset, false).await?;
    let Some(s) = list.iter().find(|s| s.name == snap) else {
        return Err(format!("{full} not found"));
    };
    match action {
        SnapAction::Delete => {
            if s.pinned {
                return Err(format!("{full} is pinned; unpin it first"));
            }
            if s.userrefs > 0 {
                return Err(format!("{full} is held by another tool (zfs holds)"));
            }
            // No -r/-R/-d: exactly this snapshot; fails on dependent clones.
            zfs_output(&["destroy", &full], true).await?;
            Ok(format!("deleted {full}"))
        }
        SnapAction::Pin => {
            if s.pinned {
                return Ok(format!("{full} is already pinned"));
            }
            zfs_output(&["hold", PIN_TAG, &full], true).await?;
            Ok(format!("pinned {full}"))
        }
        SnapAction::Unpin => {
            if !s.pinned {
                return Ok(format!("{full} is not pinned"));
            }
            let expired = s.defer_destroy && s.userrefs <= 1;
            zfs_output(&["release", PIN_TAG, &full], true).await?;
            Ok(if expired {
                format!("unpinned {full}; retention had expired it, so it is deleted")
            } else {
                format!("unpinned {full}")
            })
        }
        SnapAction::Comment(c) => {
            match sanitize_comment(&c) {
                Some(c) => set_property(&full, PROP_COMMENT, &c).await?,
                None => inherit_property(&full, PROP_COMMENT).await?,
            }
            Ok(format!("updated the comment of {full}"))
        }
    }
}

/// UTC timestamp for snapshot names: `20260928-101500`.
pub fn now_ts() -> String {
    let secs = now_epoch();
    let (y, mo, d, h, mi, s) = civil_from_epoch(secs);
    format!("{y:04}{mo:02}{d:02}-{h:02}{mi:02}{s:02}")
}

pub fn now_epoch() -> i64 {
    now_secs() as i64
}

/// (year, month, day, hour, minute, second) in UTC.
fn civil_from_epoch(secs: i64) -> (i64, u32, u32, u32, u32, u32) {
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    // Howard Hinnant's days-to-civil.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    (
        y,
        m,
        d,
        (rem / 3600) as u32,
        ((rem % 3600) / 60) as u32,
        (rem % 60) as u32,
    )
}

/// `2026-09-28 10:15 UTC`
pub fn format_epoch_utc(secs: i64) -> String {
    let (y, mo, d, h, mi, _) = civil_from_epoch(secs);
    format!("{y:04}-{mo:02}-{d:02} {h:02}:{mi:02} UTC")
}

/// Short relative age: `5m ago`, `3h ago`, `2d ago`.
pub fn format_age(now: i64, then: i64) -> String {
    let d = (now - then).max(0);
    if d < 60 {
        "just now".to_string()
    } else if d < 3600 {
        format!("{}m ago", d / 60)
    } else if d < 86_400 {
        format!("{}h ago", d / 3600)
    } else {
        format!("{}d ago", d / 86_400)
    }
}

pub fn human_bytes(b: u64) -> String {
    const UNITS: &[&str] = &["B", "K", "M", "G", "T"];
    let mut v = b as f64;
    let mut i = 0;
    while v >= 1024.0 && i + 1 < UNITS.len() {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{b}B")
    } else {
        format!("{v:.1}{}", UNITS[i])
    }
}

/// Short badge for a snapshot name: (label, daisyUI badge class).
pub fn snapshot_kind(name: &str) -> (String, &'static str) {
    if let Some(rest) = name.strip_prefix("zfs-auto-snap_") {
        let label = rest.split('-').next().unwrap_or("auto");
        return (label.to_string(), "badge-ghost");
    }
    if name.starts_with("neo-prerestore-") {
        return ("pre-restore".to_string(), "badge-warning");
    }
    if name.starts_with("neo-data-") {
        return ("manual · all data".to_string(), "badge-info");
    }
    if let Some(rest) = name.strip_prefix("neo-snap-") {
        // neo-snap-<service>-<YYYYMMDD-HHMMSS>
        let svc = rest
            .len()
            .checked_sub(16)
            .map(|i| &rest[..i])
            .unwrap_or(rest);
        return (format!("manual · {svc}"), "badge-info");
    }
    ("other".to_string(), "badge-ghost")
}

/// Whether `<mount>/.zfs/snapshot/<snap>/<rel>` exists. `None` when it cannot
/// be checked as this user (the restore then relies on rsync's own error).
pub fn snapshot_has_path(mount: &ZfsMount, snap: &str, rel: &str) -> Option<bool> {
    let p = Path::new(&mount.mountpoint)
        .join(".zfs/snapshot")
        .join(snap)
        .join(rel);
    match std::fs::symlink_metadata(&p) {
        Ok(m) => Some(m.is_dir()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Some(false),
        Err(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mounts_parse_zfs_only() {
        let t = "zroot/root / zfs rw,relatime,xattr,posixacl 0 0\n\
                 /dev/vda1 /boot vfat rw 0 0\n\
                 zroot/neo /var/neo zfs rw 0 0\n\
                 zroot/neo@x /var/neo/.zfs/snapshot/x zfs ro 0 0\n\
                 zpool-Media/data /var/neo/DATA/My\\040Media zfs rw 0 0\n";
        let m = parse_zfs_mounts(t);
        assert_eq!(m.len(), 3);
        assert_eq!(m[2].mountpoint, "/var/neo/DATA/My Media");
    }

    #[test]
    fn dataset_longest_prefix() {
        let m = vec![
            ZfsMount {
                dataset: "zroot/root".into(),
                mountpoint: "/".into(),
            },
            ZfsMount {
                dataset: "zroot/neo".into(),
                mountpoint: "/var/neo".into(),
            },
        ];
        let (d, rel) = dataset_for_path(&m, "/var/neo/DATA/AppData/calino").unwrap();
        assert_eq!(d.dataset, "zroot/neo");
        assert_eq!(rel, "DATA/AppData/calino");
        let (d, rel) = dataset_for_path(&m, "/var/lib/foo").unwrap();
        assert_eq!(d.dataset, "zroot/root");
        assert_eq!(rel, "var/lib/foo");
        let (_, rel) = dataset_for_path(&m, "/var/neo/").unwrap();
        assert_eq!(rel, "");
        // Prefix must end at a path boundary.
        let (d, _) = dataset_for_path(&m, "/var/neonate").unwrap();
        assert_eq!(d.dataset, "zroot/root");
    }

    #[test]
    fn snapshot_list_parse() {
        let t = "zroot/neo@zfs-auto-snap_daily-2026-09-27-0000\t1790467200\t12345\n";
        let s = parse_snapshot_list(t);
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].dataset, "zroot/neo");
        assert_eq!(s[0].name, "zfs-auto-snap_daily-2026-09-27-0000");
        assert_eq!(s[0].creation, 1790467200);
        assert_eq!(s[0].used, 12345);
    }

    #[test]
    fn snapshot_list_parse_holds_and_comment() {
        let t = "zroot/neo@neo-data-20260928-101500\t1790467200\t0\t1\ton\tknown good\n\
                 zroot/neo@b\t1790467300\t5\t0\toff\t-\n";
        let s = parse_snapshot_list(t);
        assert_eq!(s.len(), 2);
        assert_eq!(s[0].userrefs, 1);
        assert!(s[0].defer_destroy);
        assert_eq!(s[0].comment.as_deref(), Some("known good"));
        assert!(!s[0].pinned);
        assert_eq!(s[1].used, 5);
        assert_eq!(s[1].comment, None);
        assert!(!s[1].defer_destroy);
    }

    #[test]
    fn holds_parse() {
        let t = "zroot/neo@a\tneo-pin\tMon Sep 28 10:15 2026\n\
                 zroot/neo@b\tsyncoid\tMon Sep 28 10:15 2026\n";
        assert_eq!(parse_holds(t, PIN_TAG), vec!["zroot/neo@a".to_string()]);
    }

    #[test]
    fn snapshot_refs_validated() {
        assert_eq!(
            snapshot_ref("zroot/neo", "neo-data-20260928-101500").as_deref(),
            Ok("zroot/neo@neo-data-20260928-101500")
        );
        assert!(snapshot_ref(
            "zroot/neo.prev-20260928-101500",
            "zfs-auto-snap_daily-2026-09-27-0000"
        )
        .is_ok());
        // Never a dataset, a range, a list, or something option-like.
        assert!(snapshot_ref("zroot/neo", "").is_err());
        assert!(snapshot_ref("zroot/neo", "a%b").is_err());
        assert!(snapshot_ref("zroot/neo", "a,b").is_err());
        assert!(snapshot_ref("zroot/neo", "a@b").is_err());
        assert!(snapshot_ref("zroot/neo", "a b").is_err());
        assert!(snapshot_ref("zroot/neo@x", "a").is_err());
        assert!(snapshot_ref("-r", "a").is_err());
        assert!(snapshot_ref("zroot/", "a").is_err());
        assert!(snapshot_ref("", "a").is_err());
    }

    #[test]
    fn comments_sanitized() {
        assert_eq!(
            sanitize_comment("  known\tgood\n setup "),
            Some("known good setup".into())
        );
        assert_eq!(sanitize_comment(" \n "), None);
        assert_eq!(
            sanitize_comment(&"x".repeat(500)).map(|c| c.len()),
            Some(COMMENT_MAX)
        );
        assert_eq!(
            sanitize_comment("fresh \u{2713}"),
            Some("fresh \u{2713}".into())
        );
    }

    #[test]
    fn civil_dates() {
        assert_eq!(format_epoch_utc(0), "1970-01-01 00:00 UTC");
        assert_eq!(format_epoch_utc(1_790_467_200), "2026-09-27 00:00 UTC");
        assert_eq!(format_epoch_utc(951_782_400), "2000-02-29 00:00 UTC");
    }

    #[test]
    fn names_validated() {
        assert!(snapshot_name_ok("zfs-auto-snap_daily-2026-09-27-0000"));
        assert!(!snapshot_name_ok("a b"));
        assert!(!snapshot_name_ok("x@y"));
        assert!(!snapshot_name_ok("../x"));
        assert!(dataset_name_ok("zroot/neo.prev-20260928-101500"));
        assert!(!dataset_name_ok("/zroot"));
        assert!(!dataset_name_ok("zroot/../x"));
    }

    #[test]
    fn kinds() {
        assert_eq!(
            snapshot_kind("zfs-auto-snap_hourly-2026-09-27-1300").0,
            "hourly"
        );
        assert_eq!(
            snapshot_kind("neo-snap-calino-20260928-101500").0,
            "manual · calino"
        );
        assert_eq!(
            snapshot_kind("neo-prerestore-calino-20260928-101500").0,
            "pre-restore"
        );
    }
}
