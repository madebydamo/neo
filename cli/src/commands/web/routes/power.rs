//! Host power: restart / shut down (Settings tab) and boot into a generation (versioning).
use rocket::response::content::RawHtml;
use rocket::{get, post};

use crate::commands::web::locks::Blocked;
use crate::commands::web::power::{
    self, going_down_html, hold_until_down, power_lock, PowerAction,
};
use crate::commands::web::util::{alert_html, generation_ok, proc, AlertKind};

fn power_now(
    action: PowerAction,
    kind: &str,
    label: &str,
    msg: &str,
) -> Result<RawHtml<String>, Blocked> {
    let lock = power_lock(kind, label)?;
    power::schedule(action);
    hold_until_down(lock);
    Ok(RawHtml(going_down_html(action, msg)))
}

/// Settings → Power: why the server needs a restart to take an update in
/// full (`neo-restart-required`); empty when it does not.
#[get("/system/restart-required")]
pub async fn system_restart_required() -> RawHtml<String> {
    let status = proc::blocking(crate::commands::activate::restart_required)
        .await
        .unwrap_or_default();
    RawHtml(power::restart_required_html(&status))
}

/// Current kernel boot id, polled by `static/reboot_watch.js` while the host restarts.
#[get("/system/boot-id")]
pub fn system_boot_id() -> String {
    power::boot_id()
}

#[post("/system/reboot")]
pub fn system_reboot() -> Result<RawHtml<String>, Blocked> {
    power_now(
        PowerAction::Reboot,
        "reboot",
        "Restart",
        "Restarting the server…",
    )
}

#[post("/system/poweroff")]
pub fn system_poweroff() -> Result<RawHtml<String>, Blocked> {
    power_now(
        PowerAction::Poweroff,
        "poweroff",
        "Shutdown",
        "Shutting down the server…",
    )
}

/// Set generation `n` as the boot default (`switch-to-configuration boot`, which
/// leaves the running system alone, so it may run in-process) and restart.
#[post("/versioning/generations/<n>/boot")]
pub async fn versioning_gen_boot(n: u64) -> Result<RawHtml<String>, Blocked> {
    if !generation_ok(n) {
        return Ok(RawHtml(alert_html(AlertKind::Error, "invalid generation")));
    }
    let lock = power_lock("generation", &format!("Boot into generation {n}"))?;
    if let Err(e) = power::set_boot_generation(n).await {
        return Ok(RawHtml(alert_html(
            AlertKind::Error,
            &format!("Setting generation {n} as the boot default failed, not restarting: {e}"),
        )));
    }
    power::schedule(PowerAction::Reboot);
    hold_until_down(lock);
    Ok(RawHtml(going_down_html(
        PowerAction::Reboot,
        &format!("Generation {n} is the boot default. Restarting the server into it…"),
    )))
}
