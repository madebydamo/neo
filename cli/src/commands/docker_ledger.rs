//! `neo docker-ledger`: the docker updater's image ledger and garbage
//! collection (docs/design/docker-image-retention.md). Run as root by
//! `neo-docker-updater` (update.sh) and `neo-docker-rollback` (rollback.sh);
//! the logic is in `utils::docker_ledger`, this file talks to docker.
//!
//! reconcile  record what every planned image reference points at now
//! gc         reconcile, then delete every image outside the keep set
//! select     versions to return to (`--at TIME` / `--previous`), one TSV line each
use anyhow::{bail, Context, Result};
use clap::Subcommand;
use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::utils::bins;
use crate::utils::command::now_secs;
use crate::utils::docker_ledger::{
    self as ledger, keep_set, parse_age, parse_time, reconcile, select, valid_id, KeepKind, Ledger,
    LocalImage, PlanImage, State, Target, LEDGER_PATH,
};

#[derive(Subcommand, Debug)]
pub enum DockerLedgerAction {
    /// Record which image every planned reference points at now.
    Reconcile {
        #[command(flatten)]
        common: Common,
        /// Also tag versions held back for this long (`neo-history/…`) and
        /// untag current ones. Nothing is deleted.
        #[arg(long, value_name = "AGE")]
        older_than: Option<String>,
    },
    /// Reconcile, then remove every image that is neither current, held back
    /// (closed less than --older-than ago), used by a container nor named in
    /// --keep-refs; then prune the build cache.
    Gc {
        #[command(flatten)]
        common: Common,
        /// How long replaced versions stay on disk (30d, 12h, 90m, 2w).
        #[arg(long, value_name = "AGE")]
        older_than: String,
        /// JSON list of further image references to keep (every declared container).
        #[arg(long, value_name = "FILE")]
        keep_refs: Option<PathBuf>,
        /// Print what would be removed, change nothing.
        #[arg(long)]
        dry_run: bool,
    },
    /// Print the versions to go back to: `ref<TAB>id<TAB>digest|-<TAB>local|gone<TAB>units`.
    Select {
        /// The version that ran at this time (RFC 3339 or seconds since the epoch).
        #[arg(
            long,
            conflicts_with = "previous",
            required_unless_present = "previous"
        )]
        at: Option<String>,
        /// The newest replaced version.
        #[arg(long)]
        previous: bool,
        /// Only this image reference (repeatable).
        #[arg(long = "image", value_name = "REF")]
        images: Vec<String>,
        /// Only references this unit uses (repeatable).
        #[arg(long = "unit", value_name = "UNIT")]
        units: Vec<String>,
        #[arg(long, default_value = LEDGER_PATH)]
        ledger: PathBuf,
        /// Older ledger copy to fall back on (e.g. the one in the run history).
        #[arg(long, value_name = "FILE")]
        fallback: Vec<PathBuf>,
    },
}

#[derive(clap::Args, Debug)]
pub struct Common {
    /// The updater plan (JSON list of `{image, units}`).
    #[arg(long, value_name = "FILE")]
    plan: PathBuf,
    #[arg(long, default_value = LEDGER_PATH)]
    ledger: PathBuf,
    /// Also write the ledger here (the run history, on the snapshotted dataset).
    #[arg(long, value_name = "FILE")]
    copy: Option<PathBuf>,
}

