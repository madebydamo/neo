use anyhow::Result;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

use crate::commands::{
    execute_command, get_current_branch, get_timestamp, git_cmd, has_staged_changes, run_nix,
};

pub fn activate(config_path: &str, dry_run: bool, nix_cmd: &str, sudo_cmd: &str, activation_suffix: Option<&str>) -> Result<()> {
    if dry_run {
        println!(
            "DRY-RUN: activate (run_nix write-flake + toplevel build, optional build_xxx+Build-commit if changes, switch -C activation_xxx, optional amend-recommit, nixos-rebuild, cleanup build_ on success or restore+delete branches on fail)"
        );
        return Ok(());
    }

    let suffix = activation_suffix
        .map(|s| s.to_string())
        .unwrap_or_else(|| std::env::var("NEO_ACTIVATION_SUFFIX").unwrap_or_else(|_| get_timestamp()));
    let activation_id = format!("activation_{}", suffix);
    let act_dir = PathBuf::from("/tmp/neo-activations");
    let _ = fs::create_dir_all(&act_dir);
    let state_path = act_dir.join(format!("{}.json", activation_id));
    let log_path = act_dir.join(format!("{}.log", activation_id));
    let write_state = |status: &str, phase: &str, err: Option<&str>, br: Option<&str>| {
        let mut s = serde_json::json!({
            "id": &activation_id,
            "status": status,
            "phase": phase,
            "started_at": &suffix,
            "log_path": log_path.to_string_lossy(),
        });
        if let Some(e) = err { s["error"] = serde_json::json!(e); }
        if let Some(b) = br { s["branch"] = serde_json::json!(b); }
        let _ = fs::write(&state_path, serde_json::to_string_pretty(&s).unwrap_or_else(|_| "{}".to_string()));
    };
    write_state("in_progress", "starting", None, None);

    if let Err(e) = run_nix(config_path, nix_cmd, &["run", ".#write-flake"]) {
        write_state("failed", "write-flake", Some(&e.to_string()), None);
        return Err(e);
    }
    write_state("in_progress", "write-flake-done", None, None);

    if let Err(e) = run_nix(
        config_path,
        nix_cmd,
        &[
            "build",
            ".#nixosConfigurations.neo.config.system.build.toplevel",
        ],
    ) {
        write_state("failed", "toplevel-build", Some(&e.to_string()), None);
        return Err(e);
    }
    write_state("in_progress", "toplevel-built", None, None);

    let activation_branch = format!("activation_{}", suffix);
    let build_branch = format!("build_{}", suffix);
    let orig_branch = get_current_branch(config_path).unwrap_or_else(|_| "master".to_string());

    if let Err(e) = git_cmd(config_path, &["add", "."]) {
        write_state("failed", "git-add", Some(&e.to_string()), None);
        return Err(e);
    }
    let has_changes = has_staged_changes(config_path);
    if has_changes {
        if let Err(e) = git_cmd(config_path, &["switch", "-C", &build_branch]) {
            write_state("failed", "build-branch", Some(&e.to_string()), None);
            return Err(e);
        }
        if let Err(e) = git_cmd(
            config_path,
            &["commit", "-m", &format!("Build: {}", suffix)],
        ) {
            write_state("failed", "build-commit", Some(&e.to_string()), None);
            return Err(e);
        }
    }

    if let Err(e) = git_cmd(config_path, &["switch", "-C", &activation_branch]) {
        if has_changes {
            let _ = git_cmd(config_path, &["branch", "-D", &build_branch]);
        }
        let _ = git_cmd(config_path, &["switch", &orig_branch]);
        write_state("failed", "branch-failed", Some(&e.to_string()), None);
        return Err(e);
    }
    write_state("in_progress", "branches-created", None, Some(&activation_branch));

    if has_changes {
        if let Err(e) = git_cmd(config_path, &["add", "."]) {
            write_state("failed", "amend-add", Some(&e.to_string()), None);
            return Err(e);
        }
        if let Err(e) = git_cmd(
            config_path,
            &[
                "commit",
                "--amend",
                "-m",
                &format!("Activation: {}", activation_branch),
            ],
        ) {
            write_state("failed", "amend-commit", Some(&e.to_string()), None);
            return Err(e);
        }
    }

    write_state("in_progress", "pre-rebuild", None, Some(&activation_branch));
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
        write_state("failed", "rebuild-failed", Some(&e.to_string()), None);
        return Err(e);
    }

    if has_changes {
        let _ = git_cmd(config_path, &["branch", "-D", &build_branch]);
    }
    write_state("success", "completed", None, Some(&activation_branch));
    println!("Activated using branch {}", activation_branch);
    Ok(())
}
