//! CLI entry points for `neo generation list|switch|boot`.

use anyhow::{bail, Context, Result};

use crate::utils::generation::{list_system_generations, switch_system_generation, GenerationMode};
use crate::utils::ops::OperationLog;

pub fn generation_list(dry_run: bool) -> Result<()> {
    if dry_run {
        println!("DRY-RUN: list system generations");
        return Ok(());
    }
    let list = list_system_generations();
    if list.unavailable {
        if let Some(msg) = list.message {
            println!("{msg}");
        } else {
            println!("System profile unavailable.");
        }
        return Ok(());
    }
    for g in &list.generations {
        let marker = if g.is_current { " (current)" } else { "" };
        println!("{:>4}  {}{}  {}", g.number, g.date, marker, g.path);
    }
    Ok(())
}

/// Run generation switch with optional web op-state tracking (`NEO_GENSWITCH_SUFFIX`).
pub fn generation_switch(n: u64, dry_run: bool, sudo_cmd: &str) -> Result<()> {
    run_generation_op(n, GenerationMode::Switch, dry_run, sudo_cmd)
}

/// Run generation boot with optional web op-state tracking (`NEO_GENSWITCH_SUFFIX`).
pub fn generation_boot(n: u64, dry_run: bool, sudo_cmd: &str) -> Result<()> {
    run_generation_op(n, GenerationMode::Boot, dry_run, sudo_cmd)
}

fn run_generation_op(n: u64, mode: GenerationMode, dry_run: bool, sudo_cmd: &str) -> Result<()> {
    let mode_s = match mode {
        GenerationMode::Switch => "switch",
        GenerationMode::Boot => "boot",
    };
    if dry_run {
        println!("DRY-RUN: generation {mode_s} {n}");
        return Ok(());
    }

    // When triggered from the web UI via systemd-run, track progress under /tmp/neo-activations.
    let op = std::env::var("NEO_GENSWITCH_SUFFIX").ok().map(|suf| {
        let log = OperationLog::new_generation(&suf);
        log.write_state_extra(
            "in_progress",
            "starting",
            None,
            None,
            Some(serde_json::json!({ "generation": n, "mode": mode_s })),
        );
        log
    });

    if let Some(ref op) = op {
        op.write_state_extra(
            "in_progress",
            "nix-env-switch-generation",
            None,
            None,
            Some(serde_json::json!({ "generation": n, "mode": mode_s })),
        );
    }

    match switch_system_generation(n, mode, sudo_cmd) {
        Ok(()) => {
            if let Some(ref op) = op {
                op.write_state_extra(
                    "success",
                    "completed",
                    None,
                    None,
                    Some(serde_json::json!({ "generation": n, "mode": mode_s })),
                );
            }
            match mode {
                GenerationMode::Switch => println!("Switched to generation {n}"),
                GenerationMode::Boot => println!("Boot default set to generation {n}"),
            }
            Ok(())
        }
        Err(e) => {
            if let Some(ref op) = op {
                op.write_state_extra(
                    "failed",
                    "switch-failed",
                    Some(&e),
                    None,
                    Some(serde_json::json!({ "generation": n, "mode": mode_s })),
                );
            }
            Err(anyhow::anyhow!(e)).with_context(|| format!("generation {mode_s} {n}"))
        }
    }
}

pub fn generation_help() -> Result<()> {
    eprintln!(
        "Usage:\n  neo generation list\n  neo generation switch <N>\n  neo generation boot <N>"
    );
    bail!("missing generation subcommand");
}
