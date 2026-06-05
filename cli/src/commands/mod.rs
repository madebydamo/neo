pub mod activate;
pub mod build;
pub mod edit;
pub mod generate_hardware;
pub mod git;
pub mod init;
pub mod migrate;
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

pub fn run_nix(config_path: &str, nix_cmd: &str, args: &[&str]) -> Result<()> {
    let desc = format!("{} {:?} (in {})", nix_cmd, args, config_path);
    execute_command(
        Command::new(nix_cmd)
            .current_dir(config_path)
            .args(["--extra-experimental-features", "nix-command flakes"])
            .args(args),
        &desc,
    )?;
    Ok(())
}

pub fn git_cmd(config_path: &str, args: &[&str]) -> Result<()> {
    let desc = format!("git {:?} (in {})", args, config_path);
    execute_command(
        Command::new("git").current_dir(config_path).args(args),
        &desc,
    )
}

pub fn get_timestamp() -> String {
    Command::new("date")
        .arg("+%Y%m%d-%H%M%S")
        .output()
        .map(|o| {
            std::string::String::from_utf8_lossy(&o.stdout)
                .trim()
                .to_string()
        })
        .unwrap_or_else(|_| "ts".to_string())
}

pub fn has_staged_changes(config_path: &str) -> bool {
    Command::new("git")
        .current_dir(config_path)
        .args(["diff", "--staged", "--quiet"])
        .status()
        .map(|s| !s.success())
        .unwrap_or(false)
}

pub fn get_current_branch(config_path: &str) -> Result<String> {
    let out = Command::new("git")
        .current_dir(config_path)
        .args(["branch", "--show-current"])
        .output()
        .context("get current branch")?;
    let name = std::string::String::from_utf8_lossy(&out.stdout)
        .trim()
        .to_string();
    if !name.is_empty() {
        Ok(name)
    } else {
        let out = Command::new("git")
            .current_dir(config_path)
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .output()
            .context("get abbrev ref")?;
        let n = std::string::String::from_utf8_lossy(&out.stdout)
            .trim()
            .to_string();
        Ok(if n.is_empty() { "HEAD".to_string() } else { n })
    }
}
