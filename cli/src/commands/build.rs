use anyhow::Result;
use toml_edit::DocumentMut;

use crate::utils::command::TOPLEVEL_ATTR;
use crate::utils::{
    disko_enabled, get_timestamp, git_cmd, has_staged_changes, run_nix, run_write_flake,
};

pub fn build(config_path: &str, config: &DocumentMut, dry_run: bool, nix_cmd: &str) -> Result<()> {
    let vm = format!(
        ".#nixosConfigurations.vm.config.system.build.{}",
        if disko_enabled(config) {
            "vmWithDisko"
        } else {
            "vm"
        }
    );
    if dry_run {
        println!(
            "DRY-RUN: nix build .#nixosConfigurations.neo.config.system.build.toplevel + {} + git commit-if-changes (in {})",
            &vm, config_path
        );
        return Ok(());
    }
    run_write_flake(config_path, nix_cmd)?;
    run_nix(config_path, nix_cmd, &["build", TOPLEVEL_ATTR])?;
    run_nix(config_path, nix_cmd, &["build", &vm])?;
    git_cmd(config_path, &["add", "."])?;
    if has_staged_changes(config_path) {
        let ts = get_timestamp();
        git_cmd(config_path, &["commit", "-m", &format!("Build: {}", ts)])?;
    }
    println!("Built configuration");
    Ok(())
}