pub fn docker_ledger(action: DockerLedgerAction) -> Result<()> {
    match action {
        DockerLedgerAction::Reconcile { common, older_than } => {
            let older_than = older_than.as_deref().map(parse_age).transpose()?;
            let _lock = lock_ledger(&common.ledger)?;
            let plan = read_plan(&common.plan)?;
            let mut l = ledger::load(&common.ledger)?;
            let now = now();
            reconcile(&mut l, &plan, &gather_state(&plan)?, now);
            save(&common, &l)?;
            if let Some(age) = older_than {
                sync_history_tags(&keep_set(&l, now, age), false, false);
            }
            Ok(())
        }
        DockerLedgerAction::Gc {
            common,
            older_than,
            keep_refs,
            dry_run,
        } => gc(
            &common,
            parse_age(&older_than)?,
            keep_refs.as_deref(),
            dry_run,
        ),
        DockerLedgerAction::Select {
            at,
            previous,
            images,
            units,
            ledger: path,
            fallback,
        } => {
            let target = match (at, previous) {
                (Some(t), _) => {
                    Target::At(parse_time(&t).with_context(|| format!("invalid time {t:?}"))?)
                }
                (None, true) => Target::Previous,
                (None, false) => bail!("give --at TIME or --previous"),
            };
            let mut ledgers = vec![ledger::load(&path)?];
            for f in &fallback {
                // A missing or broken fallback is no reason to fail.
                match ledger::load(f) {
                    Ok(l) => ledgers.push(l),
                    Err(e) => eprintln!("ignoring {}: {e:#}", f.display()),
                }
            }
            for p in select(&ledgers, target, &images, &units) {
                println!(
                    "{}\t{}\t{}\t{}\t{}",
                    p.reference,
                    p.version.id,
                    p.version.digest.as_deref().unwrap_or("-"),
                    if p.version.local { "local" } else { "gone" },
                    if p.units.is_empty() {
                        "-".to_string()
                    } else {
                        p.units.join(",")
                    },
                );
            }
            Ok(())
        }
    }
}

fn now() -> i64 {
    now_secs() as i64
}

fn save(common: &Common, l: &Ledger) -> Result<()> {
    ledger::save(&common.ledger, l)?;
    if let Some(copy) = &common.copy {
        // The history copy is a fallback; never fail the run for it.
        if let Err(e) = ledger::save(copy, l) {
            eprintln!("warning: ledger copy {}: {e:#}", copy.display());
        }
    }
    Ok(())
}

/// Exclusive lock next to the ledger for the whole read-modify-write
/// (updater run and rollback may overlap). Released on drop.
fn lock_ledger(path: &Path) -> Result<File> {
    let lock_path = path.with_extension("lock");
    if let Some(dir) = lock_path.parent() {
        std::fs::create_dir_all(dir).with_context(|| format!("create {}", dir.display()))?;
    }
    let f = File::create(&lock_path).with_context(|| format!("open {}", lock_path.display()))?;
    // SAFETY: flock on a file descriptor we own for the lifetime of `f`.
    let rc = unsafe { libc::flock(f.as_raw_fd(), libc::LOCK_EX) };
    if rc != 0 {
        bail!(
            "lock {}: {}",
            lock_path.display(),
            std::io::Error::last_os_error()
        );
    }
    Ok(f)
}

fn read_plan(path: &Path) -> Result<Vec<PlanImage>> {
    let s = std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    serde_json::from_str(&s).with_context(|| format!("parse {}", path.display()))
}

fn read_refs(path: &Path) -> Result<Vec<String>> {
    let s = std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    serde_json::from_str(&s).with_context(|| format!("parse {}", path.display()))
}

fn docker(args: &[&str]) -> std::io::Result<std::process::Output> {
    Command::new(bins::docker()).args(args).output()
}

