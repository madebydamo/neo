use anyhow::{Context, Result};
use std::process::Command;

pub fn update(config_path: &str, dry_run: bool) -> Result<()> {
    if dry_run {
        println!("DRY-RUN: nix flake update in {}", config_path);
        return Ok(());
    }
    Command::new("nix")
        .current_dir(config_path)
        .args([
            "--extra-experimental-features",
            "nix-command flakes",
            "flake",
            "update",
        ])
        .status()
        .context("flake update failed")?;
    println!("Flake updated in {}", config_path);
    Ok(())
}
