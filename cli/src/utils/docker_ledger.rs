//! Docker image ledger: which image ran under which image reference when
//! (docs/design/docker-image-retention.md).
//!
//! Written by `neo docker-ledger` from the docker updater (`update.sh`) and
//! `neo-docker-rollback`; read by the web UI to offer "return the app to the
//! version it ran then" after a restore. Only pure logic lives here; the docker
//! calls are in `commands::docker_ledger`.
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
use std::path::Path;

/// Authoritative copy, on the root dataset: it must not travel back with a
/// data restore. Also in `nix/services/docker-updater/default.nix`.
pub const LEDGER_PATH: &str = "/var/lib/neo/docker-images.json";

/// Repository prefix of the tags that mark held-back versions.
pub const HISTORY_REPO: &str = "neo-history/";

/// Versions that are gone from disk are forgotten after this long.
const FORGET_AFTER: i64 = 365 * 86_400;

#[derive(Serialize, Deserialize, Default, Debug, Clone, PartialEq)]
pub struct Ledger {
    #[serde(default)]
    pub updated_at: Option<String>,
    #[serde(default)]
    pub images: BTreeMap<String, Entry>,
}

#[derive(Serialize, Deserialize, Default, Debug, Clone, PartialEq)]
pub struct Entry {
    /// Units that use this reference (from the updater plan).
    #[serde(default)]
    pub units: Vec<String>,
    /// Newest first; at most one has `until = None` (the current version).
    #[serde(default)]
    pub versions: Vec<Version>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Version {
    /// Local image id (what is retagged).
    pub id: String,
    /// Registry digest `repo@sha256:…` (what can be pulled again); None for
    /// local or Nix-built images.
    #[serde(default)]
    pub digest: Option<String>,
    pub from: String,
    #[serde(default)]
    pub until: Option<String>,
    /// The image is still on disk.
    #[serde(default = "yes")]
    pub local: bool,
}

fn yes() -> bool {
    true
}

impl Version {
    pub fn from_epoch(&self) -> i64 {
        parse_time(&self.from).unwrap_or(0)
    }
    /// None while current.
    pub fn until_epoch(&self) -> Option<i64> {
        self.until
            .as_deref()
            .map(|u| parse_time(u).unwrap_or(i64::MAX))
    }
    pub fn is_current(&self) -> bool {
        self.until.is_none()
    }
    fn covers(&self, t: i64) -> bool {
        self.from_epoch() <= t && self.until_epoch().is_none_or(|u| t < u)
    }
}

impl Entry {
    pub fn current(&self) -> Option<&Version> {
        self.versions.iter().find(|v| v.is_current())
    }
    fn close_current(&mut self, now: &str) {
        for v in self.versions.iter_mut().filter(|v| v.is_current()) {
            v.until = Some(now.to_string());
        }
    }
}

/// One image of the updater plan (`NEO_PLAN`); other fields are ignored.
#[derive(Deserialize, Debug, Clone)]
pub struct PlanImage {
    pub image: String,
    #[serde(default)]
    pub units: Vec<String>,
}

/// What a reference points at locally right now.
#[derive(Debug, Clone, Default)]
pub struct LocalImage {
    pub id: String,
    pub repo_digests: Vec<String>,
}

/// Local state: per planned reference, and (when known) every image id on disk.
#[derive(Debug, Clone, Default)]
pub struct State {
    pub refs: BTreeMap<String, LocalImage>,
    pub present: Option<BTreeSet<String>>,
}

pub fn valid_id(id: &str) -> bool {
    let hex = id
        .strip_prefix("sha256:")
        .or_else(|| id.strip_prefix("sha512:"));
    hex.is_some_and(|h| !h.is_empty() && h.chars().all(|c| c.is_ascii_hexdigit()))
}

/// Bring the ledger in line with the plan and what is on disk at `now`:
/// a planned reference whose local id differs from its current version gets a
/// new version (the old one is closed), references no longer planned are
/// closed, `local` follows `present`, and versions that are gone and old (or
/// gone and not pullable) are dropped.
pub fn reconcile(ledger: &mut Ledger, plan: &[PlanImage], state: &State, now: i64) {
    let now_s = format_time(now);
    let planned: BTreeSet<&str> = plan.iter().map(|p| p.image.as_str()).collect();

    for p in plan {
        let entry = ledger.images.entry(p.image.clone()).or_default();
        entry.units = p.units.clone();
        let Some(local) = state.refs.get(&p.image).filter(|l| valid_id(&l.id)) else {
            continue;
        };
        let digest = pick_digest(&p.image, &local.repo_digests);
        let same = entry.current().is_some_and(|v| v.id == local.id);
        if same {
            let cur = entry
                .versions
                .iter_mut()
                .find(|v| v.is_current())
                .expect("current exists");
            if cur.digest.is_none() {
                cur.digest = digest;
            }
            cur.local = true;
        } else {
            entry.close_current(&now_s);
            entry.versions.insert(
                0,
                Version {
                    id: local.id.clone(),
                    digest,
                    from: now_s.clone(),
                    until: None,
                    local: true,
                },
            );
        }
    }

    for (r, entry) in ledger.images.iter_mut() {
        if !planned.contains(r.as_str()) {
            entry.close_current(&now_s);
        }
    }

    if let Some(present) = &state.present {
        for entry in ledger.images.values_mut() {
            for v in entry.versions.iter_mut() {
                v.local = present.contains(&v.id);
            }
        }
    }

    for entry in ledger.images.values_mut() {
        entry.versions.retain(|v| {
            if v.local || v.is_current() {
                return true;
            }
            v.digest.is_some() && v.until_epoch().is_some_and(|u| u > now - FORGET_AFTER)
        });
    }
    ledger.images.retain(|_, e| !e.versions.is_empty());
    ledger.updated_at = Some(now_s);
}

/// Why an image is kept.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum KeepKind {
    Current,
    /// Closed less than the GC age ago; carries its `neo-history/…` tag.
    Holdback(String),
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Keep {
    pub id: String,
    pub reference: String,
    pub kind: KeepKind,
}

/// Images the ledger keeps: every current version, and closed versions still
/// on disk whose `until` is younger than `older_than` seconds. An id that is
/// current somewhere is never also a holdback.
pub fn keep_set(ledger: &Ledger, now: i64, older_than: i64) -> Vec<Keep> {
    let current: BTreeSet<&str> = ledger
        .images
        .values()
        .filter_map(|e| e.current().map(|v| v.id.as_str()))
        .collect();
    let mut out = Vec::new();
    for (r, e) in &ledger.images {
        for v in &e.versions {
            if v.is_current() {
                out.push(Keep {
                    id: v.id.clone(),
                    reference: r.clone(),
                    kind: KeepKind::Current,
                });
            } else if v.local
                && !current.contains(v.id.as_str())
                && v.until_epoch().is_some_and(|u| u > now - older_than)
            {
                out.push(Keep {
                    id: v.id.clone(),
                    reference: r.clone(),
                    kind: KeepKind::Holdback(history_tag(r, &v.id)),
                });
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

/// `neo-history/<ref, / : @ as __>:<12 hex of the id>`, a valid docker tag.
pub fn history_tag(reference: &str, id: &str) -> String {
    let hex = id.split_once(':').map(|(_, h)| h).unwrap_or(id);
    let short: String = hex.chars().take(12).collect();
    format!("{HISTORY_REPO}{}:{short}", repo_component(reference))
}

/// One docker repository path component: `[a-z0-9]+((\.|_|__|-+)[a-z0-9]+)*`.
fn repo_component(s: &str) -> String {
    let mut out = String::new();
    let mut sep = String::new();
    for c in s.chars().map(|c| c.to_ascii_lowercase()) {
        if c.is_ascii_alphanumeric() {
            if !out.is_empty() && !sep.is_empty() {
                let ok = sep == "." || sep == "_" || sep == "__" || sep.chars().all(|c| c == '-');
                out.push_str(if ok { &sep } else { "__" });
            }
            sep.clear();
            out.push(c);
        } else {
            sep.push(c);
        }
    }
    // Repository names are at most 255 characters; leave room for the prefix.
    out.truncate(200);
    while out.ends_with(['.', '_', '-']) {
        out.pop();
    }
    if out.is_empty() {
        "image".to_string()
    } else {
        out
    }
}

/// Repository part of a reference, without tag or digest, with Docker Hub's
/// implicit prefixes removed: `docker.io/library/redis:7` → `redis`.
pub fn repo_of(reference: &str) -> String {
    let no_digest = reference.split('@').next().unwrap_or(reference);
    let (head, last) = match no_digest.rsplit_once('/') {
        Some((h, l)) => (Some(h), l),
        None => (None, no_digest),
    };
    let last = last.split(':').next().unwrap_or(last);
    let full = match head {
        Some(h) => format!("{h}/{last}"),
        None => last.to_string(),
    };
    let full = full
        .strip_prefix("docker.io/")
        .or_else(|| full.strip_prefix("index.docker.io/"))
        .map(str::to_string)
        .unwrap_or(full);
    full.strip_prefix("library/")
        .map(str::to_string)
        .unwrap_or(full)
}

/// The registry digest to record for `reference`: the reference itself when it
/// is pinned by digest, else the `RepoDigests` entry of the same repository.
pub fn pick_digest(reference: &str, repo_digests: &[String]) -> Option<String> {
    if reference.contains("@sha256:") {
        return Some(reference.to_string());
    }
    let repo = repo_of(reference);
    repo_digests
        .iter()
        .find(|d| d.contains('@') && repo_of(d) == repo)
        .cloned()
}

/// What `select` looks for.
#[derive(Debug, Clone, Copy)]
pub enum Target {
    /// The version that ran at this time.
    At(i64),
    /// The newest closed version.
    Previous,
}

/// A version to go back to.
#[derive(Debug, Clone, PartialEq)]
pub struct Pick {
    pub reference: String,
    pub version: Version,
    pub units: Vec<String>,
}

/// Versions to return to, one per reference. `ledgers[0]` is authoritative
/// (its current version is what runs now); later ones are fallbacks for
/// references or times the first no longer knows. A reference is selected
/// when it is in `images`, uses one of `units`, or both filters are empty.
/// References whose target is what runs now are left out.
pub fn select(
    ledgers: &[Ledger],
    target: Target,
    images: &[String],
    units: &[String],
) -> Vec<Pick> {
    let Some(main) = ledgers.first() else {
        return Vec::new();
    };
    let refs: BTreeSet<&String> = ledgers.iter().flat_map(|l| l.images.keys()).collect();
    let mut out = Vec::new();
    for r in refs {
        let entry_units = ledgers
            .iter()
            .find_map(|l| l.images.get(r))
            .map(|e| e.units.clone())
            .unwrap_or_default();
        let wanted = (images.is_empty() && units.is_empty())
            || images.iter().any(|i| i == r)
            || units.iter().any(|u| entry_units.contains(u));
        if !wanted {
            continue;
        }
        let current_id = main
            .images
            .get(r)
            .and_then(|e| e.current())
            .map(|v| v.id.clone());
        let found = ledgers.iter().find_map(|l| {
            let e = l.images.get(r)?;
            match target {
                Target::At(t) => e.versions.iter().find(|v| v.covers(t)).cloned(),
                Target::Previous => e
                    .versions
                    .iter()
                    .filter(|v| !v.is_current() && Some(&v.id) != current_id.as_ref())
                    .max_by_key(|v| v.until_epoch())
                    .cloned(),
            }
        });
        if let Some(v) = found {
            if Some(&v.id) == current_id.as_ref() {
                continue;
            }
            // A fallback ledger may not know the version is still on disk.
            let local = main
                .images
                .values()
                .flat_map(|e| e.versions.iter())
                .any(|m| m.id == v.id && m.local);
            out.push(Pick {
                reference: r.clone(),
                version: Version { local, ..v },
                units: entry_units,
            });
        }
    }
    out
}

/// Seconds from `30d`, `12h`, `90m`, `2w`, `45s` (or a bare number of days).
pub fn parse_age(s: &str) -> Result<i64> {
    let s = s.trim();
    let (num, mult) = match s.char_indices().last() {
        Some((i, 's')) => (&s[..i], 1),
        Some((i, 'm')) => (&s[..i], 60),
        Some((i, 'h')) => (&s[..i], 3600),
        Some((i, 'd')) => (&s[..i], 86_400),
        Some((i, 'w')) => (&s[..i], 7 * 86_400),
        _ => (s, 86_400),
    };
    let n: i64 = num
        .trim()
        .parse()
        .with_context(|| format!("invalid age {s:?} (expected e.g. 30d, 12h, 90m)"))?;
    if n < 0 {
        bail!("invalid age {s:?}: negative");
    }
    Ok(n.saturating_mul(mult))
}

/// Seconds since the epoch from `1763442720`, `@1763442720` or RFC 3339
/// (`2026-11-18T05:12:00+01:00`, `2026-11-18T04:12:00Z`).
pub fn parse_time(s: &str) -> Option<i64> {
    let s = s.trim();
    let digits = s.strip_prefix('@').unwrap_or(s);
    if !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit()) {
        return digits.parse().ok();
    }
    let b = s.as_bytes();
    if b.len() < 19 || !matches!(b[10], b'T' | b't' | b' ') {
        return None;
    }
    let num = |r: std::ops::Range<usize>| -> Option<i64> {
        let part = s.get(r)?;
        part.chars()
            .all(|c| c.is_ascii_digit())
            .then(|| part.parse().ok())?
    };
    if b[4] != b'-' || b[7] != b'-' || b[13] != b':' || b[16] != b':' {
        return None;
    }
    let (y, mo, d) = (num(0..4)?, num(5..7)?, num(8..10)?);
    let (h, mi, sec) = (num(11..13)?, num(14..16)?, num(17..19)?);
    let mut rest = &s[19..];
    if let Some(frac) = rest.strip_prefix('.') {
        let n = frac.chars().take_while(|c| c.is_ascii_digit()).count();
        rest = &frac[n..];
    }
    let offset = match rest {
        "Z" | "z" | "" => 0,
        _ => {
            let sign = match rest.as_bytes()[0] {
                b'+' => 1,
                b'-' => -1,
                _ => return None,
            };
            let hm = &rest[1..];
            let (oh, om) = hm.split_once(':').unwrap_or((hm.get(..2)?, hm.get(2..)?));
            sign * (oh.parse::<i64>().ok()? * 3600 + om.parse::<i64>().ok()? * 60)
        }
    };
    if !(1..=12).contains(&mo) || !(1..=31).contains(&d) || h > 23 || mi > 59 || sec > 60 {
        return None;
    }
    Some(days_from_civil(y, mo, d) * 86_400 + h * 3600 + mi * 60 + sec - offset)
}

/// `2026-11-18T04:12:00Z`
pub fn format_time(secs: i64) -> String {
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let (y, m, d) = civil_from_days(days);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60
    )
}

// Howard Hinnant's civil calendar algorithms.
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (
        if m <= 2 {
            yoe + era * 400 + 1
        } else {
            yoe + era * 400
        },
        m,
        d,
    )
}

/// The ledger at `path`; empty when the file does not exist.
pub fn load(path: &Path) -> Result<Ledger> {
    match std::fs::read_to_string(path) {
        Ok(s) if s.trim().is_empty() => Ok(Ledger::default()),
        Ok(s) => serde_json::from_str(&s).with_context(|| format!("parse {}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Ledger::default()),
        Err(e) => Err(e).with_context(|| format!("read {}", path.display())),
    }
}

/// Write atomically (temp file + rename), world-readable: the web UI reads it.
pub fn save(path: &Path, ledger: &Ledger) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let dir = path.parent().unwrap_or(Path::new("."));
    std::fs::create_dir_all(dir).with_context(|| format!("create {}", dir.display()))?;
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "ledger.json".into());
    let tmp = dir.join(format!(".{name}.tmp-{}", std::process::id()));
    let mut f = std::fs::File::create(&tmp).with_context(|| format!("create {}", tmp.display()))?;
    let mut body = serde_json::to_vec_pretty(ledger)?;
    body.push(b'\n');
    f.write_all(&body)?;
    f.sync_all()?;
    f.set_permissions(std::fs::Permissions::from_mode(0o644))?;
    drop(f);
    std::fs::rename(&tmp, path).with_context(|| format!("rename to {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: &str = "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const B: &str = "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    const C: &str = "sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";
    const IMMICH: &str = "ghcr.io/immich-app/immich-server:release";
    const DAY: i64 = 86_400;

    fn plan(refs: &[&str]) -> Vec<PlanImage> {
        refs.iter()
            .map(|r| PlanImage {
                image: r.to_string(),
                units: vec![format!("docker-{}", repo_of(r).replace('/', "-"))],
            })
            .collect()
    }

    fn state(refs: &[(&str, &str)], present: &[&str]) -> State {
        State {
            refs: refs
                .iter()
                .map(|(r, id)| {
                    (
                        r.to_string(),
                        LocalImage {
                            id: id.to_string(),
                            repo_digests: vec![format!("{}@sha256:{}", repo_of(r), &id[7..15])],
                        },
                    )
                })
                .collect(),
            present: Some(present.iter().map(|s| s.to_string()).collect()),
        }
    }

    #[test]
    fn times_round_trip() {
        let t = parse_time("2026-11-18T05:12:00+01:00").unwrap();
        assert_eq!(format_time(t), "2026-11-18T04:12:00Z");
        assert_eq!(parse_time("2026-11-18T04:12:00Z"), Some(t));
        assert_eq!(parse_time("2026-11-18T04:12:00.123Z"), Some(t));
        assert_eq!(parse_time(&t.to_string()), Some(t));
        assert_eq!(parse_time(&format!("@{t}")), Some(t));
        assert_eq!(parse_time("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(parse_time("2000-02-29T00:00:00Z"), Some(951_782_400));
        assert_eq!(parse_time("garbage"), None);
        assert_eq!(parse_time("2026-13-01T00:00:00Z"), None);
    }

    #[test]
    fn ages() {
        assert_eq!(parse_age("30d").unwrap(), 30 * DAY);
        assert_eq!(parse_age("1m").unwrap(), 60);
        assert_eq!(parse_age("2w").unwrap(), 14 * DAY);
        assert_eq!(parse_age("12h").unwrap(), 12 * 3600);
        assert_eq!(parse_age("7").unwrap(), 7 * DAY);
        assert!(parse_age("soon").is_err());
        assert!(parse_age("").is_err());
    }

    #[test]
    fn repos_and_digests() {
        assert_eq!(repo_of("docker.io/library/redis:7"), "redis");
        assert_eq!(repo_of("redis"), "redis");
        assert_eq!(repo_of("localhost:5000/x/y:1"), "localhost:5000/x/y");
        assert_eq!(repo_of(IMMICH), "ghcr.io/immich-app/immich-server");
        assert_eq!(repo_of("ghcr.io/a/b@sha256:00"), "ghcr.io/a/b");
        let ds = vec![
            "other/thing@sha256:11".to_string(),
            "ghcr.io/immich-app/immich-server@sha256:22".to_string(),
        ];
        assert_eq!(
            pick_digest(IMMICH, &ds).as_deref(),
            Some("ghcr.io/immich-app/immich-server@sha256:22")
        );
        assert_eq!(
            pick_digest("redis:7", &["redis@sha256:33".into()]).as_deref(),
            Some("redis@sha256:33")
        );
        assert_eq!(pick_digest("local/app:dev", &[]), None);
        assert_eq!(
            pick_digest("x/y@sha256:44", &[]).as_deref(),
            Some("x/y@sha256:44")
        );
    }

    #[test]
    fn history_tags_are_valid() {
        assert_eq!(
            history_tag(IMMICH, A),
            "neo-history/ghcr.io__immich-app__immich-server__release:aaaaaaaaaaaa"
        );
        assert_eq!(
            history_tag("localhost:5000/My_App@sha256:ff", B),
            "neo-history/localhost__5000__my_app__sha256__ff:bbbbbbbbbbbb"
        );
        assert_eq!(history_tag("a_/b", C), "neo-history/a__b:cccccccccccc");
        assert_eq!(history_tag("--", C), "neo-history/image:cccccccccccc");
    }

    #[test]
    fn first_run_then_update_then_removal() {
        let mut l = Ledger::default();
        let p = plan(&[IMMICH, "redis:7"]);
        reconcile(
            &mut l,
            &p,
            &state(&[(IMMICH, A), ("redis:7", C)], &[A, C]),
            100,
        );
        let e = &l.images[IMMICH];
        assert_eq!(e.versions.len(), 1);
        assert_eq!(e.current().unwrap().id, A);
        assert_eq!(e.current().unwrap().from, format_time(100));
        assert_eq!(e.units, vec!["docker-ghcr.io-immich-app-immich-server"]);

        // Same state again: nothing new.
        reconcile(
            &mut l,
            &p,
            &state(&[(IMMICH, A), ("redis:7", C)], &[A, C]),
            150,
        );
        assert_eq!(l.images[IMMICH].versions.len(), 1);

        // Pull moved the tag.
        reconcile(
            &mut l,
            &p,
            &state(&[(IMMICH, B), ("redis:7", C)], &[A, B, C]),
            200,
        );
        let e = &l.images[IMMICH];
        assert_eq!(e.versions.len(), 2);
        assert_eq!(e.versions[0].id, B);
        assert!(e.versions[0].is_current());
        assert_eq!(e.versions[1].id, A);
        assert_eq!(
            e.versions[1].until.as_deref(),
            Some(format_time(200).as_str())
        );

        // redis no longer planned: closed.
        reconcile(
            &mut l,
            &plan(&[IMMICH]),
            &state(&[(IMMICH, B)], &[A, B, C]),
            300,
        );
        assert!(l.images["redis:7"].current().is_none());

        // Missing local image for a planned ref: current version stays open.
        reconcile(&mut l, &plan(&[IMMICH]), &state(&[], &[A, B, C]), 310);
        assert_eq!(l.images[IMMICH].current().unwrap().id, B);
    }

    #[test]
    fn gone_versions_keep_digest_then_are_forgotten() {
        let mut l = Ledger::default();
        let p = plan(&[IMMICH]);
        reconcile(&mut l, &p, &state(&[(IMMICH, A)], &[A]), 0);
        reconcile(&mut l, &p, &state(&[(IMMICH, B)], &[A, B]), DAY);
        // A deleted by GC.
        reconcile(&mut l, &p, &state(&[(IMMICH, B)], &[B]), 2 * DAY);
        let old = &l.images[IMMICH].versions[1];
        assert!(!old.local);
        assert!(old.digest.is_some());
        // A year after it was closed.
        reconcile(
            &mut l,
            &p,
            &state(&[(IMMICH, B)], &[B]),
            DAY + FORGET_AFTER + 1,
        );
        assert_eq!(l.images[IMMICH].versions.len(), 1);
    }

    #[test]
    fn gone_without_digest_is_dropped() {
        let mut l = Ledger::default();
        let p = plan(&["local/app:dev"]);
        let mut s = State::default();
        s.refs.insert(
            "local/app:dev".into(),
            LocalImage {
                id: A.into(),
                repo_digests: vec![],
            },
        );
        s.present = Some([A.to_string()].into());
        reconcile(&mut l, &p, &s, 0);
        s.refs.insert(
            "local/app:dev".into(),
            LocalImage {
                id: B.into(),
                repo_digests: vec![],
            },
        );
        s.present = Some([B.to_string()].into());
        reconcile(&mut l, &p, &s, 10);
        assert_eq!(l.images["local/app:dev"].versions.len(), 1);
    }

    #[test]
    fn keep_current_and_young_holdbacks() {
        let mut l = Ledger::default();
        let p = plan(&[IMMICH, "redis:7"]);
        reconcile(
            &mut l,
            &p,
            &state(&[(IMMICH, A), ("redis:7", C)], &[A, C]),
            0,
        );
        reconcile(
            &mut l,
            &p,
            &state(&[(IMMICH, B), ("redis:7", C)], &[A, B, C]),
            10 * DAY,
        );
        let k = keep_set(&l, 20 * DAY, 30 * DAY);
        let ids: Vec<_> = k.iter().map(|k| (k.id.as_str(), &k.kind)).collect();
        assert!(ids.contains(&(A, &KeepKind::Holdback(history_tag(IMMICH, A)))));
        assert!(ids.contains(&(B, &KeepKind::Current)));
        assert!(ids.contains(&(C, &KeepKind::Current)));
        // After the age passed, A is no longer kept.
        let k = keep_set(&l, 41 * DAY, 30 * DAY);
        assert!(!k.iter().any(|k| k.id == A));
    }

    #[test]
    fn id_current_elsewhere_is_not_holdback() {
        let mut l = Ledger::default();
        let p = plan(&["a:1", "b:1"]);
        reconcile(&mut l, &p, &state(&[("a:1", A), ("b:1", B)], &[A, B]), 0);
        reconcile(&mut l, &p, &state(&[("a:1", B), ("b:1", B)], &[A, B]), 10);
        let k = keep_set(&l, 20, 1000);
        assert!(k
            .iter()
            .any(|k| k.id == A && matches!(k.kind, KeepKind::Holdback(_))));
        assert!(!k
            .iter()
            .any(|k| k.id == B && matches!(k.kind, KeepKind::Holdback(_))));
    }

    #[test]
    fn select_at_and_previous() {
        let mut l = Ledger::default();
        let p = plan(&[IMMICH, "redis:7"]);
        reconcile(
            &mut l,
            &p,
            &state(&[(IMMICH, A), ("redis:7", C)], &[A, C]),
            100,
        );
        reconcile(
            &mut l,
            &p,
            &state(&[(IMMICH, B), ("redis:7", C)], &[A, B, C]),
            200,
        );

        let picks = select(&[l.clone()], Target::At(150), &[], &[]);
        assert_eq!(picks.len(), 1, "redis ran C then and now: left out");
        assert_eq!(picks[0].reference, IMMICH);
        assert_eq!(picks[0].version.id, A);
        assert!(picks[0].version.local);

        // Now = current: nothing to do.
        assert!(select(&[l.clone()], Target::At(250), &[], &[]).is_empty());
        // Before anything was recorded: nothing.
        assert!(select(&[l.clone()], Target::At(50), &[], &[]).is_empty());

        let prev = select(&[l.clone()], Target::Previous, &[IMMICH.to_string()], &[]);
        assert_eq!(prev[0].version.id, A);
        assert!(select(&[l.clone()], Target::Previous, &["redis:7".into()], &[]).is_empty());

        // Unit filter.
        let u = l.images[IMMICH].units.clone();
        assert_eq!(select(&[l.clone()], Target::At(150), &[], &u).len(), 1);
        assert!(select(&[l.clone()], Target::At(150), &[], &["docker-other".into()]).is_empty());
    }

    #[test]
    fn select_falls_back_to_second_ledger() {
        let mut old = Ledger::default();
        let p = plan(&[IMMICH]);
        reconcile(&mut old, &p, &state(&[(IMMICH, A)], &[A]), 100);
        let mut main = Ledger::default();
        reconcile(&mut main, &p, &state(&[(IMMICH, B)], &[B]), 5000);
        let picks = select(&[main, old], Target::At(200), &[], &[]);
        assert_eq!(picks.len(), 1);
        assert_eq!(picks[0].version.id, A);
        assert!(!picks[0].version.local, "main does not have A on disk");
    }

    #[test]
    fn rollback_then_reconcile_records_old_version_as_current() {
        let mut l = Ledger::default();
        let p = plan(&[IMMICH]);
        reconcile(&mut l, &p, &state(&[(IMMICH, A)], &[A]), 100);
        reconcile(&mut l, &p, &state(&[(IMMICH, B)], &[A, B]), 200);
        // Rolled back: the tag points at A again.
        reconcile(&mut l, &p, &state(&[(IMMICH, A)], &[A, B]), 300);
        let e = &l.images[IMMICH];
        assert_eq!(e.current().unwrap().id, A);
        assert_eq!(e.versions[1].id, B);
        // Back at 250, B ran.
        let picks = select(&[l.clone()], Target::At(250), &[], &[]);
        assert_eq!(picks[0].version.id, B);
    }

    #[test]
    fn ledger_json_shape() {
        let json = r#"{"updated_at":"2026-11-18T04:12:00Z","images":{"x:1":{"units":["docker-x"],"versions":[{"id":"sha256:ab","digest":null,"from":"2026-11-16T04:03:11Z","until":null}]}}}"#;
        let l: Ledger = serde_json::from_str(json).unwrap();
        assert!(l.images["x:1"].versions[0].local, "local defaults to true");
    }
}
