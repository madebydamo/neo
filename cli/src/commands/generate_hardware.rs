use anyhow::{Context, Result};
use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};
use toml_edit::DocumentMut;

use crate::commands::generate_hardware;

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
    let desc = format!(
        "nixos-generate-config --no-filesystems --show-hardware-config (in {})",
        config_path
    );
    println!("→ {}", desc);
    let mut binding = Command::new("nixos-generate-config");
    let cmd = binding
        .current_dir(config_path)
        .arg("--show-hardware-config");
    let cmd = if disko_enabled {
        cmd.arg("--no-filesystems")
    } else {
        cmd
    };
    let output = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .output()
        .with_context(|| format!("failed to spawn: {}", desc))?;
    if !output.status.success() {
        let stderr = std::string::String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Command failed: {}\nStderr: {}", desc, stderr);
    }
    fs::write(
        Path::new(config_path).join("hardware-configuration.nix"),
        output.stdout,
    )
    .context("write hardware-configuration.nix")?;
    println!("Generated hardware-configuration.nix in {}", config_path);
    Ok(())
}
