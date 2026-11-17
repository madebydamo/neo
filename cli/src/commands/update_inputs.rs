use anyhow::Result;

use crate::utils::command::run_write_flake;

pub fn update_inputs(config_path: &str, dry_run: bool, nix_cmd: &str) -> Result<()> {
    if dry_run {
        println!("DRY-RUN: nix run --impure .#write-flake in {}", config_path);
        return Ok(());
    }
    run_write_flake(config_path, nix_cmd)?;
    println!("Flake updated in {}", config_path);
    Ok(())
}
