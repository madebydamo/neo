use anyhow::Result;

use crate::commands::run_nix;

pub fn update(config_path: &str, dry_run: bool, nix_cmd: &str) -> Result<()> {
    if dry_run {
        println!("DRY-RUN: nix flake update in {}", config_path);
        return Ok(());
    }
    run_nix(config_path, nix_cmd, &["flake", "update"])?;
    println!("Flake updated in {}", config_path);
    Ok(())
}
