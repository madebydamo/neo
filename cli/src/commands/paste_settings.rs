use anyhow::{Context, Result};
use std::fs;
use std::path::PathBuf;
use toml_edit::DocumentMut;

pub fn paste_settings(
    config_path: &str,
    settings_source: &PathBuf,
    config: &DocumentMut,
    dry_run: bool,
) -> Result<()> {
    if dry_run {
        println!(
            "DRY-RUN: paste from {:?} to {}",
            settings_source, config_path
        );
        return Ok(());
    }
    let target = PathBuf::from(config_path).join("settings.toml");
    fs::create_dir_all(config_path)?;
    fs::write(&target, config.to_string()).context("write settings")?;
    println!("Pasted settings.toml to {}", config_path);
    Ok(())
}
