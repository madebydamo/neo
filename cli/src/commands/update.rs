use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use toml_edit::DocumentMut;

use crate::commands::{get_timestamp, run_nix};

pub fn update(
    config_path: &str,
    config: &DocumentMut,
    section: &str,
    dry_run: bool,
    nix_cmd: &str,
    update_suffix: Option<&str>,
) -> Result<()> {
    if dry_run {
        println!("DRY-RUN: nix flake update in {}", config_path);
        println!("DRY-RUN: would delete modules/ and run nix flake init -t <template> (refresh from neo template, if set as bootstrapMethod)");
        println!("DRY-RUN: would nix run .#neo -- migrate (from updated input)");
        return Ok(());
    }

    let suffix = update_suffix
        .map(|s| s.to_string())
        .unwrap_or_else(|| std::env::var("NEO_UPDATE_SUFFIX").unwrap_or_else(|_| get_timestamp()));
    let update_id = format!("update_{}", suffix);
    let act_dir = PathBuf::from("/tmp/neo-activations");
    let _ = fs::create_dir_all(&act_dir);
    let state_path = act_dir.join(format!("{}.json", update_id));
    let log_path = act_dir.join(format!("{}.log", update_id));
    let write_state = |status: &str, phase: &str, err: Option<&str>| {
        let mut s = serde_json::json!({
            "id": &update_id,
            "status": status,
            "phase": phase,
            "started_at": &suffix,
            "log_path": log_path.to_string_lossy(),
        });
        if let Some(e) = err {
            s["error"] = serde_json::json!(e);
        }
        let _ = fs::write(
            &state_path,
            serde_json::to_string_pretty(&s).unwrap_or_else(|_| "{}".to_string()),
        );
    };
    write_state("in_progress", "starting", None);

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
        let flake_path = Path::new(config_path).join("flake.nix");
        let flake_backup = Path::new(config_path).join("flake.nix.update-backup");
        let had_flake = flake_path.exists();
        if had_flake {
            fs::rename(&flake_path, &flake_backup)
                .context("backup flake.nix for template refresh init")?;
        }
        let template = config
            .get(section)
            .and_then(|t| t.get("template"))
            .and_then(|v| v.as_str())
            .unwrap_or("github:madebydamo/neo#homeserver");

        write_state("in_progress", "flake init", None);
        let init_result = run_nix(config_path, nix_cmd, &["flake", "init", "-t", template]);
        if had_flake && flake_backup.exists() {
            if let Err(rerr) = fs::rename(&flake_backup, &flake_path) {
                eprintln!("warning: could not restore flake.nix backup: {}", rerr);
                if init_result.is_ok() {
                    write_state("failed", "post-init restore", Some(&rerr.to_string()));
                    return Err(rerr)
                        .context("failed to restore flake.nix after successful template init");
                }
            }
        }
        if let Err(e) = init_result {
            write_state("failed", "flake init", Some(&e.to_string()));
            return Err(e);
        }
    }

    write_state("in_progress", "write-flake", None);
    if let Err(e) = run_nix(config_path, nix_cmd, &["run", ".#write-flake"]) {
        write_state("failed", "write-flake", Some(&e.to_string()));
        return Err(e);
    }
    write_state("in_progress", "flake update", None);
    if let Err(e) = run_nix(config_path, nix_cmd, &["flake", "update"]) {
        write_state("failed", "flake update", Some(&e.to_string()));
        return Err(e);
    }
    write_state("in_progress", "migrate", None);
    if let Err(e) = run_nix(config_path, nix_cmd, &["run", ".#neo", "--", "migrate"]) {
        write_state("failed", "migrate", Some(&e.to_string()));
        return Err(e);
    }
    write_state("success", "complete", None);
    println!("Flake updated and migrated in {}", config_path);
    Ok(())
}
