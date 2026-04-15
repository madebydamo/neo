use anyhow::{Context, Result};
use std::process::Command;

pub fn update_inputs(config_path: &str, dry_run: bool) -> Result<()> {
    if dry_run {
        println!("DRY-RUN: nix run .#write-flake in {}", config_path);
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
    println!("Flake updated in {}", config_path);
    Ok(())
}
