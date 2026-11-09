//! Batched systemd unit status for many services at once.
//!
//! One `systemctl show --property=Id,LoadState,ActiveState,SubState -- u1 u2 …`
//! process answers every unit on the page; [`UnitStatusCache`] keeps the result for
//! a few seconds and serializes refreshes (single-flight) so concurrent page loads
//! and pollers share one query instead of hitting systemd per unit.
//!
//! The per-service summary (running / failed / …) is computed client-side by
//! `NeoUnitHealth.summarize` (static/service_status.js), the same function the
//! option pane header uses, so the grid dots and the pane badge always agree.
use std::collections::{BTreeMap, HashMap, HashSet};
use std::future::Future;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tokio::process::Command as AsyncCommand;
use tokio::sync::Mutex as AsyncMutex;

use super::super::util::{service_name_ok, systemctl_bin, unit_name_valid};

/// Properties requested from systemd (order in the output is systemd's, not ours).
/// Type / RemainAfterExit / Result / ExecMainExitTimestampMonotonic tell a finished
/// setup or oneshot job ("done") apart from a stopped daemon; see [`UnitHealth`].
const SHOW_PROPERTIES: &str =
    "Id,LoadState,ActiveState,SubState,Type,RemainAfterExit,Result,ExecMainExitTimestampMonotonic";
/// Upper bound on units per batched query (guards the argv and the cache).
pub const MAX_UNITS: usize = 512;
/// Upper bound on services per status request.
pub const MAX_SERVICES: usize = 256;
/// A wedged systemd must not hang the status endpoint (or the cache lock) forever.
const QUERY_TIMEOUT: Duration = Duration::from_secs(5);
/// Page-level status cache lifetime: short enough to feel live, long enough that
/// many tabs / pollers collapse onto one systemctl call.
pub const STATUS_CACHE_TTL: Duration = Duration::from_secs(3);

const UNIT_SUFFIXES: &[&str] = &[
    "service",
    "target",
    "timer",
    "socket",
    "mount",
    "automount",
    "path",
    "slice",
    "scope",
    "device",
    "swap",
];

/// Load/active/sub state of one systemd unit.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct UnitStatus {
    /// ActiveState (`active`, `inactive`, `failed`, `activating`, …) or `unknown`.
    pub active: String,
    /// SubState (`running`, `dead`, `exited`, …); empty when unknown.
    pub sub: String,
    /// LoadState (`loaded`, `not-found`, …); empty when unknown.
    pub load: String,
    /// Service Type (`simple`, `oneshot`, `exec`, …); empty for non-services.
    #[serde(skip)]
    pub unit_type: String,
    /// RemainAfterExit=yes.
    #[serde(skip)]
    pub remain_after_exit: bool,
    /// Result of the last run (`success`, `exit-code`, …); empty when unknown.
    #[serde(skip)]
    pub result: String,
    /// The main process has exited at least once since the unit was loaded.
    #[serde(skip)]
    pub has_run: bool,
}

/// What a unit's state means for the service, derived from systemd properties only
/// (no unit-name matching). Shared by the Status tab rows (control.rs) and the
/// `/status/services` summary (service_status.js).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum UnitHealth {
    /// Long-running process up.
    Running,
    /// Setup / oneshot finished successfully: `active (exited)` with RemainAfterExit,
    /// or an inactive oneshot job whose last run succeeded.
    Done,
    /// Setup still working or retrying: Type=simple + RemainAfterExit while its
    /// script runs, or a RemainAfterExit oneshot in its start job.
    Setup,
    /// Other transitional state (activating, deactivating, reloading, …).
    Changing,
    /// Oneshot job that never ran yet (waits for a trigger or timer).
    Idle,
    /// Not running.
    Stopped,
    Failed,
    /// Unit file not on the host (config not activated yet).
    Missing,
    Unknown,
}

