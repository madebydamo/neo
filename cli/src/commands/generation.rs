//! CLI entry points for `neo generation list|switch|boot`.

use anyhow::{bail, Context, Result};

use crate::utils::generation::{list_system_generations, switch_system_generation, GenerationMode};
use crate::utils::ops::{OperationKind, OperationLog};

pub fn generation_list(dry_run: bool, sudo_cmd: &str) -> Result<()> {
    if dry_run {
        println!("DRY-RUN: list system generations");
        return Ok(());
    }
    let list = list_system_generations(sudo_cmd);
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
    let mode_s = mode.as_str();
    if dry_run {
        println!("DRY-RUN: generation {mode_s} {n}");
        return Ok(());
    }

    // When triggered from the web UI via systemd-run, track progress under /tmp/neo-activations.
    let op = std::env::var("NEO_GENSWITCH_SUFFIX").ok().map(|suf| {
        OperationLog::new(OperationKind::Generation, &suf)
            .with_extra(serde_json::json!({ "generation": n, "mode": mode_s }))
    });
    let track = |status: &str, phase: &str, err: Option<&str>| {
        if let Some(op) = &op {
            op.write_state(status, phase, err, None);
        }
    };
    track("in_progress", "starting", None);
    let _tee = op.as_ref().and_then(|op| op.capture_stdio());
    track("in_progress", "nix-env-switch-generation", None);

    match switch_system_generation(n, mode, sudo_cmd) {
        Ok(()) => {
            track("success", "completed", None);
            match mode {
                GenerationMode::Switch => println!("Switched to generation {n}"),
                GenerationMode::Boot => println!("Boot default set to generation {n}"),
            }
            Ok(())
        }
        Err(e) => {
            track("failed", "switch-failed", Some(&format!("{e:#}")));
            Err(e).with_context(|| format!("generation {mode_s} {n}"))
        }
    }
}

pub fn generation_help() -> Result<()> {
    eprintln!(
        "Usage:\n  neo generation list\n  neo generation switch <N>\n  neo generation boot <N>"
    );
    bail!("missing generation subcommand");
}
