//! Take a service's units down for work on its data (clear appdata, snapshot
//! restore) and bring back exactly what was running.
//!
//! The caller holds the service's operation lock; a **start guard** on its units
//! keeps them down meanwhile, so neither the UI, the CLI, a timer, a dependency, an
//! activation, nor a manual `systemctl start` can bring the service up mid-work.
use std::time::{Duration, Instant};

use super::super::types::AppConfig;
use super::control::{broadcast_unit_update, systemctl, UnitAction};
use super::status::query_unit_states;
use crate::utils::locks::LockGuard;

/// Units never stopped: stopping them kills the web UI running the job.
/// Same exclusions as the service targets (nix/lib/service-targets.nix).
const NEVER_STOP: &[&str] = &["neo-web", "neo-bootstrap"];

fn unit_is_stopped(state: &str) -> bool {
    matches!(state, "inactive" | "failed" | "dead" | "not-found")
}

/// Whether a unit was up, so it gets started again afterwards.
/// Inactive / failed / not-found units stay down.
fn unit_was_running(state: &str) -> bool {
    matches!(state, "active" | "activating" | "reloading")
}

async fn units_currently_running(units: &[String]) -> Vec<String> {
    let states = query_unit_states(units).await;
    units
        .iter()
        .filter(|u| states.get(*u).is_some_and(|s| unit_was_running(&s.active)))
        .cloned()
        .collect()
}

async fn wait_units_stopped(units: &[String], timeout: Duration) -> Result<(), String> {
    if units.is_empty() {
        return Ok(());
    }
    let deadline = Instant::now() + timeout;
    loop {
        let states = query_unit_states(units).await;
        let pending: Vec<String> = states
            .iter()
            .filter(|(_, s)| !unit_is_stopped(&s.active))
            .map(|(u, s)| format!("{u}={}", s.active))
            .collect();
        if pending.is_empty() {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "timeout waiting for units to stop ({})",
                pending.join(", ")
            ));
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

/// Starting a target pulls in all its units, so it only comes back when every unit
/// was up: a unit the operator stopped stays down, the others start one by one.
fn restart_target(target_was_active: bool, units: usize, running: usize) -> bool {
    target_was_active && running == units
}

/// What [`quiesce`] stopped. [`Quiesced::restart`] undoes it on every path.
#[must_use]
pub struct Quiesced {
    to_restart: Vec<String>,
    group: Option<String>,
    restart_target: bool,
}

impl Quiesced {
    /// Anything to start again (for a status message).
    pub fn will_start(&self) -> bool {
        self.restart_target || !self.to_restart.is_empty()
    }

    /// Release the start guards and start what was running.
    pub async fn restart(self, lock: &mut LockGuard, config: &AppConfig) {
        lock.release_unit_guards();
        if let (Some(t), true) = (&self.group, self.restart_target) {
            if let Err(e) = systemctl(UnitAction::Start, t).await {
                eprintln!("web: start {t}: {e}");
            }
        }
        for u in &self.to_restart {
            if let Err(e) = systemctl(UnitAction::Start, u).await {
                eprintln!("web: start {u}: {e}");
            }
            broadcast_unit_update(u, config).await;
        }
    }
}

/// Guard `units` and `group`, stop them and wait until they are down. On a timeout
/// everything is started again and the error returned.
pub async fn quiesce(
    units: &[String],
    group: Option<&str>,
    timeout: Duration,
    lock: &mut LockGuard,
    config: &AppConfig,
) -> Result<Quiesced, String> {
    let units: Vec<String> = units
        .iter()
        .filter(|u| !NEVER_STOP.contains(&u.as_str()))
        .cloned()
        .collect();
    let group = group.map(str::to_string);
    let to_restart = units_currently_running(&units).await;
    let target_was_active = match &group {
        Some(t) => !units_currently_running(std::slice::from_ref(t))
            .await
            .is_empty(),
        None => false,
    };
    let quiesced = Quiesced {
        restart_target: restart_target(target_was_active, units.len(), to_restart.len()),
        to_restart,
        group: group.clone(),
    };

    // systemd refuses to start these until the guards are released.
    let guarded: Vec<String> = units.iter().cloned().chain(group.clone()).collect();
    lock.guard_units(&guarded)
        .map_err(|e| format!("cannot protect units meanwhile: {e}"))?;

    // The target first, so it does not pull units back in. A failed stop is not
    // fatal here: waiting below reports units that stay up.
    for u in group.iter().chain(&units) {
        if let Err(e) = systemctl(UnitAction::Stop, u).await {
            eprintln!("web: stop {u}: {e}");
        }
    }
    for u in &units {
        broadcast_unit_update(u, config).await;
    }

    if let Err(e) = wait_units_stopped(&units, timeout).await {
        quiesced.restart(lock, config).await;
        return Err(e);
    }
    Ok(quiesced)
}

#[cfg(test)]
mod tests {
    use super::{restart_target, unit_was_running};

    #[test]
    fn target_comes_back_only_when_no_unit_was_stopped() {
        assert!(restart_target(true, 2, 2));
        assert!(!restart_target(true, 2, 1));
        assert!(!restart_target(false, 2, 2));
    }

    #[test]
    fn unit_was_running_only_up_states() {
        assert!(unit_was_running("active"));
        assert!(unit_was_running("activating"));
        assert!(unit_was_running("reloading"));
        assert!(!unit_was_running("inactive"));
        assert!(!unit_was_running("failed"));
        assert!(!unit_was_running("dead"));
        assert!(!unit_was_running("not-found"));
        assert!(!unit_was_running("deactivating"));
        assert!(!unit_was_running("unknown"));
    }
}
