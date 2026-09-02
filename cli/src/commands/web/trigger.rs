use std::process::Command;

use rocket::response::content::RawHtml;

use crate::utils::{execute_command, get_timestamp, GenerationMode, OperationLog};

use super::activation;
use super::structs::AppConfig;
use super::util::{self, alert_html, AlertKind};

/// Kind of neo oneshot unit started via `systemd-run`.
pub enum OneshotKind {
    Activate,
    Update,
}

impl OneshotKind {
    fn subcommand(&self) -> &'static str {
        match self {
            OneshotKind::Activate => "activate",
            OneshotKind::Update => "update",
        }
    }

    fn env_var(&self) -> &'static str {
        match self {
            OneshotKind::Activate => "NEO_ACTIVATION_SUFFIX",
            OneshotKind::Update => "NEO_UPDATE_SUFFIX",
        }
    }
}

/// Launch `neo <args…>` as a detached oneshot under homeserver via systemd-run.
/// Survives `neo-web` restarts (e.g. generation switch stops neo-web.service).
pub fn trigger_systemd_run_args(
    unit_leaf: &str,
    neo_args: &[&str],
    env_pairs: &[(&str, &str)],
    description: &str,
) {
    let sudo_cmd = util::sudo_cmd();
    let nix_bin = util::nix_bin();
    let neo_bin = util::neo_bin();
    let unit = format!("neo-{}.service", unit_leaf);
    let mut run_cmd = Command::new(&sudo_cmd);
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

pub fn trigger_systemd_run(subcommand: &str, env_var: &str, suffix: &str) {
    trigger_systemd_run_args(
        &format!("{subcommand}@{suffix}"),
        &[subcommand],
        &[(env_var, suffix)],
        &format!("Neo one-shot {subcommand} {suffix}"),
    );
}

/// Create OperationLog, mark triggered, and launch the matching oneshot unit.
fn trigger_oneshot(kind: OneshotKind) -> OperationLog {
    let ts = get_timestamp();
    let op = match kind {
        OneshotKind::Activate => OperationLog::new_activation(&ts),
        OneshotKind::Update => OperationLog::new_update(&ts),
    };
    op.init_for_web_trigger(&ts);
    trigger_systemd_run(kind.subcommand(), kind.env_var(), op.suffix());
    op
}

pub fn trigger_activation(_config: &AppConfig) -> RawHtml<String> {
    activation::gc_old_activations();
    if let Some(other) = activation::find_recent_in_progress_activation() {
        return RawHtml(alert_html(
            AlertKind::Error,
            &format!(
                "Another activation {} in progress (or auto-update). Wait.",
                other
            ),
        ));
    }
    if let Some(other) = activation::find_recent_in_progress_genswitch() {
        return RawHtml(alert_html(
            AlertKind::Error,
            &format!("Generation switch {} in progress. Wait.", other),
        ));
    }
    let op = trigger_oneshot(OneshotKind::Activate);
    RawHtml(activation::build_monitor_fragment(op.id()))
}

/// Flake/input update oneshot (blocks if activate or update already running).
pub fn trigger_update() -> RawHtml<String> {
    activation::gc_old_activations();
    if let Some(id) = activation::find_recent_in_progress_activation() {
        return RawHtml(alert_html(
            AlertKind::Info,
            &format!("Activation {} in progress — cannot update", id),
        ));
    }
    if let Some(id) = activation::find_recent_in_progress_update() {
        return RawHtml(alert_html(
            AlertKind::Info,
            &format!("Update {} already in progress", id),
        ));
    }
    if let Some(id) = activation::find_recent_in_progress_genswitch() {
        return RawHtml(alert_html(
            AlertKind::Info,
            &format!("Generation switch {} in progress — cannot update", id),
        ));
    }
    let op = trigger_oneshot(OneshotKind::Update);
    RawHtml(activation::build_update_monitor_fragment(op.id()))
}

/// Detached generation switch/boot. Must not run in the neo-web process:
/// `switch-to-configuration` stops `neo-web.service`.
pub fn trigger_generation_switch(n: u64, mode: GenerationMode) -> RawHtml<String> {
    activation::gc_old_activations();
    if let Some(id) = activation::find_recent_in_progress_activation() {
        return RawHtml(alert_html(
            AlertKind::Error,
            &format!("Activation {} in progress — cannot switch generation", id),
        ));
    }
    if let Some(id) = activation::find_recent_in_progress_genswitch() {
        return RawHtml(alert_html(
            AlertKind::Error,
            &format!("Generation switch {} already in progress", id),
        ));
    }

    let ts = get_timestamp();
    // Suffix embeds mode + gen for uniqueness and log readability.
    let mode_s = match mode {
        GenerationMode::Switch => "switch",
        GenerationMode::Boot => "boot",
    };
    let suffix = format!("{mode_s}-{n}-{ts}");
    let op = OperationLog::new_generation(&suffix);
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
    trigger_systemd_run_args(
        &format!("genswitch@{suffix}"),
        &["generation", mode_s, &n_s],
        &[("NEO_GENSWITCH_SUFFIX", op.suffix())],
        &format!("Neo generation {mode_s} {n}"),
    );

    RawHtml(activation::build_genswitch_monitor_fragment(
        op.id(),
        n,
        mode_s,
    ))
}
