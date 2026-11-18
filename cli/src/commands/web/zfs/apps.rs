//! App versions after a restore (docs/design/docker-image-retention.md): the
//! docker updater's image ledger says which image ran when, and
//! `neo-docker-rollback --at <snapshot time>` returns apps to it (retag, or
//! pull by digest). Settings are never changed; the next scheduled update moves
//! forward again.
use std::path::Path;

use super::super::util::proc;
use crate::utils::docker_ledger::{self, Ledger, Pick, Target, LEDGER_PATH};

/// `neo-docker-rollback` (docker-updater module; allowed for sudo there).
fn rollback_bin() -> String {
    std::env::var("NEO_DOCKER_ROLLBACK")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "/run/current-system/sw/bin/neo-docker-rollback".to_string())
}

/// The image ledger (empty when the docker updater never ran or it is unreadable).
pub fn ledger() -> Ledger {
    docker_ledger::load(Path::new(LEDGER_PATH)).unwrap_or_default()
}

/// Apps (image references) that ran another version at `at`. Restricted to
/// `units` when given; `None` means every app.
pub fn versions_at(ledger: &Ledger, at: i64, units: Option<&[String]>) -> Vec<Pick> {
    if units.is_some_and(|u| u.is_empty()) {
        return Vec::new();
    }
    docker_ledger::select(
        std::slice::from_ref(ledger),
        Target::At(at),
        &[],
        units.unwrap_or(&[]),
    )
}

/// Short list for a title or confirm text: `immich-server, redis (+2)`.
pub fn names(picks: &[Pick]) -> String {
    let mut v: Vec<String> = picks
        .iter()
        .map(|p| {
            let repo = docker_ledger::repo_of(&p.reference);
            repo.rsplit('/').next().unwrap_or(&repo).to_string()
        })
        .collect();
    v.dedup();
    if v.len() > 4 {
        let more = v.len() - 4;
        v.truncate(4);
        format!("{} (+{more})", v.join(", "))
    } else {
        v.join(", ")
    }
}

/// Run `neo-docker-rollback --at <at>` for `units` (every app when `None`).
/// `restart = false`: retag only, the caller starts the units.
pub async fn rollback(at: i64, units: Option<&[String]>, restart: bool) -> Result<String, String> {
    let mut cmd = proc::sudo();
    cmd.arg(rollback_bin()).arg("--at").arg(at.to_string());
    for u in units.unwrap_or(&[]) {
        cmd.arg("--unit").arg(u);
    }
    if !restart {
        cmd.arg("--no-restart");
    }
    let out = cmd
        .output()
        .await
        .map_err(|e| format!("neo-docker-rollback: {e}"))?;
    let text = |b: &[u8]| {
        String::from_utf8_lossy(b)
            .lines()
            .rev()
            .find(|l| !l.trim().is_empty())
            .unwrap_or("")
            .trim()
            .to_string()
    };
    if out.status.success() {
        Ok(text(&out.stdout))
    } else {
        let err = text(&out.stderr);
        Err(if err.is_empty() {
            format!("neo-docker-rollback failed ({})", out.status)
        } else {
            err
        })
    }
}
