//! List and switch NixOS system generations (profile under `/nix/var/nix/profiles/system`).
//! Activation commits record generation in the commit message (`(generation N)`).

use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::UNIX_EPOCH;

const SYSTEM_PROFILE: &str = "/nix/var/nix/profiles/system";
const PROFILES_DIR: &str = "/nix/var/nix/profiles";
const RUNNING_SYSTEM: &str = "/run/current-system";
/// `<epoch> <system store path>` per activation (boot, switch, test); written by
/// `system.activationScripts.neo-system-history` (nix/modules/core/base.nix).
/// Lives on the root filesystem, so a Neo data restore does not rewind it.
pub const ACTIVATION_LOG: &str = "/var/lib/neo/system-activations";

#[derive(Serialize, Clone, Debug)]
pub struct SystemGeneration {
    pub number: u64,
    /// Display date from `nix-env --list-generations` (24h, e.g. `2026-05-12 12:50:29`).
    pub date: String,
    #[serde(rename = "isCurrent")]
    pub is_current: bool,
    /// System running right now (`/run/current-system`); differs from
    /// `is_current` (boot default) after `switch-to-configuration boot`.
    #[serde(rename = "isRunning")]
    pub is_running: bool,
    /// Profile path for display.
    pub path: String,
}

#[derive(Serialize, Clone, Debug)]
pub struct GenerationsList {
    pub generations: Vec<SystemGeneration>,
    /// True when `/nix/var/nix/profiles/system` is missing (e.g. local/dev).
    #[serde(rename = "unavailable")]
    pub unavailable: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GenerationMode {
    Switch,
    Boot,
}

impl GenerationMode {
    /// `switch-to-configuration` action (also the CLI subcommand).
    pub fn as_str(self) -> &'static str {
        match self {
            GenerationMode::Switch => "switch",
            GenerationMode::Boot => "boot",
        }
    }
}

fn parse_generation_from_link(link: &Path) -> Option<u64> {
    let name = link.file_name()?.to_str()?;
    let name = name.strip_prefix("system-")?.strip_suffix("-link")?;
    name.parse().ok()
}

/// True when this machine has a NixOS system profile (not local/dev).
pub fn system_profile_available() -> bool {
    let profile = Path::new(SYSTEM_PROFILE);
    profile.exists() || profile.is_symlink()
}

/// Current system generation number, if resolvable.
pub fn current_generation_number() -> Option<u64> {
    let link = std::fs::read_link(SYSTEM_PROFILE).ok()?;
    parse_generation_from_link(&link)
}

/// Parse a single line of `nix-env --list-generations` output.
/// Examples:
///   ` 211   2026-05-12 12:50:29`
///   ` 221   2026-07-15 16:09:01   (current)`
fn parse_list_generations_line(line: &str) -> Option<SystemGeneration> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }
    let is_current = line.contains("(current)");
    let cleaned = line.replace("(current)", "");
    let mut parts = cleaned.split_whitespace();
    let number: u64 = parts.next()?.parse().ok()?;
    let date_part = parts.next()?; // YYYY-MM-DD
    let time_part = parts.next().unwrap_or("00:00:00");
    let date = format!("{date_part} {time_part}");
    Some(SystemGeneration {
        number,
        date,
        is_current,
        is_running: false,
        path: format!("/nix/var/nix/profiles/system-{}-link", number),
    })
}

fn run_list_generations(sudo_cmd: Option<&str>) -> Result<String, String> {
    let mut cmd = if let Some(sudo) = sudo_cmd {
        let mut c = Command::new(sudo);
        c.args([
            "-n",
            "nix-env",
            "--list-generations",
            "--profile",
            SYSTEM_PROFILE,
        ]);
        c
    } else {
        let mut c = Command::new("nix-env");
        c.args(["--list-generations", "--profile", SYSTEM_PROFILE]);
        c
    };
    let o = cmd
        .output()
        .map_err(|e| format!("spawn nix-env --list-generations: {e}"))?;
    if !o.status.success() {
        let err = String::from_utf8_lossy(&o.stderr);
        return Err(if err.trim().is_empty() {
            format!(
                "nix-env --list-generations failed (exit {:?})",
                o.status.code()
            )
        } else {
            err.trim().to_string()
        });
    }
    Ok(String::from_utf8_lossy(&o.stdout).into_owned())
}