impl UnitHealth {
    /// `.neo-unit-dot` data-state.
    pub fn dot(self) -> &'static str {
        match self {
            UnitHealth::Running | UnitHealth::Done => "ok",
            UnitHealth::Setup | UnitHealth::Changing => "changing",
            UnitHealth::Idle | UnitHealth::Stopped | UnitHealth::Missing => "none",
            UnitHealth::Failed => "failed",
            UnitHealth::Unknown => "partial",
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            UnitHealth::Running => "running",
            UnitHealth::Done => "done",
            UnitHealth::Setup => "setup",
            UnitHealth::Changing => "changing",
            UnitHealth::Idle => "idle",
            UnitHealth::Stopped => "stopped",
            UnitHealth::Failed => "failed",
            UnitHealth::Missing => "missing",
            UnitHealth::Unknown => "unknown",
        }
    }
}

impl UnitStatus {
    pub fn unknown() -> Self {
        Self {
            active: "unknown".into(),
            ..Self::default()
        }
    }

    fn is_simple(&self) -> bool {
        matches!(self.unit_type.as_str(), "simple" | "exec")
    }

    pub fn health(&self) -> UnitHealth {
        if self.load == "not-found" && self.active != "failed" {
            return UnitHealth::Missing;
        }
        match self.active.as_str() {
            "failed" => UnitHealth::Failed,
            "active" => {
                if self.sub == "exited" {
                    UnitHealth::Done
                } else if self.remain_after_exit && self.is_simple() && self.sub == "running" {
                    // Non-blocking setup (mkSetupService): active from the start, its
                    // script is still working. It becomes `exited` once it succeeded.
                    UnitHealth::Setup
                } else {
                    UnitHealth::Running
                }
            }
            // Blocking setup (oneshot + RemainAfterExit) in its start job or
            // waiting to be restarted after a failed attempt.
            "activating" if self.remain_after_exit => UnitHealth::Setup,
            "activating" | "deactivating" | "reloading" | "refreshing" => UnitHealth::Changing,
            "inactive" => {
                // RemainAfterExit units stay active after success; inactive means stopped.
                if self.unit_type == "oneshot" && !self.remain_after_exit {
                    if !self.has_run {
                        UnitHealth::Idle
                    } else if self.result == "success" {
                        UnitHealth::Done
                    } else {
                        UnitHealth::Stopped
                    }
                } else {
                    UnitHealth::Stopped
                }
            }
            "unknown" | "" => UnitHealth::Unknown,
            _ => UnitHealth::Changing,
        }
    }

    /// Short label for the Status tab row.
    pub fn label(&self) -> String {
        match self.health() {
            UnitHealth::Running => "running".into(),
            UnitHealth::Done => "done".into(),
            UnitHealth::Setup if self.sub == "auto-restart" => "retrying".into(),
            UnitHealth::Setup => "setting up".into(),
            UnitHealth::Idle => "idle".into(),
            UnitHealth::Stopped => "stopped".into(),
            UnitHealth::Failed => "failed".into(),
            UnitHealth::Missing => "not deployed".into(),
            UnitHealth::Unknown => "unknown".into(),
            UnitHealth::Changing => self.active.clone(),
        }
    }

    /// Tooltip: raw systemd state, e.g. `active (exited), result success`.
    pub fn detail(&self) -> String {
        let mut s = self.active.clone();
        if !self.sub.is_empty() {
            s.push_str(&format!(" ({})", self.sub));
        }
        if !self.result.is_empty() && self.result != "success" {
            s.push_str(&format!(", result {}", self.result));
        }
        s
    }
}

/// `docker-foo` → `docker-foo.service`; names with a unit-type suffix stay as-is.
fn canonical_unit_id(name: &str) -> String {
    match name.rsplit_once('.') {
        Some((_, suffix)) if UNIT_SUFFIXES.contains(&suffix) => name.to_string(),
        _ => format!("{name}.service"),
    }
}

