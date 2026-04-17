use anyhow::Result;
use std::process::Command;

use crate::commands::execute_command;

pub fn update_inputs(config_path: &str, dry_run: bool, nix_cmd: &str) -> Result<()> {
    if dry_run {
        println!("DRY-RUN: nix run .#write-flake in {}", config_path);
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
    println!("Flake updated in {}", config_path);
    Ok(())
}
