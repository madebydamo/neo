use anyhow::Result;
use std::process::Command;

use crate::commands::{
    execute_command, get_current_branch, get_timestamp, git_cmd, has_staged_changes, run_nix,
};

pub fn activate(config_path: &str, dry_run: bool, nix_cmd: &str, sudo_cmd: &str) -> Result<()> {
    if dry_run {
        println!(
            "DRY-RUN: activate (run_nix write-flake + toplevel build, optional build_xxx+Build-commit if changes, switch -C activation_xxx, optional amend-recommit, nixos-rebuild, cleanup build_ on success or restore+delete branches on fail)"
        );
        return Ok(());
    }
    run_nix(config_path, nix_cmd, &["run", ".#write-flake"])?;
    run_nix(
        config_path,
        nix_cmd,
        &[
            "build",
            ".#nixosConfigurations.neo.config.system.build.toplevel",
        ],
    )?;

    let suffix = get_timestamp();
    let activation_branch = format!("activation_{}", suffix);
    let build_branch = format!("build_{}", suffix);
    let orig_branch = get_current_branch(config_path).unwrap_or_else(|_| "master".to_string());

    git_cmd(config_path, &["add", "."])?;
    let has_changes = has_staged_changes(config_path);
    if has_changes {
        git_cmd(config_path, &["switch", "-C", &build_branch])?;
        git_cmd(
            config_path,
            &["commit", "-m", &format!("Build: {}", suffix)],
        )?;
    }

    if let Err(e) = git_cmd(config_path, &["switch", "-C", &activation_branch]) {
        if has_changes {
            let _ = git_cmd(config_path, &["branch", "-D", &build_branch]);
        }
        let _ = git_cmd(config_path, &["switch", &orig_branch]);
        return Err(e);
    }

    if has_changes {
        git_cmd(config_path, &["add", "."])?;
        git_cmd(
            config_path,
            &[
                "commit",
                "--amend",
                "-m",
                &format!("Activation: {}", activation_branch),
            ],
        )?;
    }

    let desc = format!(
        "{} nixos-rebuild switch --flake .#neo (in {})",
        sudo_cmd, config_path
    );
    if let Err(e) = execute_command(
        Command::new(sudo_cmd).current_dir(config_path).args([
            "nixos-rebuild",
            "switch",
            "--flake",
            ".#neo",
        ]),
        &desc,
    ) {
        if has_changes {
            let _ = git_cmd(config_path, &["branch", "-D", &build_branch]);
        }
        let _ = git_cmd(config_path, &["switch", &orig_branch]);
        let _ = git_cmd(config_path, &["branch", "-D", &activation_branch]);
        return Err(e);
    }

    if has_changes {
        let _ = git_cmd(config_path, &["branch", "-D", &build_branch]);
    }
    println!("Activated using branch {}", activation_branch);
    Ok(())
}
