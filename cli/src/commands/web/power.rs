//! Host restart / shutdown from the web UI (Settings → Power, versioning
//! "Boot into generation", data restore).
use std::time::Duration;

use super::locks::{try_lock, Blocked};
use super::util::{escape_html, proc};
use crate::utils::locks::{LockGuard, LockSpec, OpInfo};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PowerAction {
    Reboot,
    Poweroff,
}

impl PowerAction {
    /// `systemctl` verb.
    fn verb(self) -> &'static str {
        match self {
            PowerAction::Reboot => "reboot",
            PowerAction::Poweroff => "poweroff",
        }
    }
}

/// System scope exclusive: no activation / update / restore may run into a power cycle.
pub fn power_lock(kind: &str, label: &str) -> Result<LockGuard, Blocked> {
    try_lock(&LockSpec::system_change(), OpInfo::new(kind, label))
}

/// Keep `lock` until the host goes down, so nothing new starts in the delay.
pub fn hold_until_down(lock: LockGuard) {
    tokio::spawn(async move {
        let _lock = lock;
        tokio::time::sleep(Duration::from_secs(300)).await;
    });
}

/// Run `systemctl <action>` shortly after answering, so the response reaches the browser.
pub fn schedule(action: PowerAction) {
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(2)).await;
        let what = format!("systemctl {}", action.verb());
        let mut cmd = proc::sudo();
        cmd.args(["systemctl", action.verb(), "--no-ask-password"]);
        if let Err(e) = proc::run_ok(&mut cmd, &what).await {
            eprintln!("web: {e}");
        }
    });
}

/// Kernel boot id: changes on every boot, so the browser can tell the restarted host
/// from the one that is still going down.
pub fn boot_id() -> String {
    std::fs::read_to_string("/proc/sys/kernel/random/boot_id")
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

/// `data-neo-await-boot` attribute: `static/reboot_watch.js` polls `/system/boot-id`
/// and reloads the page once a different boot answers.
pub fn await_boot_attr() -> String {
    format!(r#"data-neo-await-boot="{}""#, escape_html(&boot_id()))
}

/// Notice shown while the host goes down.
pub fn going_down_html(action: PowerAction, msg: &str) -> String {
    let (attr, hint) = match action {
        PowerAction::Reboot => (
            await_boot_attr(),
            "This page reloads once the server is back.",
        ),
        PowerAction::Poweroff => (
            String::new(),
            "The server stays off until it is powered on again (physically or via its management interface).",
        ),
    };
    format!(
        r#"<div {attr}><div role="alert" class="alert alert-warning text-sm"><span class="loading loading-spinner loading-sm"></span><span data-neo-await-boot-msg>{}</span><span data-neo-await-boot-elapsed class="ml-auto tabular-nums opacity-70"></span></div><p class="text-xs opacity-70 mt-2">{hint}</p></div>"#,
        escape_html(msg)
    )
}
