use anyhow::Result;
use toml_edit::DocumentMut;

use crate::commands::{get_timestamp, git_cmd, has_staged_changes, run_nix};

pub fn build(config_path: &str, config: &DocumentMut, dry_run: bool, nix_cmd: &str) -> Result<()> {
    let disko_enabled = config
        .get("disko")
        .and_then(|t| t.get("enabled"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let vm = format!(
        ".#nixosConfigurations.vm.config.system.build.{}",
        if disko_enabled { "vmWithDisko" } else { "vm" }
    );
    if dry_run {
        println!(
            "DRY-RUN: nix build .#nixosConfigurations.neo.config.system.build.toplevel + {} + git commit-if-changes (in {})",
            &vm, config_path
        );
        return Ok(());
    }
    run_nix(config_path, nix_cmd, &["run", ".#write-flake"])?;
    run_nix(
        config_path,
        nix_cmd,
        &[
            "build",
            ".#nixosConfigurations.neo.config.system.build.toplevel",
        ],
    )?;
    run_nix(config_path, nix_cmd, &["build", &vm])?;
    git_cmd(config_path, &["add", "."])?;
    if has_staged_changes(config_path) {
        let ts = get_timestamp();
        git_cmd(config_path, &["commit", "-m", &format!("Build: {}", ts)])?;
    }
    println!("Built configuration");
    Ok(())
}
