use anyhow::Result;
use std::process::Command;

use crate::commands::execute_command;

pub fn activate(config_path: &str, dry_run: bool, nix_cmd: &str, sudo_cmd: &str) -> Result<()> {
    if dry_run {
        println!(
            "DRY-RUN: activate sequence (write-flake, build, git branch, nixos-rebuild switch)"
        );
        return Ok(());
    }
    let desc = format!("{} run .#write-flake (in {})", nix_cmd, config_path);
    execute_command(
        Command::new(nix_cmd).current_dir(config_path).args([
            "--extra-experimental-features",
            "nix-command flakes",
            "run",
            ".#write-flake",
        ]),
        &desc,
    )?;

    let desc = format!(
        "{} build .#nixosConfigurations.neo.config.system.build.toplevel (in {})",
        nix_cmd, config_path
    );
    execute_command(
        Command::new(nix_cmd).current_dir(config_path).args([
            "--extra-experimental-features",
            "nix-command flakes",
            "build",
            ".#nixosConfigurations.neo.config.system.build.toplevel",
        ]),
        &desc,
    )?;

    let branch = Command::new("date")
        .arg("+%Y%m%d-%H%M%S")
        .output()
        .map(|o| {
            std::string::String::from_utf8_lossy(&o.stdout)
                .trim()
                .to_string()
        })
        .unwrap_or_else(|_| "rebuild".to_string());

    let git_desc = |action: &str| format!("git {} (in {})", action, config_path);
    execute_command(
        Command::new("git")
            .current_dir(config_path)
            .args(["switch", "-C", &branch]),
        &git_desc("switch -C"),
    )?;

    execute_command(
        Command::new("git")
            .current_dir(config_path)
            .arg("add")
            .arg("."),
        &git_desc("add"),
    )?;

    if Command::new("git")
        .current_dir(config_path)
        .args(["diff", "--staged", "--quiet"])
        .status()
        .map(|s| !s.success())
        .unwrap_or(false)
    {
        execute_command(
            Command::new("git").current_dir(config_path).args([
                "commit",
                "-m",
                &format!("Rebuild: {}", branch),
            ]),
            &git_desc("commit"),
        )?;
    }

    let desc = format!(
        "{} nixos-rebuild switch --flake .#neo (in {})",
        sudo_cmd, config_path
    );
    execute_command(
        Command::new(sudo_cmd).current_dir(config_path).args([
            "nixos-rebuild",
            "switch",
            "--flake",
            ".#neo",
        ]),
        &desc,
    )?;

    println!("Activated using branch {}", branch);
    Ok(())
}
