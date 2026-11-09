use anyhow::{Context, Result};
use clap::ValueEnum;
use std::process::{Command, Stdio};

use crate::utils::command::TOPLEVEL_ATTR;
use crate::utils::{
    format_command, get_current_branch, git_cmd, has_staged_changes, record_generation_in_commit,
    resolve_suffix, run_nix, run_write_flake, OperationKind, OperationLog,
};

/// How `neo activate` applies the new system generation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, ValueEnum)]
pub enum ActivationMode {
    /// Live `nixos-rebuild switch` (default; interactive UI/CLI).
    #[default]
    Switch,
    /// `nixos-rebuild boot`: set boot default only; running system unchanged until reboot.
    Boot,
}

impl ActivationMode {
    pub fn as_str(self) -> &'static str {
        match self {
            ActivationMode::Switch => "switch",
            ActivationMode::Boot => "boot",
        }
    }
}

pub fn activate(
    config_path: &str,
    dry_run: bool,
    nix_cmd: &str,
    sudo_cmd: &str,
    activation_suffix: Option<&str>,
    mode: ActivationMode,
) -> Result<()> {
    let mode_s = mode.as_str();
    if dry_run {
        println!(
            "DRY-RUN: activate mode={mode_s} (write-flake + toplevel build + git branch dance + pre-clean + nixos-rebuild {mode_s}; exit 0 or 4 treated as success (keep branch, 'Activated using...'); other non-zero keeps branch but errors)"
        );
        return Ok(());
    }

    let suffix = resolve_suffix(activation_suffix, "NEO_ACTIVATION_SUFFIX");
    let op = OperationLog::new(OperationKind::Activation, &suffix)
        .with_extra(serde_json::json!({ "activation_mode": mode_s }));
    op.write_state("in_progress", "starting", None, None);
    let _tee = op.capture_stdio();

    op.step("write-flake", || run_write_flake(config_path, nix_cmd))?;
    op.write_state("in_progress", "write-flake-done", None, None);

    op.step("toplevel-build", || {
        run_nix(config_path, nix_cmd, &["build", TOPLEVEL_ATTR])
    })?;
    op.write_state("in_progress", "toplevel-built", None, None);

    let branches = prepare_branches(config_path, &op, &suffix)?;
    let activation_branch = branches.activation.as_str();
    let has_changes = branches.has_changes;

    op.write_state("in_progress", "pre-rebuild", None, Some(activation_branch));
    clear_stale_rebuild_unit(config_path, sudo_cmd);
    let mut rebuild = Command::new(sudo_cmd);
    rebuild
        .current_dir(config_path)
        .args(["nixos-rebuild", mode_s, "--flake", ".#neo"]);
    let display = format_command(&rebuild);
    println!("→ {display}");
    let status = rebuild
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .with_context(|| format!("failed to spawn: {display}"))?;
    let code = status.code().unwrap_or(-1);

    // Exit 4: the rebuild succeeded with warnings (common for user session reloads on switch).
    let warnings = code == 4;
    branches.drop_build_branch(config_path);
    if !status.success() && !warnings {
        println!(
            "nixos-rebuild failed (non-zero exit {}). Keeping {} branch+checkout (check 'systemctl --failed' and logs).",
            code, activation_branch
        );
        op.write_state(
            "failed",
            "rebuild-failed",
            Some(&format!("exit code {}", code)),
            None,
        );
        anyhow::bail!("Command failed: {display} (exit {code})");
    }
    if warnings {
        match mode {
            ActivationMode::Switch => {
                println!(
                    "warning: nixos-rebuild exited 4 (success with warnings). The switch succeeded and the new generation is active. This is common for user session reloads (dbus-broker), non-critical service restarts, etc. Treating as success for activation tracking."
                );
            }
            ActivationMode::Boot => {
                println!(
                    "warning: nixos-rebuild exited 4 (success with warnings). Boot default was set; reboot to run the new generation. Treating as success for activation tracking."
                );
            }
        }
    }
    let phase = if warnings {
        "completed-with-warnings"
    } else {
        "completed"
    };
    // reboot_pending: boot mode leaves the profile pointer ahead of /run/current-system.
    let reboot_pending = mode == ActivationMode::Boot;
    op.write_state_extra(
        "success",
        phase,
        None,
        Some(activation_branch),
        Some(serde_json::json!({ "reboot_pending": reboot_pending })),
    );
    record_gen_after_activate(config_path, activation_branch, has_changes);
    match mode {
        ActivationMode::Switch => {
            println!(
                "Activated using branch {}{}",
                activation_branch,
                if warnings {
                    " (exit code 4 / warnings)"
                } else {
                    ""
                }
            );
        }
        ActivationMode::Boot => {
            println!(
                "Boot default set using branch {}{}; reboot to run the new generation",
                activation_branch,
                if warnings {
                    " (exit code 4 / warnings)"
                } else {
                    ""
                },
            );
        }
    }
    Ok(())
}

