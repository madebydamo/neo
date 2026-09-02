use anyhow::{Context, Result};
use std::fs;
use std::path::Path;
use toml_edit::DocumentMut;

use crate::utils::{neo_cli_get, resolve_suffix, run_nix, OperationLog};

pub fn update(
    config_path: &str,
    config: &DocumentMut,
    profile: &str,
    dry_run: bool,
    nix_cmd: &str,
    update_suffix: Option<&str>,
) -> Result<()> {
    if dry_run {
        println!(
            "DRY-RUN: nix flake update in {} (profile={})",
            config_path, profile
        );
        println!("DRY-RUN: would delete modules/ and run nix flake init -t <template> (refresh from neo template, if set as bootstrapMethod)");
        println!("DRY-RUN: would nix run .#neo -- migrate (from updated input)");
        return Ok(());
    }

    let suffix = resolve_suffix(update_suffix, "NEO_UPDATE_SUFFIX");
    let op = OperationLog::new_update(&suffix);
    op.write_state("in_progress", "starting", None, None);
    let _tee = op.capture_stdio();

    // Delete modules/ folder and re-run the exact same nix flake init as init.rs does.
    // This refreshes the template-provided modules (imports, inputs, nixos, settings) from the
    // (possibly newly updated) neo template, without clobbering user files like settings.toml or
    // the committed flake.nix (we temp-move it to let init succeed, then restore).

    let bootstrap_method = neo_cli_get(config, profile, "bootstrapMethod").unwrap_or("template");

    if bootstrap_method != "clone" {
        let modules_dir = Path::new(config_path).join("modules");
        if modules_dir.exists() {
            fs::remove_dir_all(&modules_dir).context("delete modules folder")?;
        }
        let flake_path = Path::new(config_path).join("flake.nix");
        let flake_backup = Path::new(config_path).join("flake.nix.update-backup");
        let had_flake = flake_path.exists();
        if had_flake {
            fs::rename(&flake_path, &flake_backup)
                .context("backup flake.nix for template refresh init")?;
        }
        let template =
            neo_cli_get(config, profile, "template").unwrap_or("github:madebydamo/neo#homeserver");

        let init_result = op.step("flake init", || {
            run_nix(config_path, nix_cmd, &["flake", "init", "-t", template])
        });
        if had_flake && flake_backup.exists() {
            if let Err(rerr) = fs::rename(&flake_backup, &flake_path) {
                eprintln!("warning: could not restore flake.nix backup: {}", rerr);
                if init_result.is_ok() {
                    op.write_state("failed", "post-init restore", Some(&rerr.to_string()), None);
                    return Err(rerr)
                        .context("failed to restore flake.nix after successful template init");
                }
            }
        }
        init_result?;
    }

    op.step("write-flake", || {
        run_nix(config_path, nix_cmd, &["run", ".#write-flake"])
    })?;
    op.step("flake update", || {
        run_nix(config_path, nix_cmd, &["flake", "update"])
    })?;
    op.step("migrate", || {
        run_nix(config_path, nix_cmd, &["run", ".#neo", "--", "migrate"])
    })?;
    op.write_state("success", "complete", None, None);
    println!("Flake updated and migrated in {}", config_path);
    Ok(())
}