/// List system profile generations via `nix-env --list-generations` (correct dates).
/// Tries without sudo first, then `sudo -n` (web / homeserver path).
pub fn list_system_generations(sudo_cmd: &str) -> GenerationsList {
    if !system_profile_available() {
        return GenerationsList {
            generations: vec![],
            unavailable: true,
            message: Some(
                "No system profile at /nix/var/nix/profiles/system (local/dev or non-NixOS)."
                    .to_string(),
            ),
        };
    }

    let raw = match run_list_generations(None).or_else(|_| run_list_generations(Some(sudo_cmd))) {
        Ok(s) => s,
        Err(e) => {
            return GenerationsList {
                generations: vec![],
                unavailable: true,
                message: Some(format!("Could not list generations: {e}")),
            };
        }
    };

    let mut generations: Vec<SystemGeneration> = raw
        .lines()
        .filter_map(parse_list_generations_line)
        .collect();

    // Newest first (match previous UI).
    generations.sort_by_key(|g| std::cmp::Reverse(g.number));

    // Ensure current marker if nix-env omitted it but profile points somewhere.
    if !generations.iter().any(|g| g.is_current) {
        if let Some(cur) = current_generation_number() {
            for g in &mut generations {
                g.is_current = g.number == cur;
            }
        }
    }

    if let Some(run) = running_generation_number() {
        for g in &mut generations {
            g.is_running = g.number == run;
        }
    }

    GenerationsList {
        generations,
        unavailable: false,
        message: None,
    }
}

/// A `system-N-link` in the profiles dir.
#[derive(Clone, Debug, PartialEq)]
pub struct GenerationLink {
    pub number: u64,
    /// Creation time (link mtime, epoch seconds).
    pub created: i64,
    /// System store path the generation points at.
    pub target: String,
}

/// All surviving system generations, sorted by number.
pub fn generation_links() -> Vec<GenerationLink> {
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(PROFILES_DIR) else {
        return out;
    };
    for e in rd.flatten() {
        let Some(number) = parse_generation_from_link(&e.path()) else {
            continue;
        };
        let Ok(target) = std::fs::read_link(e.path()) else {
            continue;
        };
        let created = std::fs::symlink_metadata(e.path())
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|m| m.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        out.push(GenerationLink {
            number,
            created,
            target: target.to_string_lossy().into_owned(),
        });
    }
    out.sort_by_key(|l| l.number);
    out
}

/// Generation for a system store path. Several generations can share a path
/// (identical rebuilds): `prefer` wins if it matches, else the newest one that
/// existed at `at`, else the oldest.
fn generation_for_path(
    links: &[GenerationLink],
    path: &str,
    prefer: Option<u64>,
    at: Option<i64>,
) -> Option<u64> {
    let path = path.trim_end_matches('/');
    let matches: Vec<&GenerationLink> = links.iter().filter(|l| l.target == path).collect();
    if let Some(p) = prefer.filter(|p| matches.iter().any(|l| l.number == *p)) {
        return Some(p);
    }
    let existing = matches
        .iter()
        .filter(|l| at.is_none_or(|t| l.created <= t))
        .map(|l| l.number)
        .max();
    existing.or_else(|| matches.iter().map(|l| l.number).min())
}

/// Generation whose system is running now (`/run/current-system`). Unlike
/// [`current_generation_number`] (the boot default) this stays on the old
/// generation after `switch-to-configuration boot` until the reboot.
pub fn running_generation_number() -> Option<u64> {
    let running = std::fs::read_link(RUNNING_SYSTEM).ok()?;
    generation_for_path(
        &generation_links(),
        running.to_str()?,
        current_generation_number(),
        None,
    )
}

/// Which system generation was running when. Built from activation records
/// (the Neo activation log, and the journal: `switching to system
/// configuration …` per switch/test, `init=…` on each boot's kernel command
/// line). Before the first record it falls back to "newest generation created
/// by then", which cannot see rollbacks.
#[derive(Debug, Default)]
pub struct GenerationTimeline {
    links: Vec<GenerationLink>,
    /// `(epoch, generation)` sorted by time; `None`: that system no longer has a
    /// generation (deleted) or never had one (`switch-to-configuration test`).
    events: Vec<(i64, Option<u64>)>,
}

