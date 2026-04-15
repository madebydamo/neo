use anyhow::{Context, Result};
use std::process::Command;
use toml_edit::DocumentMut;

pub fn build(config_path: &str, config: &DocumentMut, dry_run: bool) -> Result<()> {
    let disko_enabled = config
        .get("disko")
        .and_then(|t| t.get("enabled"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let vm = format!(
        ".#nixosConfigurations.vm.config.system.build.{}",
        if disko_enabled { "vmWithDisko" } else { "vm" }
    );
    if dry_run {
        println!(
            "DRY-RUN: nix build .#nixosConfigurations.neo.config.system.build.toplevel + {}) in {}",
            &vm, config_path
        );
        return Ok(());
    }
    let run_nix = |args: &[&str]| -> Result<()> {
        Command::new("nix")
            .current_dir(config_path)
            .args(["--extra-experimental-features", "nix-command flakes"])
            .args(args)
            .status()
            .map(|_| ())
            .context("nix command failed")
    };
    run_nix(&["run", ".#write-flake"])?;
    //TODO
    run_nix(&[
        "build",
        ".#nixosConfigurations.neo.config.system.build.toplevel",
    ])?;
    run_nix(&["build", &vm])?;
    println!("Built configuration");
    Ok(())
}
