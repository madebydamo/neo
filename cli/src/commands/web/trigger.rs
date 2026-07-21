use std::process::Command;

use rocket::response::content::RawHtml;

use crate::commands::log::OperationLog;

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

pub fn trigger_systemd_run(
    subcommand: &str,
    env_var: &str,
    suffix: &str,
    log_path: &std::path::Path,
) {
    let sudo_cmd = util::sudo_cmd();
    let nix_bin = util::nix_bin();
    let neo_bin = util::neo_bin();
    let unit = format!("neo-{}@{}.service", subcommand, suffix);
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
        &format!("{}={}", env_var, suffix),
        "-E",
        "PATH=/run/current-system/sw/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin",
        "--property",
        &format!("StandardOutput=append:{}", log_path.to_string_lossy()),
        "--property",
        &format!("StandardError=append:{}", log_path.to_string_lossy()),
        "--property",
        &format!("Description=Neo one-shot {} {}", subcommand, suffix),
        &neo_bin,
        subcommand,
    ]);
    let _ = crate::commands::execute_command(&mut run_cmd);
}

/// Create OperationLog, mark triggered, and launch the matching oneshot unit.
fn trigger_oneshot(kind: OneshotKind) -> OperationLog {
    let ts = crate::commands::get_timestamp();
    let op = match kind {
        OneshotKind::Activate => OperationLog::new_activation(&ts),
        OneshotKind::Update => OperationLog::new_update(&ts),
    };
    op.init_for_web_trigger(&ts);
    trigger_systemd_run(
        kind.subcommand(),
        kind.env_var(),
        op.suffix(),
        op.log_path(),
    );
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
    let op = trigger_oneshot(OneshotKind::Update);
    RawHtml(activation::build_update_monitor_fragment(op.id()))
}
