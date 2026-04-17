use anyhow::{Context, Result};
use std::fs;
use std::path::Path;
use std::process::Command;
use std::process::Stdio;

pub fn generate_hardware(config_path: &str, dry_run: bool, _nix_cmd: &str) -> Result<()> {
    if dry_run {
        println!("DRY-RUN: mkdir -p '{}' && nixos-generate-config --show-hardware-config > hardware-configuration.nix", config_path);
        return Ok(());
    }
    fs::create_dir_all(config_path).context("create config dir")?;
    let desc = format!(
        "nixos-generate-config --show-hardware-config (in {})",
        config_path
    );
    println!("→ {}", desc);
    let output = Command::new("nixos-generate-config")
        .current_dir(config_path)
        .arg("--show-hardware-config")
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