/// stdout of a docker command that must succeed.
fn docker_ok(args: &[&str]) -> Result<String> {
    let out = docker(args).with_context(|| format!("run docker {}", args.join(" ")))?;
    if !out.status.success() {
        bail!(
            "docker {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// What `reference` points at locally; None when it is not on disk.
fn inspect_ref(reference: &str) -> Option<LocalImage> {
    let out = docker(&[
        "image",
        "inspect",
        "--format",
        "{{.Id}}\t{{json .RepoDigests}}",
        "--",
        reference,
    ])
    .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout);
    let (id, digests) = s.trim().split_once('\t')?;
    let id = id.trim().to_string();
    if !valid_id(&id) {
        return None;
    }
    let repo_digests: Vec<String> = serde_json::from_str(digests.trim()).unwrap_or_default();
    Some(LocalImage { id, repo_digests })
}

/// Ids of all images. `-a`: with the containerd image store, the dangling
/// image a pull leaves behind is only listed with it.
fn all_image_ids() -> Result<BTreeSet<String>> {
    Ok(docker_ok(&["image", "ls", "-a", "-q", "--no-trunc"])?
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| valid_id(l))
        .collect())
}

/// Image ids of every container, running or stopped.
fn container_image_ids() -> Result<BTreeSet<String>> {
    let ids: Vec<String> = docker_ok(&["ps", "-a", "-q", "--no-trunc"])?
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect();
    if ids.is_empty() {
        return Ok(BTreeSet::new());
    }
    let mut args = vec!["container", "inspect", "--format", "{{.Image}}", "--"];
    args.extend(ids.iter().map(String::as_str));
    // A container removed meanwhile makes inspect fail; ask again one by one.
    match docker_ok(&args) {
        Ok(out) => Ok(out.lines().map(|l| l.trim().to_string()).collect()),
        Err(_) => Ok(ids
            .iter()
            .filter_map(|c| {
                docker_ok(&["container", "inspect", "--format", "{{.Image}}", "--", c]).ok()
            })
            .map(|s| s.trim().to_string())
            .collect()),
    }
}

fn gather_state(plan: &[PlanImage]) -> Result<State> {
    let refs: BTreeMap<String, LocalImage> = plan
        .iter()
        .filter_map(|p| inspect_ref(&p.image).map(|l| (p.image.clone(), l)))
        .collect();
    Ok(State {
        refs,
        present: Some(all_image_ids()?),
    })
}

fn repo_tags(id: &str) -> Vec<String> {
    docker_ok(&[
        "image",
        "inspect",
        "--format",
        "{{json .RepoTags}}",
        "--",
        id,
    ])
    .ok()
    .and_then(|s| serde_json::from_str(s.trim()).ok())
    .unwrap_or_default()
}

fn short(id: &str) -> &str {
    let hex = id.split_once(':').map(|(_, h)| h).unwrap_or(id);
    &hex[..hex.len().min(12)]
}

fn first_line(b: &[u8]) -> String {
    String::from_utf8_lossy(b)
        .lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("")
        .trim()
        .to_string()
}

/// Remove an image without -f: drop its tags one by one (the last one removes
/// it when no container uses it), then the id itself if it is still there.
fn remove_image(id: &str) -> Result<(), String> {
    for tag in repo_tags(id) {
        if let Ok(out) = docker(&["rmi", "--", &tag]) {
            if !out.status.success() {
                return Err(first_line(&out.stderr));
            }
        }
    }
    let exists = docker(&["image", "inspect", "--format", "{{.Id}}", "--", id])
        .map(|o| o.status.success())
        .unwrap_or(false);
    if !exists {
        return Ok(());
    }
    match docker(&["rmi", "--", id]) {
        Ok(out) if out.status.success() => Ok(()),
        Ok(out) => Err(first_line(&out.stderr)),
        Err(e) => Err(e.to_string()),
    }
}

/// Tags under `neo-history/` and the legacy `neo-rollback` repository.
fn marker_tags() -> Vec<(String, String)> {
    let mut out = Vec::new();
    for filter in ["reference=neo-history/*", "reference=neo-rollback"] {
        if let Ok(s) = docker_ok(&[
            "image",
            "ls",
            "--no-trunc",
            "--filter",
            filter,
            "--format",
            "{{.Repository}}:{{.Tag}}\t{{.ID}}",
        ]) {
            out.extend(s.lines().filter_map(|l| {
                let (t, id) = l.trim().split_once('\t')?;
                Some((t.to_string(), id.to_string()))
            }));
        }
    }
    out
}

/// Held-back versions carry a `neo-history/…` tag; other marker tags go.
/// `all = false` (between GCs) only untags images that are current, so
/// nothing is deleted. Returns the number of held-back images.
fn sync_history_tags(keeps: &[ledger::Keep], all: bool, dry_run: bool) -> usize {
    let wanted: BTreeMap<String, String> = keeps
        .iter()
        .filter_map(|k| match &k.kind {
            KeepKind::Holdback(tag) => Some((tag.clone(), k.id.clone())),
            KeepKind::Current => None,
        })
        .collect();
    let current: BTreeSet<&str> = keeps
        .iter()
        .filter(|k| k.kind == KeepKind::Current)
        .map(|k| k.id.as_str())
        .collect();
    let existing = marker_tags();
    for (tag, id) in &wanted {
        if existing.iter().any(|(t, i)| t == tag && i == id) {
            continue;
        }
        println!("tag {} as {tag}", short(id));
        if !dry_run {
            if let Err(e) = docker_ok(&["tag", id, tag]) {
                eprintln!("  {e:#}");
            }
        }
    }
    for (tag, id) in existing.iter().filter(|(t, _)| !wanted.contains_key(t)) {
        if !all && !current.contains(id.as_str()) {
            continue;
        }
        println!("untag {tag}");
        if !dry_run {
            // Fails harmlessly when a container still uses an image with no other tag.
            let _ = docker(&["rmi", "--", tag]);
        }
    }
    wanted.len()
}

fn gc(common: &Common, older_than: i64, keep_refs: Option<&Path>, dry_run: bool) -> Result<()> {
    let _lock = lock_ledger(&common.ledger)?;
    let plan = read_plan(&common.plan)?;
    let mut l = ledger::load(&common.ledger)?;
    let now = now();
    let state = gather_state(&plan)?;
    reconcile(&mut l, &plan, &state, now);
    if !dry_run {
        save(common, &l)?;
    }

    // Keep set. Any failure to learn part of it aborts before deleting anything.
    let keeps = keep_set(&l, now, older_than);
    let mut keep: BTreeSet<String> = keeps.iter().map(|k| k.id.clone()).collect();
    keep.extend(state.refs.values().map(|li| li.id.clone()));
    keep.extend(container_image_ids().context("list containers; nothing removed")?);
    if let Some(p) = keep_refs {
        for r in read_refs(p)? {
            if let Some(li) = inspect_ref(&r) {
                keep.insert(li.id);
            }
        }
    }

    let held = sync_history_tags(&keeps, true, dry_run);

    let all = all_image_ids()?;
    let (mut removed, mut failed) = (0usize, 0usize);
    for id in all.iter().filter(|id| !keep.contains(*id)) {
        let tags = repo_tags(id);
        let label = if tags.is_empty() {
            "<untagged>".to_string()
        } else {
            tags.join(", ")
        };
        println!("remove {} ({label})", short(id));
        if dry_run {
            removed += 1;
            continue;
        }
        match remove_image(id) {
            Ok(()) => removed += 1,
            Err(e) => {
                failed += 1;
                println!("  kept: {e}");
            }
        }
    }
    println!(
        "kept {} images ({} held back), {} {removed}{}",
        all.len() - removed,
        held,
        if dry_run { "would remove" } else { "removed" },
        if failed > 0 {
            format!(", {failed} could not be removed")
        } else {
            String::new()
        }
    );

    if dry_run {
        return Ok(());
    }
    // `local` follows what is left.
    let after = State {
        refs: state.refs,
        present: Some(all_image_ids()?),
    };
    reconcile(&mut l, &plan, &after, now);
    save(common, &l)?;

    match docker(&["builder", "prune", "-af"]) {
        Ok(out) if out.status.success() => {
            let last = String::from_utf8_lossy(&out.stdout)
                .lines()
                .rev()
                .find(|l| !l.trim().is_empty())
                .unwrap_or("")
                .trim()
                .to_string();
            if !last.is_empty() {
                println!("build cache: {last}");
            }
        }
        Ok(out) => eprintln!("docker builder prune: {}", first_line(&out.stderr)),
        Err(e) => eprintln!("docker builder prune: {e}"),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_ids() {
        assert_eq!(short("sha256:0123456789abcdef"), "0123456789ab");
        assert_eq!(short("abc"), "abc");
    }
}
