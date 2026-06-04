use anyhow::Result;

use crate::commands::run_nix;

pub fn update_inputs(config_path: &str, dry_run: bool, nix_cmd: &str) -> Result<()> {
    if dry_run {
        println!("DRY-RUN: nix run .#write-flake in {}", config_path);
        return Ok(());
    }
    run_nix(config_path, nix_cmd, &["run", ".#write-flake"])?;
    println!("Flake updated in {}", config_path);
    Ok(())
}
