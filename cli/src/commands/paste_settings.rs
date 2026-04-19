use anyhow::{Context, Result};
use std::fs;
use std::path::PathBuf;
use toml_edit::DocumentMut;

pub fn paste_settings(
    config_path: &str,
    settings_source: &PathBuf,
    config: &DocumentMut,
    dry_run: bool,
    _nix_cmd: &str,
) -> Result<()> {
    if dry_run {
        println!(
            "DRY-RUN: copy settings from {:?} to {}",
            settings_source, config_path
        );
        return Ok(());
    }
    let target = PathBuf::from(config_path).join("settings.toml");
    fs::create_dir_all(config_path)?;

    let content = if settings_source.exists() {
        fs::read_to_string(settings_source).context("read source settings.toml")?
    } else {
        config.to_string()
    };
    fs::write(&target, content).context("write settings")?;

    println!(
        "Pasted settings from {:?} to {}",
        settings_source, config_path
    );
    Ok(())
}