fn block_status(block: &HashMap<&str, &str>) -> UnitStatus {
    let get = |k: &str| block.get(k).map(|v| v.trim()).unwrap_or("").to_string();
    let active = get("ActiveState");
    UnitStatus {
        active: if active.is_empty() {
            "unknown".into()
        } else {
            active
        },
        sub: get("SubState"),
        load: get("LoadState"),
        unit_type: get("Type"),
        remain_after_exit: get("RemainAfterExit") == "yes",
        result: get("Result"),
        has_run: get("ExecMainExitTimestampMonotonic")
            .parse::<u64>()
            .is_ok_and(|t| t > 0),
    }
}

/// Parse `systemctl show` output for `requested` units (in the order they were passed).
///
/// systemd prints one blank-line-separated `Key=Value` block per argument, in argument
/// order. Aliases resolve to the real unit (`dbus` → `Id=dbus-broker.service`), so the
/// primary mapping is positional; if the block count does not line up (partial output),
/// fall back to matching `Id` against the canonical unit name. Anything unmatched is
/// `unknown`.
pub fn parse_show_output(stdout: &str, requested: &[String]) -> HashMap<String, UnitStatus> {
    let mut blocks: Vec<HashMap<&str, &str>> = Vec::new();
    let mut cur: HashMap<&str, &str> = HashMap::new();
    for line in stdout.lines() {
        let line = line.trim_end_matches('\r');
        if line.trim().is_empty() {
            if !cur.is_empty() {
                blocks.push(std::mem::take(&mut cur));
            }
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            cur.insert(k.trim(), v);
        }
    }
    if !cur.is_empty() {
        blocks.push(cur);
    }

    let mut out = HashMap::with_capacity(requested.len());
    if blocks.len() == requested.len() {
        for (name, block) in requested.iter().zip(blocks.iter()) {
            out.insert(name.clone(), block_status(block));
        }
        return out;
    }

    let by_id: HashMap<&str, &HashMap<&str, &str>> = blocks
        .iter()
        .filter_map(|b| b.get("Id").map(|id| (id.trim(), b)))
        .collect();
    for name in requested {
        let status = by_id
            .get(canonical_unit_id(name).as_str())
            .map(|b| block_status(b))
            .unwrap_or_else(UnitStatus::unknown);
        out.insert(name.clone(), status);
    }
    out
}

/// Valid, de-duplicated unit names (first occurrence order), capped at [`MAX_UNITS`].
fn sanitize_units<'a>(units: impl IntoIterator<Item = &'a String>) -> Vec<String> {
    let mut seen = HashSet::new();
    units
        .into_iter()
        .filter(|u| unit_name_valid(u) && !u.starts_with('-'))
        .filter(|u| seen.insert(u.as_str()))
        .take(MAX_UNITS)
        .cloned()
        .collect()
}

fn all_unknown(units: &[String]) -> HashMap<String, UnitStatus> {
    units
        .iter()
        .map(|u| (u.clone(), UnitStatus::unknown()))
        .collect()
}

/// One `systemctl show` for all `units`. Never errors: when systemctl is missing,
/// times out or prints nothing, every unit is `unknown`.
///
/// Read-only property queries need no privileges, so this runs systemctl directly
/// (no sudo → no per-call auth log noise).
pub async fn query_unit_states(units: &[String]) -> HashMap<String, UnitStatus> {
    let units = sanitize_units(units);
    if units.is_empty() {
        return HashMap::new();
    }
    let mut cmd = AsyncCommand::new(systemctl_bin());
    cmd.arg("show")
        .arg("--no-pager")
        .arg(format!("--property={SHOW_PROPERTIES}"))
        .arg("--")
        .args(&units)
        .stdin(std::process::Stdio::null())
        .kill_on_drop(true);
    match tokio::time::timeout(QUERY_TIMEOUT, cmd.output()).await {
        Ok(Ok(o)) if !o.stdout.is_empty() => {
            parse_show_output(&String::from_utf8_lossy(&o.stdout), &units)
        }
        _ => all_unknown(&units),
    }
}