impl GenerationTimeline {
    pub fn load(sudo_cmd: &str) -> Self {
        let links = generation_links();
        let mut records = read_activation_log();
        records.extend(journal_activations(sudo_cmd));
        Self::from_records(links, records)
    }

    fn from_records(links: Vec<GenerationLink>, mut records: Vec<(i64, String)>) -> Self {
        records.sort();
        let events = records
            .into_iter()
            .map(|(t, path)| (t, generation_for_path(&links, &path, None, Some(t))))
            .collect();
        Self { links, events }
    }

    /// `(epoch, generation)` activation records, oldest first.
    pub fn events(&self) -> &[(i64, Option<u64>)] {
        &self.events
    }

    /// No generations on this machine (dev VM, non-NixOS).
    pub fn is_empty(&self) -> bool {
        self.links.is_empty()
    }

    /// Generation that was running at `t` (epoch seconds).
    pub fn at(&self, t: i64) -> Option<u64> {
        if let Some((_, g)) = self.events.iter().rev().find(|(et, _)| *et <= t) {
            return *g;
        }
        self.links
            .iter()
            .filter(|l| l.created <= t)
            .max_by_key(|l| (l.created, l.number))
            .map(|l| l.number)
    }
}

fn read_activation_log() -> Vec<(i64, String)> {
    std::fs::read_to_string(ACTIVATION_LOG)
        .map(|s| s.lines().filter_map(parse_activation_log_line).collect())
        .unwrap_or_default()
}

/// `1790611135 /nix/store/…-nixos-system-…`
fn parse_activation_log_line(line: &str) -> Option<(i64, String)> {
    let (t, path) = line.trim().split_once(' ')?;
    let path = path.trim();
    path.starts_with("/nix/store/")
        .then(|| Some((t.parse().ok()?, path.to_string())))?
}

fn journal_lines(sudo_cmd: &str, args: &[&str]) -> Vec<String> {
    let out = Command::new(sudo_cmd)
        .args(["-n", "journalctl", "--no-pager", "-q", "-o", "short-unix"])
        .args(args)
        .output();
    match out {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout)
            .lines()
            .map(str::to_string)
            .collect(),
        _ => Vec::new(),
    }
}

/// Switches and boots recorded in the journal (as far back as it reaches).
fn journal_activations(sudo_cmd: &str) -> Vec<(i64, String)> {
    let mut out = Vec::new();
    for line in journal_lines(
        sudo_cmd,
        &[
            "SYSLOG_IDENTIFIER=nixos",
            "-g",
            "^switching to system configuration ",
        ],
    ) {
        out.extend(parse_journal_switch(&line));
    }
    for line in journal_lines(
        sudo_cmd,
        &["_TRANSPORT=kernel", "-g", "^Command line: .*init="],
    ) {
        out.extend(parse_journal_boot(&line));
    }
    out
}

fn journal_epoch(line: &str) -> Option<i64> {
    let ts = line.split_whitespace().next()?;
    ts.split('.').next()?.parse().ok()
}

/// `1790611135.419086 orion nixos[3441407]: switching to system configuration /nix/store/…`
fn parse_journal_switch(line: &str) -> Option<(i64, String)> {
    let path = line
        .split_once("switching to system configuration ")?
        .1
        .trim();
    path.starts_with("/nix/store/")
        .then(|| Some((journal_epoch(line)?, path.to_string())))?
}

/// `1790612419.783699 orion kernel: Command line: … init=/nix/store/…/init …`
fn parse_journal_boot(line: &str) -> Option<(i64, String)> {
    let init = line
        .split_whitespace()
        .find_map(|w| w.strip_prefix("init="))?;
    let path = init.strip_suffix("/init")?;
    path.starts_with("/nix/store/")
        .then(|| Some((journal_epoch(line)?, path.to_string())))?
}