/// The branches one activation commits to.
struct Branches {
    activation: String,
    /// Only carries the commit over to the activation branch.
    build: String,
    has_changes: bool,
}

impl Branches {
    fn drop_build_branch(&self, config_path: &str) {
        if self.has_changes {
            let _ = git_cmd(config_path, &["branch", "-D", &self.build]);
        }
    }
}

/// Commit the working tree (when it changed) and check out the activation branch.
fn prepare_branches(config_path: &str, op: &OperationLog, suffix: &str) -> Result<Branches> {
    let orig_branch = get_current_branch(config_path).unwrap_or_else(|_| "master".to_string());
    op.step("git-add", || git_cmd(config_path, &["add", "."]))?;
    let branches = Branches {
        activation: format!("activation_{suffix}"),
        build: format!("build_{suffix}"),
        has_changes: has_staged_changes(config_path),
    };
    if branches.has_changes {
        op.step("build-branch", || {
            git_cmd(config_path, &["switch", "-C", &branches.build])
        })?;
        op.step("build-commit", || {
            git_cmd(config_path, &["commit", "-m", &format!("Build: {suffix}")])
        })?;
    }

    if let Err(e) = git_cmd(config_path, &["switch", "-C", &branches.activation]) {
        branches.drop_build_branch(config_path);
        let _ = git_cmd(config_path, &["switch", &orig_branch]);
        op.write_state("failed", "branch-failed", Some(&format!("{e:#}")), None);
        return Err(e);
    }
    op.write_state(
        "in_progress",
        "branches-created",
        None,
        Some(&branches.activation),
    );

    if branches.has_changes {
        op.step("amend-add", || git_cmd(config_path, &["add", "."]))?;
        op.step("amend-commit", || {
            git_cmd(
                config_path,
                &[
                    "commit",
                    "--amend",
                    "-m",
                    &format!("Activation: {}", branches.activation),
                ],
            )
        })?;
    }
    Ok(branches)
}

/// Best-effort clear of a leftover nixos-rebuild transient unit. The unit is
/// only loaded while a switch is in flight (or leftover failed); when it is
/// not loaded, systemctl prints "Unit … not loaded" on stderr. Status is
/// already ignored — also silence stdio so those lines do not pollute the
/// activation log/UI on every successful activate.
fn clear_stale_rebuild_unit(config_path: &str, sudo_cmd: &str) {
    let rebuild_unit = "nixos-rebuild-switch-to-configuration.service";
    let unit_loaded = Command::new(sudo_cmd)
        .current_dir(config_path)
        .args([
            "systemctl",
            "show",
            "-p",
            "LoadState",
            "--value",
            rebuild_unit,
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .is_some_and(|s| s.trim() == "loaded");
    if unit_loaded {
        for action in ["reset-failed", "stop"] {
            let _ = Command::new(sudo_cmd)
                .current_dir(config_path)
                .args(["systemctl", action, rebuild_unit])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
    }
}

/// Embed generation number in the activation commit message (no sidecar files).
/// `has_activation_commit` is true when this run created/amended a real Activation commit
/// (dirty tree); otherwise we add an empty commit so re-activates still get history + gen.
fn record_gen_after_activate(
    config_path: &str,
    activation_branch: &str,
    has_activation_commit: bool,
) {
    match record_generation_in_commit(config_path, activation_branch, has_activation_commit) {
        Ok(gen) => {
            println!(
                "Recorded generation {} on {} commit for {}",
                gen,
                if has_activation_commit {
                    "amended"
                } else {
                    "empty"
                },
                activation_branch
            );
        }
        Err(e) => {
            eprintln!("warning: could not record generation in commit: {e:#}");
        }
    }
}
