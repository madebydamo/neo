use anyhow::{Context, Result};
use std::process::Command;

pub fn activate(config_path: &str, dry_run: bool) -> Result<()> {
    if dry_run {
        println!(
            "DRY-RUN: activate sequence (write-flake, build, git branch, nixos-rebuild switch)"
        );
        return Ok(());
    }
    Command::new("nix")
        .current_dir(config_path)
        .args([
            "--extra-experimental-features",
            "nix-command flakes",
            "run",
            ".#write-flake",
        ])
        .status()
        .context("write-flake failed")?;

    let _ = Command::new("nix")
        .current_dir(config_path)
        .args([
            "--extra-experimental-features",
            "nix-command flakes",
            "build",
            ".#nixosConfigurations.neo.config.system.build.toplevel",
        ])
        .status();

    let branch = Command::new("date")
        .arg("+%Y%m%d-%H%M%S")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|_| "rebuild".to_string());

    let _ = Command::new("git")
        .current_dir(config_path)
        .args(["switch", "-C", &branch])
        .status();

    let _ = Command::new("git")
        .current_dir(config_path)
        .arg("add")
        .arg(".")
        .status();

    if Command::new("git")
        .current_dir(config_path)
        .args(["diff", "--staged", "--quiet"])
        .status()
        .map(|s| !s.success())
        .unwrap_or(false)
    {
        let _ = Command::new("git")
            .current_dir(config_path)
            .args(["commit", "-m", &format!("Rebuild: {}", branch)])
            .status();
    }

    let _ = Command::new("sudo")
        .current_dir(config_path)
        .args(["nixos-rebuild", "switch", "--flake", ".#neo"])
        .status();

    println!("Activated using branch {}", branch);
    Ok(())
}