/// Switch or set boot default for generation `n`.
/// Uses shell: `nix-env --switch-generation` + generation's `switch-to-configuration`.
///
/// **Must not run inside the neo-web process** for live `switch`: the activation
/// stops `neo-web.service`. Web UI triggers this via `systemd-run` oneshot.
pub fn switch_system_generation(
    n: u64,
    mode: GenerationMode,
    sudo_cmd: &str,
) -> Result<(), String> {
    let link = PathBuf::from(format!("/nix/var/nix/profiles/system-{}-link", n));
    if !link.exists() && !link.is_symlink() {
        return Err(format!("generation {} not found ({})", n, link.display()));
    }

    // Check before touching the profile: a failure must not leave it moved
    // to a generation whose bootloader entry / activation never ran.
    let stc = link.join("bin/switch-to-configuration");
    if !stc.exists() {
        return Err(format!(
            "switch-to-configuration missing for generation {n} (store path gone?)"
        ));
    }

    // Point the system profile at this generation (boot default + current pointer).
    let status = Command::new(sudo_cmd)
        .args([
            "-n",
            "nix-env",
            "-p",
            SYSTEM_PROFILE,
            "--switch-generation",
            &n.to_string(),
        ])
        .status()
        .map_err(|e| format!("spawn nix-env: {e}"))?;
    if !status.success() {
        return Err(format!(
            "nix-env --switch-generation {} failed (exit {:?})",
            n,
            status.code()
        ));
    }

    let action = mode.as_str();
    let stc_s = stc.to_string_lossy();
    println!("→ {sudo_cmd} -n {stc_s} {action}");
    let status = Command::new(sudo_cmd)
        .args(["-n", stc_s.as_ref(), action])
        .status()
        .map_err(|e| format!("spawn switch-to-configuration: {e}"))?;
    if !status.success() {
        return Err(format!(
            "switch-to-configuration {} failed (exit {:?})",
            action,
            status.code()
        ));
    }
    Ok(())
}

/// Build activation commit subject including optional generation.
/// Example: `Activation: activation_20260721-123033 (generation 221)`
pub fn activation_commit_message(activation_branch: &str, generation: Option<u64>) -> String {
    match generation {
        Some(n) => format!("Activation: {activation_branch} (generation {n})"),
        None => format!("Activation: {activation_branch}"),
    }
}

/// Parse `Neo-Generation` / `(generation N)` from a commit subject or body.
pub fn parse_generation_from_message(text: &str) -> Option<u64> {
    // Prefer explicit trailer
    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line
            .strip_prefix("Neo-Generation:")
            .or_else(|| line.strip_prefix("neo-generation:"))
        {
            if let Ok(n) = rest.trim().parse::<u64>() {
                if n > 0 {
                    return Some(n);
                }
            }
        }
    }
    // Subject form: `(generation 221)`
    let lower = text.to_ascii_lowercase();
    if let Some(idx) = lower.find("(generation ") {
        let rest = &text[idx + "(generation ".len()..];
        let num: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
        if let Ok(n) = num.parse::<u64>() {
            if n > 0 {
                return Some(n);
            }
        }
    }
    None
}

