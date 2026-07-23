//! List and switch NixOS system generations (profile under `/nix/var/nix/profiles/system`).
//! Activation commits record generation in the commit message (`(generation N)`).

use anyhow::{bail, Context, Result};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::Command;

const SYSTEM_PROFILE: &str = "/nix/var/nix/profiles/system";

#[derive(Serialize, Clone, Debug)]
pub struct SystemGeneration {
    pub number: u64,
    /// Display date from `nix-env --list-generations` (24h, e.g. `2026-05-12 12:50:29`).
    pub date: String,
    #[serde(rename = "isCurrent")]
    pub is_current: bool,
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

fn parse_generation_from_link(link: &Path) -> Option<u64> {
    let name = link.file_name()?.to_str()?;
    let name = name.strip_prefix("system-")?.strip_suffix("-link")?;
    name.parse().ok()
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
pub fn list_system_generations() -> GenerationsList {
    list_system_generations_with_sudo("sudo")
}

/// Same as [`list_system_generations`] but with an explicit sudo binary path.
pub fn list_system_generations_with_sudo(sudo_cmd: &str) -> GenerationsList {
    let profile = Path::new(SYSTEM_PROFILE);
    if !profile.exists() && !profile.is_symlink() {
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
    generations.sort_by(|a, b| b.number.cmp(&a.number));

    // Ensure current marker if nix-env omitted it but profile points somewhere.
    if !generations.iter().any(|g| g.is_current) {
        if let Some(cur) = current_generation_number() {
            for g in &mut generations {
                g.is_current = g.number == cur;
            }
        }
    }

    GenerationsList {
        generations,
        unavailable: false,
        message: None,
    }
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

    // Prefer the generation link path (stable even mid-switch); fall back to profile.
    let stc_gen = link.join("bin/switch-to-configuration");
    let stc_profile = PathBuf::from(SYSTEM_PROFILE).join("bin/switch-to-configuration");
    let stc = if stc_gen.exists() {
        stc_gen
    } else if stc_profile.exists() {
        stc_profile
    } else {
        return Err(format!(
            "switch-to-configuration missing for generation {n}"
        ));
    };
    let action = match mode {
        GenerationMode::Switch => "switch",
        GenerationMode::Boot => "boot",
    };
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

pub fn generation_list(dry_run: bool) -> Result<()> {
    if dry_run {
        println!("DRY-RUN: list system generations");
        return Ok(());
    }
    let list = list_system_generations();
    if list.unavailable {
        if let Some(msg) = list.message {
            println!("{msg}");
        } else {
            println!("System profile unavailable.");
        }
        return Ok(());
    }
    for g in &list.generations {
        let marker = if g.is_current { " (current)" } else { "" };
        println!("{:>4}  {}{}  {}", g.number, g.date, marker, g.path);
    }
    Ok(())
}

/// Run generation switch with optional web op-state tracking (`NEO_GENSWITCH_SUFFIX`).
pub fn generation_switch(n: u64, dry_run: bool, sudo_cmd: &str) -> Result<()> {
    run_generation_op(n, GenerationMode::Switch, dry_run, sudo_cmd)
}

/// Run generation boot with optional web op-state tracking (`NEO_GENSWITCH_SUFFIX`).
pub fn generation_boot(n: u64, dry_run: bool, sudo_cmd: &str) -> Result<()> {
    run_generation_op(n, GenerationMode::Boot, dry_run, sudo_cmd)
}

fn run_generation_op(n: u64, mode: GenerationMode, dry_run: bool, sudo_cmd: &str) -> Result<()> {
    let mode_s = match mode {
        GenerationMode::Switch => "switch",
        GenerationMode::Boot => "boot",
    };
    if dry_run {
        println!("DRY-RUN: generation {mode_s} {n}");
        return Ok(());
    }

    // When triggered from the web UI via systemd-run, track progress under /tmp/neo-activations.
    let op = std::env::var("NEO_GENSWITCH_SUFFIX").ok().map(|suf| {
        let log = crate::commands::log::OperationLog::new_generation(&suf);
        log.write_state_extra(
            "in_progress",
            "starting",
            None,
            None,
            Some(serde_json::json!({ "generation": n, "mode": mode_s })),
        );
        log
    });

    if let Some(ref op) = op {
        op.write_state_extra(
            "in_progress",
            "nix-env-switch-generation",
            None,
            None,
            Some(serde_json::json!({ "generation": n, "mode": mode_s })),
        );
    }

    match switch_system_generation(n, mode, sudo_cmd) {
        Ok(()) => {
            if let Some(ref op) = op {
                op.write_state_extra(
                    "success",
                    "completed",
                    None,
                    None,
                    Some(serde_json::json!({ "generation": n, "mode": mode_s })),
                );
            }
            match mode {
                GenerationMode::Switch => println!("Switched to generation {n}"),
                GenerationMode::Boot => println!("Boot default set to generation {n}"),
            }
            Ok(())
        }
        Err(e) => {
            if let Some(ref op) = op {
                op.write_state_extra(
                    "failed",
                    "switch-failed",
                    Some(&e),
                    None,
                    Some(serde_json::json!({ "generation": n, "mode": mode_s })),
                );
            }
            Err(anyhow::anyhow!(e)).with_context(|| format!("generation {mode_s} {n}"))
        }
    }
}

pub fn generation_help() -> Result<()> {
    eprintln!(
        "Usage:\n  neo generation list\n  neo generation switch <N>\n  neo generation boot <N>"
    );
    bail!("missing generation subcommand");
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

    #[test]
    fn activation_message_format() {
        assert_eq!(
            activation_commit_message("activation_x", Some(9)),
            "Activation: activation_x (generation 9)"
        );
    }
}
