use std::process::Command;

use rocket::response::content::RawHtml;

use crate::commands::log::OperationLog;

use super::activation;
use super::structs::AppConfig;

pub fn trigger_systemd_run(
    subcommand: &str,
    env_var: &str,
    suffix: &str,
    log_path: &std::path::Path,
) {
    let sudo_cmd = std::env::var("SUDO_BINARY_PATH").unwrap_or_else(|_| "sudo".to_string());
    let nix_bin = std::env::var("NIX_BINARY_PATH")
        .unwrap_or_else(|_| "/run/current-system/sw/bin/nix".to_string());
    let unit = format!("neo-{}@{}.service", subcommand, suffix);
    let neo_bin = "/run/current-system/sw/bin/neo";
    let desc = format!("{} systemd-run --unit={} (as homeserver)", sudo_cmd, unit);
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
        neo_bin,
        subcommand,
    ]);
    let _ = crate::commands::execute_command(&mut run_cmd, &desc);
}

pub fn trigger_activation(_config: &AppConfig) -> RawHtml<String> {
    activation::gc_old_activations();
    let ts = crate::commands::get_timestamp();
    let op = OperationLog::new_activation(&ts);
    op.init_for_web_trigger(&ts);
    if let Some(other) = activation::find_recent_in_progress_activation() {
        if other != op.id() {
            return RawHtml(format!("<div class=\"alert alert-error text-sm\">Another activation {} in progress (or auto-update). Wait.</div>", other));
        }
    }
    trigger_systemd_run(
        "activate",
        "NEO_ACTIVATION_SUFFIX",
        op.suffix(),
        op.log_path(),
    );
    // If launch failed synchronously the state remains "triggered"; the unit would update it on real run.
    RawHtml(activation::build_monitor_fragment(op.id()))
}
