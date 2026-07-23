use anyhow::Result;
use std::process::Command;

use crate::utils::execute_command;

pub fn edit(config_path: &str, dry_run: bool) -> Result<()> {
    let settings_file = format!("{}/settings.toml", config_path);
    if dry_run {
        println!("DRY-RUN: vim {}", settings_file);
        return Ok(());
    }
    execute_command(Command::new("vim").arg(&settings_file))
}
