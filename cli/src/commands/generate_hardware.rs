use anyhow::{Context, Result};
use std::fs;
use std::path::Path;
use std::process::Command;
use toml_edit::DocumentMut;

use crate::utils::command::output_checked;
use crate::utils::{disko_enabled, format_command};

pub fn generate_hardware(config_path: &str, config: &DocumentMut, dry_run: bool) -> Result<()> {
    let disko_enabled = disko_enabled(config);
    if dry_run {
        println!("DRY-RUN: mkdir -p '{}' && nixos-generate-config --show-hardware-config {}> hardware-configuration.nix", config_path, if disko_enabled {"--no-filesystems "} else {""});
        return Ok(());
    }
    fs::create_dir_all(config_path).context("create config dir")?;
    let mut cmd = Command::new("nixos-generate-config");
    cmd.current_dir(config_path).arg("--show-hardware-config");
    if disko_enabled {
        cmd.arg("--no-filesystems");
    }
    let display = format_command(&cmd);
    println!("→ {display}");
    let output = output_checked(&mut cmd, &display)?;
    fs::write(
        Path::new(config_path).join("hardware-configuration.nix"),
        output.stdout,
    )
    .context("write hardware-configuration.nix")?;
    println!("Generated hardware-configuration.nix in {}", config_path);
    Ok(())
}