/// Single-unit [`query_unit_states`]; `unknown` on any error.
pub async fn query_unit_status(unit: &str) -> UnitStatus {
    query_unit_states(&[unit.to_string()])
        .await
        .remove(unit)
        .unwrap_or_else(UnitStatus::unknown)
}

/// Short-lived, single-flight cache of unit states shared by all status requests.
///
/// The async mutex is held across the refresh, so concurrent callers queue behind
/// the in-flight query and then read its fresh result instead of spawning their own.
#[derive(Debug)]
pub struct UnitStatusCache {
    ttl: Duration,
    entries: AsyncMutex<HashMap<String, (Instant, UnitStatus)>>,
}

impl Default for UnitStatusCache {
    fn default() -> Self {
        Self::new(STATUS_CACHE_TTL)
    }
}

impl UnitStatusCache {
    pub fn new(ttl: Duration) -> Self {
        Self {
            ttl,
            entries: AsyncMutex::new(HashMap::new()),
        }
    }

    /// States for `units`, refreshing stale/missing ones with one batched systemctl call.
    pub async fn get(&self, units: &[String]) -> HashMap<String, UnitStatus> {
        self.get_with(
            units,
            |missing| async move { query_unit_states(&missing).await },
        )
        .await
    }

    /// [`Self::get`] with an injectable batch fetcher (tests).
    pub async fn get_with<F, Fut>(&self, units: &[String], fetch: F) -> HashMap<String, UnitStatus>
    where
        F: FnOnce(Vec<String>) -> Fut,
        Fut: Future<Output = HashMap<String, UnitStatus>>,
    {
        let units = sanitize_units(units);
        if units.is_empty() {
            return HashMap::new();
        }
        let mut entries = self.entries.lock().await;
        let now = Instant::now();
        let stale: Vec<String> = units
            .iter()
            .filter(|u| {
                entries
                    .get(u.as_str())
                    .is_none_or(|(at, _)| now.duration_since(*at) >= self.ttl)
            })
            .cloned()
            .collect();
        if !stale.is_empty() {
            let fetched = fetch(stale.clone()).await;
            let at = Instant::now();
            for u in stale {
                let st = fetched.get(&u).cloned().unwrap_or_else(UnitStatus::unknown);
                entries.insert(u, (at, st));
            }
            // Bound memory: drop entries nobody asked about for a while.
            let keep = self.ttl * 20;
            entries.retain(|_, (t, _)| at.duration_since(*t) < keep);
        }
        units
            .into_iter()
            .map(|u| {
                let st = entries
                    .get(&u)
                    .map(|(_, s)| s.clone())
                    .unwrap_or_else(UnitStatus::unknown);
                (u, st)
            })
            .collect()
    }
}

/// Units + timer-backed units the page wants for one service.
#[derive(Deserialize, Default, Debug, Clone)]
pub struct ServiceUnitsRequest {
    #[serde(default)]
    pub units: Vec<String>,
    #[serde(default)]
    pub timers: Vec<String>,
}

/// Body of `POST /status/services`: every service with a dot on the page.
#[derive(Deserialize, Default, Debug, Clone)]
pub struct ServiceStatusRequest {
    #[serde(default)]
    pub services: BTreeMap<String, ServiceUnitsRequest>,
}

/// One unit row in the response (input shape for `NeoUnitHealth.summarize`).
#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
pub struct UnitStatusEntry {
    pub name: String,
    pub active: String,
    pub sub: String,
    pub load: String,
    /// [`UnitHealth`] from the systemd properties (running / done / setup / …).
    pub health: UnitHealth,
    pub timer: bool,
}

#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
pub struct ServiceStatus {
    pub units: Vec<UnitStatusEntry>,
}

