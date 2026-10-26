//! Operation locks in the web UI (see [`crate::utils::locks`] for the model).
//!
//! - Routes take locks with [`try_lock`] / [`probe`] and answer a conflict with
//!   **409** + a plain-text message ([`Blocked`]); `static/locks.js` toasts it.
//! - Long jobs move the [`LockGuard`] into their spawned task, so the scopes are
//!   held until the job ends (or the task is dropped).
//! - [`start_lock_watcher`] pushes `#neo-locks` (live holders of every process:
//!   web jobs, CLI, timers) over the status WS; `static/locks.js` disables every
//!   element whose `data-neo-lock` needs a held scope and says why.
use std::io::Cursor;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use rocket::http::{ContentType, Header, Status};
use rocket::response::{Responder, Response};
use rocket::Request;
use serde_json::json;

use super::types::AppConfig;
use super::util::escape_attr;
use crate::utils::locks::{Holder, LockError, LockGuard, LockManager, LockSpec, OpInfo};

pub use crate::utils::locks::{LockMode, Scope};

/// Process-wide lock manager.
pub fn manager() -> &'static LockManager {
    static M: OnceLock<LockManager> = OnceLock::new();
    M.get_or_init(LockManager::system)
}

/// 409 Conflict with a plain-text reason (shown as a toast by `static/locks.js`).
#[derive(Debug, Clone)]
pub struct Blocked(pub String);

impl From<LockError> for Blocked {
    fn from(e: LockError) -> Self {
        Blocked(e.to_string())
    }
}

impl<'r> Responder<'r, 'static> for Blocked {
    fn respond_to(self, _: &'r Request<'_>) -> rocket::response::Result<'static> {
        Response::build()
            .status(Status::Conflict)
            .header(ContentType::Plain)
            .header(Header::new("X-Neo-Blocked", "1"))
            .sized_body(self.0.len(), Cursor::new(self.0))
            .ok()
    }
}

/// Take `spec` now or fail with the reason.
pub fn try_lock(spec: &LockSpec, info: OpInfo) -> Result<LockGuard, Blocked> {
    manager().try_acquire(spec, &info).map_err(Blocked::from)
}

/// Whether `spec` could be taken right now (taken and released at once).
/// For ops that another process runs (`neo activate` via systemd-run), which
/// then takes the lock itself.
pub fn probe(spec: &LockSpec, info: OpInfo) -> Result<(), Blocked> {
    try_lock(spec, info).map(drop)
}

/// `data-neo-lock` value for elements that need `scopes`
/// (`system`, `service/x:sh`, …; exclusive is the default).
pub fn lock_attr(scopes: &[(Scope, LockMode)]) -> String {
    let v: Vec<String> = scopes
        .iter()
        .map(|(s, m)| match m {
            LockMode::Exclusive => s.key(),
            LockMode::Shared => format!("{}:sh", s.key()),
        })
        .collect();
    format!(r#" data-neo-lock="{}""#, escape_attr(&v.join(" ")))
}

/// Client view of the live holders.
fn holders_json(holders: &[Holder]) -> String {
    let v: Vec<serde_json::Value> = holders
        .iter()
        .map(|h| {
            json!({
                "scopes": h.scopes,
                "kind": h.kind,
                "label": h.label,
                "opId": h.op_id,
                "message": h.message(),
            })
        })
        .collect();
    serde_json::Value::Array(v).to_string()
}

fn element(json: &str, oob: bool) -> String {
    let oob_attr = if oob { r#" hx-swap-oob="true""# } else { "" };
    format!(
        r#"<div id="neo-locks" hidden data-locks="{}"{oob_attr}></div>"#,
        escape_attr(json)
    )
}

/// `#neo-locks` with the current holders (initial page load / WS reconnect).
pub async fn locks_element() -> String {
    let holders = tokio::task::spawn_blocking(|| manager().holders())
        .await
        .unwrap_or_default();
    element(&holders_json(&holders), false)
}

/// OOB `#neo-locks` with the current holders: sent on every `/ws/status` (re)connect,
/// since the watcher only pushes changes and a client that was away (neo-web restart
/// during a switch, lagged channel) would otherwise keep stale holders.
pub async fn locks_oob_element() -> String {
    let holders = tokio::task::spawn_blocking(|| manager().holders())
        .await
        .unwrap_or_default();
    element(&holders_json(&holders), true)
}

/// Push `#neo-locks` whenever the set of holders changes. Also cleans up
/// holders (and unit start guards) of processes that died.
pub fn start_lock_watcher(config: Arc<AppConfig>) {
    tokio::spawn(async move {
        let mut last = String::new();
        loop {
            let holders = tokio::task::spawn_blocking(|| manager().holders())
                .await
                .unwrap_or_default();
            let json = holders_json(&holders);
            if json != last {
                let _ = config.unit_updates.send(element(&json, true));
                last = json;
            }
            tokio::time::sleep(Duration::from_millis(1000)).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lock_attr_modes() {
        let a = lock_attr(&[
            (Scope::System, LockMode::Shared),
            (Scope::service("calino"), LockMode::Exclusive),
        ]);
        assert_eq!(a, r#" data-neo-lock="system:sh service/calino""#);
    }

    #[test]
    fn element_escapes_json() {
        let e = element(r#"[{"label":"a<b"}]"#, true);
        assert!(e.contains("&quot;label&quot;"));
        assert!(e.contains("a&lt;b"));
        assert!(e.contains(r#"hx-swap-oob="true""#));
    }
}
