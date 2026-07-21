use anyhow::{Context, Result};
use std::process::{Command, Stdio};

use crate::commands::log::{resolve_suffix, OperationLog};
use crate::commands::{format_command, get_current_branch, git_cmd, has_staged_changes, run_nix};

pub fn activate(
    config_path: &str,
    dry_run: bool,
    nix_cmd: &str,
    sudo_cmd: &str,
    activation_suffix: Option<&str>,
) -> Result<()> {
    if dry_run {
        println!(
            "DRY-RUN: activate (write-flake + toplevel build + git branch dance + pre-clean + nixos-rebuild; exit 0 or 4 treated as success (keep branch, 'Activated using...'); other non-zero keeps branch but errors)"
        );
        return Ok(());
    }

    let suffix = resolve_suffix(activation_suffix, "NEO_ACTIVATION_SUFFIX");
    let op = OperationLog::new_activation(&suffix);
    op.write_state("in_progress", "starting", None, None);

    op.step("write-flake", || {
        run_nix(config_path, nix_cmd, &["run", ".#write-flake"])
    })?;
    op.write_state("in_progress", "write-flake-done", None, None);

    op.step("toplevel-build", || {
        run_nix(
            config_path,
            nix_cmd,
            &[
                "build",
                ".#nixosConfigurations.neo.config.system.build.toplevel",
            ],
        )
    })?;
    op.write_state("in_progress", "toplevel-built", None, None);

    let activation_branch = format!("activation_{}", suffix);
    let build_branch = format!("build_{}", suffix);
    let orig_branch = get_current_branch(config_path).unwrap_or_else(|_| "master".to_string());

    op.step("git-add", || git_cmd(config_path, &["add", "."]))?;
    let has_changes = has_staged_changes(config_path);
    if has_changes {
        op.step("build-branch", || {
            git_cmd(config_path, &["switch", "-C", &build_branch])
        })?;
        op.step("build-commit", || {
            git_cmd(
                config_path,
                &["commit", "-m", &format!("Build: {}", suffix)],
            )
        })?;
    }

    if let Err(e) = git_cmd(config_path, &["switch", "-C", &activation_branch]) {
        if has_changes {
            let _ = git_cmd(config_path, &["branch", "-D", &build_branch]);
        }
        let _ = git_cmd(config_path, &["switch", &orig_branch]);
        op.write_state("failed", "branch-failed", Some(&e.to_string()), None);
        return Err(e);
    }
    op.write_state(
        "in_progress",
        "branches-created",
        None,
        Some(&activation_branch),
    );

    if has_changes {
        op.step("amend-add", || git_cmd(config_path, &["add", "."]))?;
        op.step("amend-commit", || {
            git_cmd(
                config_path,
                &[
                    "commit",
                    "--amend",
                    "-m",
                    &format!("Activation: {}", activation_branch),
                ],
            )
        })?;
    }

    op.write_state("in_progress", "pre-rebuild", None, Some(&activation_branch));
    let _ = Command::new(sudo_cmd)
        .current_dir(config_path)
        .args([
            "systemctl",
            "reset-failed",
            "nixos-rebuild-switch-to-configuration.service",
        ])
        .status();
    let _ = Command::new(sudo_cmd)
        .current_dir(config_path)
        .args([
            "systemctl",
            "stop",
            "nixos-rebuild-switch-to-configuration.service",
        ])
        .status();
    let mut rebuild = Command::new(sudo_cmd);
    rebuild
        .current_dir(config_path)
        .args(["nixos-rebuild", "switch", "--flake", ".#neo"]);
    let display = format_command(&rebuild);
    println!("→ {display}");
    let status = rebuild
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .with_context(|| format!("failed to spawn: {display}"))?;
    let code = status.code().unwrap_or(-1);

    if code == 4 {
        println!(
            "warning: nixos-rebuild exited 4 (success with warnings). The switch succeeded and the new generation is active. This is common for user session reloads (dbus-broker), non-critical service restarts, etc. Treating as success for activation tracking."
        );
        if has_changes {
            let _ = git_cmd(config_path, &["branch", "-D", &build_branch]);
        }
        op.write_state(
            "success",
            "completed-with-warnings",
            None,
            Some(&activation_branch),
        );
        println!(
            "Activated using branch {} (exit code 4 / warnings)",
            activation_branch
        );
        return Ok(());
    }

    if !status.success() {
        println!(
            "nixos-rebuild failed (non-zero exit {}). Keeping {} branch+checkout (check 'systemctl --failed' and logs).",
            code, activation_branch
        );
        if has_changes {
            let _ = git_cmd(config_path, &["branch", "-D", &build_branch]);
        }
        op.write_state(
            "failed",
            "rebuild-failed",
            Some(&format!("exit code {}", code)),
            None,
        );
        anyhow::bail!("Command failed: {display} (exit {code})");
    }

    if has_changes {
        let _ = git_cmd(config_path, &["branch", "-D", &build_branch]);
    }
    op.write_state("success", "completed", None, Some(&activation_branch));
    println!("Activated using branch {}", activation_branch);
    Ok(())
}