#[derive(Serialize, Debug, Default)]
pub struct ServiceStatusResponse {
    pub services: BTreeMap<String, ServiceStatus>,
}

impl ServiceStatusRequest {
    /// Valid services (capped), each with its valid units.
    fn services(&self) -> impl Iterator<Item = (&String, &ServiceUnitsRequest)> {
        self.services
            .iter()
            .filter(|(name, _)| service_name_ok(name))
            .take(MAX_SERVICES)
    }

    /// De-duplicated union of every requested unit → the argv of the one batched query.
    pub fn all_units(&self) -> Vec<String> {
        sanitize_units(self.services().flat_map(|(_, s)| s.units.iter()))
    }
}

/// Group batched unit states back onto the services that asked for them.
pub fn map_services(
    req: &ServiceStatusRequest,
    states: &HashMap<String, UnitStatus>,
) -> ServiceStatusResponse {
    let mut services = BTreeMap::new();
    for (name, svc) in req.services() {
        let units = sanitize_units(svc.units.iter())
            .into_iter()
            .map(|u| {
                let st = states.get(&u).cloned().unwrap_or_else(UnitStatus::unknown);
                UnitStatusEntry {
                    health: st.health(),
                    timer: svc.timers.contains(&u),
                    name: u,
                    active: st.active,
                    sub: st.sub,
                    load: st.load,
                }
            })
            .collect();
        services.insert(name.clone(), ServiceStatus { units });
    }
    ServiceStatusResponse { services }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    fn st(active: &str, sub: &str, load: &str) -> UnitStatus {
        UnitStatus {
            active: active.into(),
            sub: sub.into(),
            load: load.into(),
            ..UnitStatus::default()
        }
    }

    /// Service status with the setup-relevant properties.
    fn svc(
        active: &str,
        sub: &str,
        unit_type: &str,
        remain: bool,
        result: &str,
        has_run: bool,
    ) -> UnitStatus {
        UnitStatus {
            active: active.into(),
            sub: sub.into(),
            load: "loaded".into(),
            unit_type: unit_type.into(),
            remain_after_exit: remain,
            result: result.into(),
            has_run,
        }
    }

    #[test]
    fn parse_reads_setup_properties() {
        let out = "Id=collabora-setup.service\nLoadState=loaded\nActiveState=active\nSubState=exited\n\
                   Type=simple\nRemainAfterExit=yes\nResult=success\nExecMainExitTimestampMonotonic=123456\n\n\
                   Id=backup.service\nLoadState=loaded\nActiveState=inactive\nSubState=dead\n\
                   Type=oneshot\nRemainAfterExit=no\nResult=success\nExecMainExitTimestampMonotonic=0\n";
        let m = parse_show_output(out, &s(&["collabora-setup", "backup"]));
        let setup = &m["collabora-setup"];
        assert_eq!(setup.unit_type, "simple");
        assert!(setup.remain_after_exit);
        assert!(setup.has_run);
        assert_eq!(setup.health(), UnitHealth::Done);
        let backup = &m["backup"];
        assert!(!backup.remain_after_exit);
        assert!(!backup.has_run);
        assert_eq!(backup.health(), UnitHealth::Idle);
    }

    #[test]
    fn health_setup_units() {
        // mkSetupService non-blocking: script still retrying → setup, then done.
        let running = svc("active", "running", "simple", true, "success", false);
        assert_eq!(running.health(), UnitHealth::Setup);
        assert_eq!(running.label(), "setting up");
        let done = svc("active", "exited", "simple", true, "success", true);
        assert_eq!(done.health(), UnitHealth::Done);
        assert_eq!(done.label(), "done");
        assert_eq!(done.health().dot(), "ok");
        // Blocking (oneshot) setup in its start job / between restarts.
        let start = svc("activating", "start", "oneshot", true, "success", false);
        assert_eq!(start.health(), UnitHealth::Setup);
        let retry = svc(
            "activating",
            "auto-restart",
            "oneshot",
            true,
            "exit-code",
            true,
        );
        assert_eq!(retry.health(), UnitHealth::Setup);
        assert_eq!(retry.label(), "retrying");
        // Failed stays failed; a stopped RemainAfterExit setup is stopped, not done.
        let failed = svc("failed", "failed", "simple", true, "exit-code", true);
        assert_eq!(failed.health(), UnitHealth::Failed);
        let stopped = svc("inactive", "dead", "oneshot", true, "success", true);
        assert_eq!(stopped.health(), UnitHealth::Stopped);
    }

    #[test]
    fn health_oneshot_jobs_and_daemons() {
        // Timer job: never ran → idle; last run ok → done; last run bad → stopped.
        assert_eq!(
            svc("inactive", "dead", "oneshot", false, "success", false).health(),
            UnitHealth::Idle
        );
        assert_eq!(
            svc("inactive", "dead", "oneshot", false, "success", true).health(),
            UnitHealth::Done
        );
        assert_eq!(
            svc("inactive", "dead", "oneshot", false, "exit-code", true).health(),
            UnitHealth::Stopped
        );
        // Job in progress is plain activating, not setup.
        let job = svc("activating", "start", "oneshot", false, "success", false);
        assert_eq!(job.health(), UnitHealth::Changing);
        assert_eq!(job.label(), "activating");
        // Daemons.
        assert_eq!(
            svc("active", "running", "simple", false, "success", false).health(),
            UnitHealth::Running
        );
        assert_eq!(
            svc("inactive", "dead", "simple", false, "success", true).health(),
            UnitHealth::Stopped
        );
        assert_eq!(
            st("active", "running", "loaded").health(),
            UnitHealth::Running
        );
        assert_eq!(
            st("inactive", "dead", "not-found").health(),
            UnitHealth::Missing
        );
        assert_eq!(UnitStatus::unknown().health(), UnitHealth::Unknown);
        assert_eq!(
            st("active", "waiting", "loaded").health(),
            UnitHealth::Running
        );
    }

    #[test]
    fn health_serializes_lowercase() {
        assert_eq!(
            serde_json::to_string(&UnitHealth::Done).unwrap(),
            "\"done\""
        );
        assert_eq!(UnitHealth::Setup.as_str(), "setup");
    }

    const SAMPLE: &str = "Id=dbus-broker.service\nLoadState=loaded\nActiveState=active\nSubState=running\n\nId=docker-immich.service\nLoadState=loaded\nActiveState=failed\nSubState=failed\n\nId=docker-gone.service\nLoadState=not-found\nActiveState=inactive\nSubState=dead\n";

    #[test]
    fn parse_positional_handles_aliases() {
        let req = s(&["dbus", "docker-immich", "docker-gone"]);
        let m = parse_show_output(SAMPLE, &req);
        assert_eq!(m["dbus"], st("active", "running", "loaded"));
        assert_eq!(m["docker-immich"], st("failed", "failed", "loaded"));
        assert_eq!(m["docker-gone"], st("inactive", "dead", "not-found"));
    }

    #[test]
    fn parse_property_order_does_not_matter() {
        let out = "ActiveState=activating\nSubState=start\nId=neo-x.target\nLoadState=loaded\n";
        let m = parse_show_output(out, &s(&["neo-x.target"]));
        assert_eq!(m["neo-x.target"], st("activating", "start", "loaded"));
    }

    #[test]
    fn parse_count_mismatch_falls_back_to_id_match() {
        // Only two blocks for three units: match by canonical Id, rest unknown.
        let out = "Id=docker-a.service\nActiveState=active\nSubState=running\nLoadState=loaded\n\n\nId=b.timer\nActiveState=active\nSubState=waiting\nLoadState=loaded\n";
        let m = parse_show_output(out, &s(&["docker-a", "b.timer", "c"]));
        assert_eq!(m["docker-a"].active, "active");
        assert_eq!(m["b.timer"].sub, "waiting");
        assert_eq!(m["c"], UnitStatus::unknown());
    }

    #[test]
    fn parse_empty_or_garbage_is_unknown() {
        let m = parse_show_output("", &s(&["a", "b"]));
        assert_eq!(m["a"], UnitStatus::unknown());
        assert_eq!(m["b"], UnitStatus::unknown());
        let m = parse_show_output("Id=a.service\nLoadState=loaded\n", &s(&["a"]));
        assert_eq!(m["a"].active, "unknown");
        assert_eq!(m["a"].load, "loaded");
    }

    #[test]
    fn parse_tolerates_crlf_and_extra_blank_lines() {
        let out = "\r\nId=a.service\r\nActiveState=active\r\nSubState=running\r\nLoadState=loaded\r\n\r\n\r\n";
        let m = parse_show_output(out, &s(&["a"]));
        assert_eq!(m["a"], st("active", "running", "loaded"));
    }

    #[test]
    fn canonical_ids() {
        assert_eq!(canonical_unit_id("docker-foo"), "docker-foo.service");
        assert_eq!(canonical_unit_id("neo-foo.target"), "neo-foo.target");
        assert_eq!(canonical_unit_id("app.v2"), "app.v2.service");
    }

    fn request() -> ServiceStatusRequest {
        serde_json::from_str(
            r#"{"services":{
                "immich":{"units":["docker-immich","docker-immich-db","docker-immich"]},
                "pihole":{"units":["docker-pihole","pihole-update-gravity"],"timers":["pihole-update-gravity"]},
                "bad name":{"units":["x"]},
                "evil":{"units":["-H","ok; rm","docker-evil"]},
                "empty":{}
            }}"#,
        )
        .unwrap()
    }

    #[test]
    fn all_units_is_deduped_valid_union() {
        let units = request().all_units();
        assert_eq!(
            units,
            s(&[
                "docker-evil",
                "docker-immich",
                "docker-immich-db",
                "docker-pihole",
                "pihole-update-gravity"
            ])
        );
    }

    #[test]
    fn map_services_groups_states_and_flags_timers() {
        let req = request();
        let units = req.all_units();
        let out = "Id=docker-evil.service\nLoadState=loaded\nActiveState=active\nSubState=running\n\n\
                   Id=docker-immich.service\nLoadState=loaded\nActiveState=active\nSubState=running\n\n\
                   Id=docker-immich-db.service\nLoadState=loaded\nActiveState=failed\nSubState=failed\n\n\
                   Id=docker-pihole.service\nLoadState=loaded\nActiveState=activating\nSubState=start\n\n\
                   Id=pihole-update-gravity.service\nLoadState=loaded\nActiveState=inactive\nSubState=dead\n";
        let states = parse_show_output(out, &units);
        let resp = map_services(&req, &states);

        assert!(!resp.services.contains_key("bad name"));
        assert!(resp.services["empty"].units.is_empty());

        let immich = &resp.services["immich"].units;
        assert_eq!(immich.len(), 2, "duplicate unit collapsed");
        assert_eq!(immich[0].name, "docker-immich");
        assert_eq!(immich[0].active, "active");
        assert_eq!(immich[1].active, "failed");
        assert_eq!(immich[0].health, UnitHealth::Running);
        assert_eq!(immich[1].health, UnitHealth::Failed);
        assert!(!immich[0].timer);

        let pihole = &resp.services["pihole"].units;
        assert_eq!(pihole[0].active, "activating");
        assert!(!pihole[0].timer);
        assert_eq!(pihole[1].name, "pihole-update-gravity");
        assert!(pihole[1].timer);

        let evil = &resp.services["evil"].units;
        assert_eq!(evil.len(), 1);
        assert_eq!(evil[0].name, "docker-evil");
    }

    #[test]
    fn map_services_missing_state_is_unknown() {
        let resp = map_services(&request(), &HashMap::new());
        assert!(resp.services["immich"]
            .units
            .iter()
            .all(|u| u.active == "unknown"));
    }

    fn fake_fetch(
        calls: Arc<AtomicUsize>,
        seen: Arc<std::sync::Mutex<Vec<Vec<String>>>>,
    ) -> impl FnOnce(
        Vec<String>,
    )
        -> std::pin::Pin<Box<dyn Future<Output = HashMap<String, UnitStatus>> + Send>> {
        move |units: Vec<String>| {
            calls.fetch_add(1, Ordering::SeqCst);
            seen.lock().unwrap().push(units.clone());
            Box::pin(async move {
                tokio::time::sleep(Duration::from_millis(30)).await;
                units
                    .into_iter()
                    .map(|u| (u, st("active", "running", "loaded")))
                    .collect()
            })
        }
    }

    #[tokio::test]
    async fn cache_reuses_fresh_entries_and_fetches_only_missing() {
        let cache = UnitStatusCache::new(Duration::from_secs(60));
        let calls = Arc::new(AtomicUsize::new(0));
        let seen = Arc::new(std::sync::Mutex::new(Vec::new()));

        let m = cache
            .get_with(&s(&["a", "b"]), fake_fetch(calls.clone(), seen.clone()))
            .await;
        assert_eq!(m.len(), 2);
        cache
            .get_with(&s(&["a", "b"]), fake_fetch(calls.clone(), seen.clone()))
            .await;
        assert_eq!(
            calls.load(Ordering::SeqCst),
            1,
            "fresh hit: no second query"
        );

        cache
            .get_with(&s(&["a", "c"]), fake_fetch(calls.clone(), seen.clone()))
            .await;
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        assert_eq!(seen.lock().unwrap()[1], s(&["c"]), "only the missing unit");
    }

    #[tokio::test]
    async fn cache_expires_after_ttl() {
        let cache = UnitStatusCache::new(Duration::from_millis(0));
        let calls = Arc::new(AtomicUsize::new(0));
        let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
        for _ in 0..3 {
            cache
                .get_with(&s(&["a"]), fake_fetch(calls.clone(), seen.clone()))
                .await;
        }
        assert_eq!(calls.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn cache_is_single_flight_under_concurrency() {
        let cache = Arc::new(UnitStatusCache::new(Duration::from_secs(60)));
        let calls = Arc::new(AtomicUsize::new(0));
        let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
        let mut handles = Vec::new();
        for _ in 0..16 {
            let cache = cache.clone();
            let f = fake_fetch(calls.clone(), seen.clone());
            handles.push(tokio::spawn(async move {
                cache.get_with(&s(&["a", "b", "c"]), f).await
            }));
        }
        for h in handles {
            assert_eq!(h.await.unwrap().len(), 3);
        }
        assert_eq!(calls.load(Ordering::SeqCst), 1, "one batched query total");
    }

    /// Real systemctl when present (states vary by host); without it every unit is
    /// `unknown`. Either way: one entry per requested unit, no error.
    #[tokio::test]
    async fn query_unit_states_always_answers_every_unit() {
        let units = s(&["neo-status-test-nonexistent", "dbus", "dbus"]);
        let m = query_unit_states(&units).await;
        assert_eq!(m.len(), 2);
        let missing = &m["neo-status-test-nonexistent"];
        assert!(
            missing.active == "unknown" || missing.load == "not-found",
            "{missing:?}"
        );
    }

    #[tokio::test]
    async fn cache_ignores_invalid_units_and_empty_requests() {
        let cache = UnitStatusCache::new(Duration::from_secs(60));
        let calls = Arc::new(AtomicUsize::new(0));
        let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
        let m = cache
            .get_with(
                &s(&["", "-x", "a b"]),
                fake_fetch(calls.clone(), seen.clone()),
            )
            .await;
        assert!(m.is_empty());
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }
}
