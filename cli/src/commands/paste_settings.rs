use anyhow::{Context, Result};
use std::fs;
use std::path::PathBuf;
use toml_edit::DocumentMut;

use crate::utils::sort_document_alphabetically;

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

    // Parse + sort so paste/init/restore match web-save ordering (stable diffs).
    let mut doc: DocumentMut = if settings_source.exists() {
        let raw = fs::read_to_string(settings_source).context("read source settings.toml")?;
        raw.parse().context("parse source settings.toml")?
    } else {
        config.clone()
    };
    sort_document_alphabetically(&mut doc);
    fs::write(&target, doc.to_string()).context("write settings")?;

    println!(
        "Pasted settings from {:?} to {}",
        settings_source, config_path
    );
    Ok(())
}
