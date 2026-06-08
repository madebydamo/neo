use anyhow::{Context, Result};
use std::fs;
use std::path::Path;
use toml_edit::DocumentMut;

use crate::commands::run_nix;

pub fn update(
    config_path: &str,
    config: &DocumentMut,
    section: &str,
    dry_run: bool,
    nix_cmd: &str,
) -> Result<()> {
    if dry_run {
        println!("DRY-RUN: nix flake update in {}", config_path);
        println!("DRY-RUN: would delete modules/ and run nix flake init -t <template> (refresh from neo template, if set as bootstrapMethod)");
        println!("DRY-RUN: would nix run .#neo -- migrate (from updated input)");
        return Ok(());
    }

    // Delete modules/ folder and re-run the exact same nix flake init as init.rs does.
    // This refreshes the template-provided modules (imports, inputs, nixos, settings) from the
    // (possibly newly updated) neo template, without clobbering user files like settings.toml or
    // the committed flake.nix (we temp-move it to let init succeed, then restore).

    let bootstrap_method = config
        .get(section)
        .and_then(|t| t.get("bootstrapMethod"))
        .and_then(|v| v.as_str())
        .unwrap_or("template");

    if bootstrap_method != "clone" {
        let modules_dir = Path::new(config_path).join("modules");
        if modules_dir.exists() {
            fs::remove_dir_all(&modules_dir).context("delete modules folder")?;
        }
        let template = config
            .get(section)
            .and_then(|t| t.get("template"))
            .and_then(|v| v.as_str())
            .unwrap_or("github:madebydamo/neo#homeserver");

        run_nix(config_path, nix_cmd, &["flake", "init", "-t", template])
            .inspect_err(|_| println!("Best effort applied"));
    }

    run_nix(config_path, nix_cmd, &["run", ".#write-flake"])?;
    run_nix(config_path, nix_cmd, &["flake", "update"])?;
    run_nix(config_path, nix_cmd, &["run", ".#neo", "--", "migrate"])?;
    println!("Flake updated and migrated in {}", config_path);
    Ok(())
}
