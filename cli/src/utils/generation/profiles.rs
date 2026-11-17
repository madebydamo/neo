//! The system profile's generations (`system-N-link`) and which one is the
//! boot default or running.

use anyhow::Result;
use serde::Serialize;
use std::path::Path;
use std::process::Command;

use super::{PROFILES_DIR, RUNNING_SYSTEM, SYSTEM_PROFILE};

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

fn run_list_generations(sudo_cmd: Option<&str>) -> Result<String> {
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
    crate::utils::command::stdout_checked(&mut cmd, "nix-env --list-generations")
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
                message: Some(format!("Could not list generations: {e:#}")),
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
            .map(|m| crate::utils::command::mtime_secs(&m) as i64)
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
pub(super) fn generation_for_path(
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

#[cfg(test)]
pub(super) fn test_link(number: u64, created: i64, target: &str) -> GenerationLink {
    GenerationLink {
        number,
        created,
        target: target.to_string(),
    }
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
    fn shared_store_path() {
        let links = vec![
            test_link(5, 100, "/nix/store/a"),
            test_link(6, 200, "/nix/store/a"),
        ];
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
}
