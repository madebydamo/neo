pub mod activate;
pub mod build;
pub mod generate_hardware;
pub mod init;
pub mod nuke;
pub mod paste_settings;
pub mod update;
pub mod update_inputs;
pub mod web;

use anyhow::{Context, Result};
use std::process::{Command, Stdio};

pub fn execute_command(cmd: &mut Command, description: &str) -> Result<()> {
    println!("→ {}", description);
    let status = cmd
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .with_context(|| format!("failed to spawn: {}", description))?;
    if !status.success() {
        anyhow::bail!("Command failed: {}", description);
    }
    Ok(())
}
