use anyhow::Result;
use std::process::Command;

use crate::commands::execute_command;

pub fn git(config_path: &str, dry_run: bool) -> Result<()> {
    if dry_run {
        println!("DRY-RUN: lazygit (in {})", config_path);
        return Ok(());
    }
    execute_command(Command::new("lazygit").current_dir(config_path))
}