/// After a successful rebuild, record generation on the activation commit (message only).
/// - If `has_activation_commit` (we created/amended an Activation commit this run): amend message.
/// - Else: empty commit so re-activates of the same tree still get a history node with gen.
pub fn record_generation_in_commit(
    config_path: &str,
    activation_branch: &str,
    has_activation_commit: bool,
) -> Result<u64, String> {
    let Some(gen) = current_generation_number() else {
        return Err("could not resolve current system generation".to_string());
    };
    let msg = activation_commit_message(activation_branch, Some(gen));
    let mut cmd = Command::new("git");
    cmd.current_dir(config_path).arg("commit");
    if has_activation_commit {
        cmd.args(["--amend", "-m", &msg]);
    } else {
        // Same config re-activated, tree unchanged — still record gen on a new history node.
        cmd.args(["--allow-empty", "-m", &msg]);
    }
    let status = cmd.status().map_err(|e| format!("git commit: {e}"))?;
    if !status.success() {
        return Err(format!(
            "git commit failed while recording generation (exit {:?})",
            status.code()
        ));
    }
    Ok(gen)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_nix_env_list_line() {
        let g = parse_list_generations_line(" 211   2026-05-12 12:50:29").unwrap();
        assert_eq!(g.number, 211);
        assert_eq!(g.date, "2026-05-12 12:50:29");
        assert!(!g.is_current);

        let cur = parse_list_generations_line(" 221   2026-07-15 16:09:01   (current)").unwrap();
        assert_eq!(cur.number, 221);
        assert_eq!(cur.date, "2026-07-15 16:09:01");
        assert!(cur.is_current);
    }

    #[test]
    fn parse_generation_from_subject() {
        assert_eq!(
            parse_generation_from_message(
                "Activation: activation_20260721-123033 (generation 221)"
            ),
            Some(221)
        );
        assert_eq!(
            parse_generation_from_message("Activation: foo\n\nNeo-Generation: 42\n"),
            Some(42)
        );
        assert_eq!(parse_generation_from_message("Activation: foo"), None);
    }

    fn link(number: u64, created: i64, target: &str) -> GenerationLink {
        GenerationLink {
            number,
            created,
            target: target.to_string(),
        }
    }

    #[test]
    fn timeline_follows_activations() {
        let links = vec![
            link(10, 100, "/nix/store/a"),
            link(12, 300, "/nix/store/c"),
            link(13, 400, "/nix/store/d"),
        ];
        // 12 switched at 300, 13 at 400, reboot into 12 at 500; /nix/store/x is gone.
        let records = vec![
            (500, "/nix/store/c".to_string()),
            (300, "/nix/store/c".to_string()),
            (400, "/nix/store/d".to_string()),
            (250, "/nix/store/x".to_string()),
        ];
        let tl = GenerationTimeline::from_records(links, records);
        assert_eq!(tl.at(150), Some(10)); // before any record: creation time
        assert_eq!(tl.at(260), None);
        assert_eq!(tl.at(350), Some(12));
        assert_eq!(tl.at(450), Some(13));
        assert_eq!(tl.at(600), Some(12)); // creation time alone would say 13
    }

    #[test]
    fn timeline_without_records_uses_creation_time() {
        let links = vec![
            link(10, 100, "/a"),
            link(11, 200, "/b"),
            link(12, 300, "/c"),
        ];
        let tl = GenerationTimeline::from_records(links, vec![]);
        assert_eq!(tl.at(50), None);
        assert_eq!(tl.at(200), Some(11));
        assert_eq!(tl.at(250), Some(11));
        assert_eq!(tl.at(1000), Some(12));
    }

    #[test]
    fn shared_store_path() {
        let links = vec![link(5, 100, "/nix/store/a"), link(6, 200, "/nix/store/a")];
        assert_eq!(
            generation_for_path(&links, "/nix/store/a", None, Some(150)),
            Some(5)
        );
        assert_eq!(
            generation_for_path(&links, "/nix/store/a", None, Some(250)),
            Some(6)
        );
        assert_eq!(
            generation_for_path(&links, "/nix/store/a", None, Some(50)),
            Some(5)
        );
        assert_eq!(
            generation_for_path(&links, "/nix/store/a", Some(5), None),
            Some(5)
        );
        assert_eq!(
            generation_for_path(&links, "/nix/store/b", None, None),
            None
        );
    }

    #[test]
    fn parse_activation_records() {
        assert_eq!(
            parse_activation_log_line("1790611135 /nix/store/9q-nixos-system"),
            Some((1790611135, "/nix/store/9q-nixos-system".to_string()))
        );
        assert_eq!(parse_activation_log_line("garbage"), None);
        assert_eq!(
            parse_journal_switch("1790611135.419086 orion nixos[3441407]: switching to system configuration /nix/store/9q-nixos-system"),
            Some((1790611135, "/nix/store/9q-nixos-system".to_string()))
        );
        assert_eq!(
            parse_journal_boot("1790612419.783699 orion kernel: Command line: BOOT_IMAGE=(hd0,gpt1)//kernels/k-bzImage init=/nix/store/9q-nixos-system/init nohibernate"),
            Some((1790612419, "/nix/store/9q-nixos-system".to_string()))
        );
    }

    #[test]
    fn activation_message_format() {
        assert_eq!(
            activation_commit_message("activation_x", Some(9)),
            "Activation: activation_x (generation 9)"
        );
    }
}
