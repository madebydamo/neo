//! Start activation / update / generation-switch oneshots and return their monitor.
use rocket::response::content::RawHtml;

use crate::utils::command::sudo_command;
use crate::utils::locks::{LockSpec, OpInfo};
use crate::utils::{execute_command, get_timestamp, GenerationMode, OperationKind, OperationLog};

/// Seconds the spawned `neo` waits for the system lock (covers the probe → spawn gap
/// and a just-finishing op); a real conflict is refused by [`probe`] before spawning.
const SPAWNED_LOCK_WAIT: &str = "30";

use super::locks::{probe, Blocked};
use super::ops::monitor::monitor_fragment;
use super::ops::store::{find_recent_in_progress, gc_old_ops};
use super::util::{self, alert_html, AlertKind};

/// An activation or generation switch is running (config must not move under it).
pub fn is_activation_in_progress() -> bool {
    find_recent_in_progress(OperationKind::Activation).is_some()
        || find_recent_in_progress(OperationKind::Generation).is_some()
}

/// Launch `neo <args…>` as a detached oneshot under homeserver via systemd-run.
/// Survives `neo-web` restarts (e.g. generation switch stops neo-web.service).
fn trigger_systemd_run(
    unit_leaf: &str,
    neo_args: &[&str],
    env_pairs: &[(&str, &str)],
    description: &str,
) {
    let sudo_cmd = util::sudo_cmd();
    let nix_bin = util::nix_bin();
    let neo_bin = util::neo_bin();
    let unit = format!("neo-{}.service", unit_leaf);
    let mut run_cmd = sudo_command();
    run_cmd.args([
        "systemd-run",
        "--collect",
        "--no-ask-password",
        "--no-block",
        "--unit",
        &unit,
        "--service-type=oneshot",
        "--uid=homeserver",
        "--gid=homeserver",
        "-E",
        &format!("NIX_BINARY_PATH={}", nix_bin),
        "-E",
        &format!("SUDO_BINARY_PATH={}", sudo_cmd),
        "-E",
        &format!("NEO_LOCK_WAIT={SPAWNED_LOCK_WAIT}"),
        "-E",
        "PATH=/run/current-system/sw/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin",
    ]);
    for (k, v) in env_pairs {
        run_cmd.arg("-E");
        run_cmd.arg(format!("{k}={v}"));
    }
    run_cmd.args([
        "--property",
        &format!("Description={description}"),
        &neo_bin,
    ]);
    run_cmd.args(neo_args);
    let _ = execute_command(&mut run_cmd);
}

fn alert(kind: AlertKind, msg: String) -> RawHtml<String> {
    RawHtml(alert_html(kind, &msg))
}

/// `neo activate` / `neo update` oneshot, tracked as a fresh op log.
fn trigger_oneshot(kind: OperationKind, subcommand: &str, env_var: &str) -> RawHtml<String> {
    let ts = get_timestamp();
    let op = OperationLog::new(kind, &ts);
    op.init_for_web_trigger(&ts);
    let suffix = op.suffix();
    trigger_systemd_run(
        &format!("{subcommand}@{suffix}"),
        &[subcommand],
        &[(env_var, suffix)],
        &format!("Neo one-shot {subcommand} {suffix}"),
    );
    RawHtml(monitor_fragment(op.id(), None))
}

/// The system scope is free (no activation / update / restore / … in any process).
pub fn probe_system_change(kind: &str, label: &str) -> Result<(), Blocked> {
    probe(&LockSpec::system_change(), OpInfo::new(kind, label))
}

pub fn trigger_activation() -> Result<RawHtml<String>, Blocked> {
    gc_old_ops();
    probe_system_change("activation", "Activation")?;
    if let Some(id) = find_recent_in_progress(OperationKind::Activation) {
        return Ok(alert(
            AlertKind::Error,
            format!("Another activation {id} in progress (or auto-update). Wait."),
        ));
    }
    if let Some(id) = find_recent_in_progress(OperationKind::Generation) {
        return Ok(alert(
            AlertKind::Error,
            format!("Generation switch {id} in progress. Wait."),
        ));
    }
    Ok(trigger_oneshot(
        OperationKind::Activation,
        "activate",
        "NEO_ACTIVATION_SUFFIX",
    ))
}

/// Flake/input update oneshot (blocks if activate or update already running).
pub fn trigger_update() -> Result<RawHtml<String>, Blocked> {
    gc_old_ops();
    probe_system_change("update", "Update")?;
    if let Some(id) = find_recent_in_progress(OperationKind::Activation) {
        return Ok(alert(
            AlertKind::Info,
            format!("Activation {id} in progress — cannot update"),
        ));
    }
    if let Some(id) = find_recent_in_progress(OperationKind::Update) {
        return Ok(alert(
            AlertKind::Info,
            format!("Update {id} already in progress"),
        ));
    }
    if let Some(id) = find_recent_in_progress(OperationKind::Generation) {
        return Ok(alert(
            AlertKind::Info,
            format!("Generation switch {id} in progress — cannot update"),
        ));
    }
    Ok(trigger_oneshot(
        OperationKind::Update,
        "update",
        "NEO_UPDATE_SUFFIX",
    ))
}

/// Detached generation switch/boot. Must not run in the neo-web process:
/// `switch-to-configuration` stops `neo-web.service`.
pub fn trigger_generation_switch(n: u64, mode: GenerationMode) -> Result<RawHtml<String>, Blocked> {
    gc_old_ops();
    probe_system_change("generation", "Generation switch")?;
    if let Some(id) = find_recent_in_progress(OperationKind::Activation) {
        return Ok(alert(
            AlertKind::Error,
            format!("Activation {id} in progress — cannot switch generation"),
        ));
    }
    if let Some(id) = find_recent_in_progress(OperationKind::Generation) {
        return Ok(alert(
            AlertKind::Error,
            format!("Generation switch {id} already in progress"),
        ));
    }

    let ts = get_timestamp();
    // Suffix embeds mode + gen for uniqueness and log readability.
    let mode_s = mode.as_str();
    let suffix = format!("{mode_s}-{n}-{ts}");
    let op = OperationLog::new(OperationKind::Generation, &suffix);
    op.init_for_web_trigger(&ts);
    op.write_state_extra(
        "in_progress",
        "triggered",
        None,
        None,
        Some(serde_json::json!({
            "generation": n,
            "mode": mode_s,
        })),
    );

    let n_s = n.to_string();
    trigger_systemd_run(
        &format!("genswitch@{suffix}"),
        &["generation", mode_s, &n_s],
        &[("NEO_GENSWITCH_SUFFIX", op.suffix())],
        &format!("Neo generation {mode_s} {n}"),
    );

    let what = match mode {
        GenerationMode::Boot => format!("Setting generation {n} as the boot default."),
        GenerationMode::Switch => format!("Switching to generation {n}."),
    };
    Ok(RawHtml(monitor_fragment(
        op.id(),
        Some(&format!(
            "{what} The web UI may restart during the switch; the output reconnects automatically."
        )),
    )))
}
