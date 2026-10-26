//! Host restart / shutdown from the web UI (Settings → Power, versioning
//! "Boot into generation", data restore).
use std::time::Duration;

use tokio::process::Command as AsyncCommand;

use super::locks::{try_lock, Blocked};
use super::util::{escape_html, sudo_cmd};
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
        let out = AsyncCommand::new(sudo_cmd())
            .args(["-n", "systemctl", action.verb(), "--no-ask-password"])
            .output()
            .await;
        match out {
            Ok(o) if !o.status.success() => eprintln!(
                "web: systemctl {}: {}",
                action.verb(),
                String::from_utf8_lossy(&o.stderr).trim()
            ),
            Err(e) => eprintln!("web: systemctl {}: {e}", action.verb()),
            _ => {}
        }
    });
}

/// Notice shown while the host goes down.
pub fn going_down_html(action: PowerAction, msg: &str) -> String {
    let hint = match action {
        PowerAction::Reboot => "The UI reconnects once the server is back.",
        PowerAction::Poweroff => {
            "The server stays off until it is powered on again (physically or via its management interface)."
        }
    };
    format!(
        r#"<div role="alert" class="alert alert-warning text-sm"><span class="loading loading-spinner loading-sm"></span><span>{}</span></div><p class="text-xs opacity-70 mt-2">{hint}</p>"#,
        escape_html(msg)
    )
}
