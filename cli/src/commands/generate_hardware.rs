use anyhow::{Context, Result};
use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};
use toml_edit::DocumentMut;

use crate::utils::format_command;

pub fn generate_hardware(
    config_path: &str,
    config: &DocumentMut,
    dry_run: bool,
    _nix_cmd: &str,
) -> Result<()> {
    let disko_enabled = config
        .get("disko")
        .and_then(|t| t.get("enabled"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
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
    let output = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .output()
        .with_context(|| format!("failed to spawn: {display}"))?;
    if !output.status.success() {
        let stderr = std::string::String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Command failed: {display}\nStderr: {stderr}");
    }
    fs::write(
        Path::new(config_path).join("hardware-configuration.nix"),
        output.stdout,
    )
    .context("write hardware-configuration.nix")?;
    println!("Generated hardware-configuration.nix in {}", config_path);
    Ok(())
}
