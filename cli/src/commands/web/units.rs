use std::path::{Component, Path};
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::io::{AsyncReadExt, BufReader as AsyncBufReader};
use tokio::process::Command as AsyncCommand;

use super::structs::AppConfig;
use super::util::{escape_attr, escape_html, sudo_cmd};

pub use super::util::unit_name_valid;

/// systemctl action allowed from the web UI.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnitAction {
    Start,
    Stop,
    Restart,
}

impl UnitAction {
    pub fn as_str(self) -> &'static str {
        match self {
            UnitAction::Start => "start",
            UnitAction::Stop => "stop",
            UnitAction::Restart => "restart",
        }
    }
}

pub fn is_pull_in_flight(config: &AppConfig, unit: &str) -> bool {
    config
        .pulls_in_flight
        .lock()
        .map(|s| s.contains(unit))
        .unwrap_or(false)
}

/// Mark unit as pulling. Returns false if a pull is already in flight for this unit.
pub fn try_begin_pull(config: &AppConfig, unit: &str) -> bool {
    match config.pulls_in_flight.lock() {
        Ok(mut s) => {
            if s.contains(unit) {
                false
            } else {
                s.insert(unit.to_string());
                true
            }
        }
        Err(_) => false,
    }
}

pub fn end_pull(config: &AppConfig, unit: &str) {
    if let Ok(mut s) = config.pulls_in_flight.lock() {
        s.remove(unit);
    }
}

/// Normalize systemctl is-active stdout into a state string.
fn parse_active_state_stdout(stdout: &[u8]) -> String {
    let s = String::from_utf8_lossy(stdout).trim().to_string();
    if s.is_empty() {
        "unknown".into()
    } else {
        s
    }
}

/// Query systemctl is-active for a unit (sync; used when building OOB control fragments).
pub fn unit_active_state(unit: &str) -> String {
    let sudo = sudo_cmd();
    Command::new(&sudo)
        .args(["systemctl", "is-active", unit])
        .output()
        .map(|o| parse_active_state_stdout(&o.stdout))
        .unwrap_or_else(|_| "unknown".into())
}

pub async fn unit_active_state_async(unit: &str) -> String {
    let sudo = sudo_cmd();
    match AsyncCommand::new(&sudo)
        .args(["systemctl", "is-active", unit])
        .output()
        .await
    {
        Ok(o) => parse_active_state_stdout(&o.stdout),
        Err(_) => "unknown".into(),
    }
}

const UPDATE_OUT_CLASSES: &str =
    "update-out update-out-inline text-[10px] ml-1 flex-shrink-0 max-w-[16rem] truncate";

/// OOB fragment for the per-row pull status slot (`#update-out-{unit}`).
pub fn update_out_oob(unit: &str, inner: &str, title: &str) -> String {
    format!(
        r#"<div id="update-out-{}" class="{}" title="{}" hx-swap-oob="true">{}</div>"#,
        escape_html(unit),
        UPDATE_OUT_CLASSES,
        escape_attr(title),
        inner
    )
}

fn status_pulling(msg: &str) -> (String, String) {
    let title = msg.to_string();
    let inner = format!(
        r#"<span class="inline-flex items-center gap-1 text-info max-w-full"><span class="loading loading-spinner loading-xs flex-shrink-0"></span><span class="truncate">{}</span></span>"#,
        escape_html(msg)
    );
    (inner, title)
}

fn status_ok(msg: &str) -> (String, String) {
    let title = msg.to_string();
    let inner = format!(
        r#"<span class="text-success truncate">✓ {}</span>"#,
        escape_html(msg)
    );
    (inner, title)
}

fn status_err(msg: &str) -> (String, String) {
    let title = msg.to_string();
    let inner = format!(
        r#"<span class="text-error truncate">✗ {}</span>"#,
        escape_html(msg)
    );
    (inner, title)
}

pub fn broadcast_update_out(unit: &str, inner: &str, title: &str, config: &AppConfig) {
    let _ = config.unit_updates.send(update_out_oob(unit, inner, title));
}

/// Push an error status for a container pull, clear in-flight, and refresh controls.
fn fail_pull(unit: &str, msg: &str, config: &AppConfig) {
    let (inner, title) = status_err(msg);
    broadcast_update_out(unit, &inner, &title, config);
    end_pull(config, unit);
    broadcast_unit_update(unit, config);
}

/// Build the inner content (dot + state + buttons) for a unit controls area.
/// Used for OOB WS pushes and composed into full divs.
///
/// Buttons stay stable across transitional states so restart/stop never "vanish"
/// while systemctl --no-block is still settling (the live WS watcher re-renders
/// as soon as ActiveState changes).
pub fn render_unit_controls_content_with_state(unit: &str, active: &str, pulling: bool) -> String {
    let is_container = unit.starts_with("docker-");

    let dot_cls = match active {
        "active" => "bg-success",
        "inactive" => "bg-base-300",
        "activating" | "deactivating" | "reloading" => "bg-info animate-pulse",
        "failed" => "bg-error",
        _ => "bg-warning",
    };

    let u = escape_html(unit);
    let state_label = escape_html(active);
    // Basic JS string escape for onclick arg (single quotes in unit names are rare for units)
    let u_js = u.replace('\'', "\\'");

    let mut inner = String::new();
    inner.push_str(&format!(
        r#"<span class="inline-block w-2 h-2 rounded-full flex-shrink-0 {}" title="{}"></span>"#,
        dot_cls,
        escape_attr(unit)
    ));
    inner.push_str(&format!(
        r#"<span class="text-[10px] opacity-60 font-mono min-w-[4.5rem]" title="ActiveState">{}</span>"#,
        state_label
    ));

    let start_btn = format!(
        r##"<button class="btn btn-ghost btn-xs h-5 min-h-0 px-1.5" hx-post="/unit/start/{u}" hx-swap="none" title="systemctl start">▶</button>"##,
        u = u
    );
    let restart_btn = format!(
        r##"<button class="btn btn-ghost btn-xs h-5 min-h-0 px-1.5" hx-post="/unit/restart/{u}" hx-swap="none" title="systemctl restart">⟳</button>"##,
        u = u
    );
    let stop_btn = format!(
        r##"<button class="btn btn-ghost btn-xs h-5 min-h-0 px-1.5" hx-post="/unit/stop/{u}" hx-swap="none" title="systemctl stop">⏹</button>"##,
        u = u
    );

    // Stable control set: inactive/failed → start; anything running/transitional → stop+restart.
    // failed also keeps restart so a retry is one click.
    match active {
        "inactive" => {
            inner.push_str(&start_btn);
        }
        "failed" => {
            inner.push_str(&start_btn);
            inner.push_str(&restart_btn);
        }
        _ => {
            // active | activating | deactivating | reloading | unknown
            inner.push_str(&stop_btn);
            inner.push_str(&restart_btn);
        }
    }

    // logs always opens dialog (live via SSE)
    inner.push_str(&format!(
        r#"<button class="btn btn-ghost btn-xs h-5 min-h-0 px-1.5" onclick="openUnitLogs('{}')" title="open live logs dialog (infinitely scrollable)">logs</button>"#,
        u_js
    ));

    if is_container {
        if pulling {
            inner.push_str(
                r#"<button class="btn btn-accent btn-xs h-5 min-h-0 px-1.5 btn-disabled" disabled title="docker pull in progress"><span class="loading loading-spinner loading-xs"></span></button>"#,
            );
        } else {
            // hx-swap=none: immediate OOB (update-out + controls) comes from the response;
            // long pull progress is pushed over /ws/status.
            inner.push_str(&format!(
                r##"<button class="btn btn-accent btn-xs h-5 min-h-0 px-1.5" hx-post="/container/update/{u}" hx-swap="none" hx-disabled-elt="this" title="docker pull (current running image) + restart">↻</button>"##,
                u = u
            ));
        }
    }

    inner
}

/// OOB fragment for htmx ws (and action HTTP responses).
pub fn unit_controls_oob_fragment(unit: &str, config: &AppConfig) -> String {
    let active = unit_active_state(unit);
    let pulling = is_pull_in_flight(config, unit);
    unit_controls_oob_fragment_with_state(unit, &active, pulling)
}

pub fn unit_controls_oob_fragment_with_state(unit: &str, active: &str, pulling: bool) -> String {
    format!(
        r#"<div id="unit-controls-{}" class="unit-controls flex items-center gap-1 flex-shrink-0" data-active-state="{}" hx-swap-oob="true">{}</div>"#,
        escape_html(unit),
        escape_attr(active),
        render_unit_controls_content_with_state(unit, active, pulling)
    )
}

/// Broadcast an OOB swap fragment for a unit's controls to all connected WS clients.
pub fn broadcast_unit_update(unit: &str, config: &AppConfig) {
    let _ = config
        .unit_updates
        .send(unit_controls_oob_fragment(unit, config));
}

/// After a non-blocking systemctl action, ActiveState may lag for a few seconds.
/// Push a short burst of refreshes so the UI settles without waiting for the next
/// watcher tick alone (and even if the pane only did a one-shot HTTP OOB).
pub fn schedule_unit_refresh_burst(unit: String, config: Arc<AppConfig>) {
    if !unit_name_valid(&unit) {
        return;
    }
    tokio::spawn(async move {
        for delay_ms in [150_u64, 400, 900, 1800, 3500] {
            tokio::time::sleep(Duration::from_millis(delay_ms)).await;
            broadcast_unit_update(&unit, &config);
        }
    });
}

pub fn perform_unit_action(action: UnitAction, unit: &str) {
    if !unit_name_valid(unit) {
        return;
    }
    let sudo = sudo_cmd();
    let _ = Command::new(&sudo)
        .args([
            "systemctl",
            action.as_str(),
            unit,
            "--no-block",
            "--no-ask-password",
        ])
        .status();
}

/// Best-effort parse of `id="unit-controls-…"` + `data-active-state="…"` from an OOB fragment.
pub fn extract_unit_state_from_oob(fragment: &str) -> Option<(String, String)> {
    let id_marker = r#"id="unit-controls-"#;
    let state_marker = r#"data-active-state=""#;
    let id_start = fragment.find(id_marker)? + id_marker.len();
    let id_end = fragment[id_start..].find('"')? + id_start;
    let unit = fragment[id_start..id_end].to_string();
    let state_start = fragment.find(state_marker)? + state_marker.len();
    let state_end = fragment[state_start..].find('"')? + state_start;
    let state = fragment[state_start..state_end].to_string();
    if unit_name_valid(&unit) {
        Some((unit, state))
    } else {
        None
    }
}

/// Normalize path param to (systemd unit name, bare docker container name).
pub fn normalize_container_unit(container: &str) -> (String, String) {
    if container.starts_with("docker-") {
        let bare = container[7..].to_string();
        (container.to_string(), bare)
    } else {
        (format!("docker-{}", container), container.to_string())
    }
}

fn last_progress_line(buf: &str) -> Option<&str> {
    buf.split(|c| c == '\r' || c == '\n')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .last()
}

fn spawn_pull_pipe_reader(
    pipe: impl tokio::io::AsyncRead + Unpin + Send + 'static,
    tx: tokio::sync::mpsc::Sender<String>,
) {
    tokio::spawn(async move {
        let mut reader = AsyncBufReader::new(pipe);
        let mut buf = [0u8; 512];
        let mut acc = String::new();
        loop {
            match reader.read(&mut buf).await {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    acc.push_str(&String::from_utf8_lossy(&buf[..n]));
                    if acc.len() > 4096 {
                        acc = acc[acc.len() - 2048..].to_string();
                    }
                    if let Some(line) = last_progress_line(&acc) {
                        let _ = tx.try_send(line.to_string());
                    }
                }
            }
        }
    });
}

/// Background: docker inspect → pull (stream progress over WS) → systemctl restart.
pub async fn run_container_pull(unit: String, cname: String, config: Arc<AppConfig>) {
    let push = |inner: String, title: String| {
        broadcast_update_out(&unit, &inner, &title, &config);
    };

    let inspect = AsyncCommand::new("docker")
        .args(["inspect", "--format", "{{.Config.Image}}", &cname])
        .output()
        .await;

    let img = match inspect {
        Ok(o) if o.status.success() => {
            let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
            if s.is_empty() {
                fail_pull(&unit, "no image from inspect", &config);
                return;
            }
            s
        }
        Ok(o) => {
            let err = String::from_utf8_lossy(&o.stderr);
            fail_pull(&unit, &format!("inspect failed: {}", err.trim()), &config);
            return;
        }
        Err(e) => {
            fail_pull(&unit, &format!("docker error: {}", e), &config);
            return;
        }
    };

    {
        let (inner, title) = status_pulling(&format!("pulling {}", img));
        push(inner, title);
    }

    let mut child = match AsyncCommand::new("docker")
        .args(["pull", &img])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            fail_pull(&unit, &format!("pull spawn: {}", e), &config);
            return;
        }
    };

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let (line_tx, mut line_rx) = tokio::sync::mpsc::channel::<String>(16);

    if let Some(pipe) = stdout {
        spawn_pull_pipe_reader(pipe, line_tx.clone());
    }
    if let Some(pipe) = stderr {
        spawn_pull_pipe_reader(pipe, line_tx);
    }

    let unit_p = unit.clone();
    let config_p = Arc::clone(&config);
    let img_p = img.clone();
    let progress_task = tokio::spawn(async move {
        let started = Instant::now();
        let mut last_emit = Instant::now() - Duration::from_secs(1);
        let mut last_line = format!("pulling {}", img_p);
        let mut ticker = tokio::time::interval(Duration::from_millis(500));
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                msg = line_rx.recv() => {
                    match msg {
                        Some(line) => {
                            last_line = if line.len() > 80 {
                                format!("{}…", &line[..77])
                            } else {
                                line
                            };
                            if last_emit.elapsed() >= Duration::from_millis(350) {
                                let (inner, title) = status_pulling(&last_line);
                                broadcast_update_out(&unit_p, &inner, &title, &config_p);
                                last_emit = Instant::now();
                            }
                        }
                        None => break,
                    }
                }
                _ = ticker.tick() => {
                    // Heartbeat so a quiet pull still shows the UI is alive.
                    if last_emit.elapsed() >= Duration::from_secs(2) {
                        let secs = started.elapsed().as_secs();
                        let msg = format!("pulling {} ({}s)", img_p, secs);
                        let (inner, title) = status_pulling(&msg);
                        broadcast_update_out(&unit_p, &inner, &title, &config_p);
                        last_emit = Instant::now();
                    }
                }
            }
        }
    });

    let status = child.wait().await;
    // Drop senders finished with pipes; wait for progress UI task.
    let _ = progress_task.await;

    match status {
        Ok(s) if s.success() => {
            let (inner, title) = status_pulling("restarting…");
            push(inner, title);

            let sudo = sudo_cmd();
            let _ = AsyncCommand::new(&sudo)
                .args([
                    "systemctl",
                    "restart",
                    &unit,
                    "--no-block",
                    "--no-ask-password",
                ])
                .status()
                .await;

            let (inner, title) = status_ok(&format!("updated {}", img));
            push(inner, title);
            end_pull(&config, &unit);
            broadcast_unit_update(&unit, &config);
            schedule_unit_refresh_burst(unit, config);
        }
        Ok(s) => {
            fail_pull(
                &unit,
                &format!("pull exit {}", s.code().unwrap_or(-1)),
                &config,
            );
        }
        Err(e) => {
            fail_pull(&unit, &format!("pull wait: {}", e), &config);
        }
    }
}

// --- Clear appdata (stop units → rm -rf → start units) ---

pub fn is_clear_appdata_in_flight(config: &AppConfig, service: &str) -> bool {
    config
        .clear_appdata_in_flight
        .lock()
        .map(|s| s.contains(service))
        .unwrap_or(false)
}

/// Mark service as clearing appdata. Returns false if already in flight.
pub fn try_begin_clear_appdata(config: &AppConfig, service: &str) -> bool {
    match config.clear_appdata_in_flight.lock() {
        Ok(mut s) => {
            if s.contains(service) {
                false
            } else {
                s.insert(service.to_string());
                true
            }
        }
        Err(_) => false,
    }
}

pub fn end_clear_appdata(config: &AppConfig, service: &str) {
    if let Ok(mut s) = config.clear_appdata_in_flight.lock() {
        s.remove(service);
    }
}

const CLEAR_APPDATA_OUT_CLASSES: &str =
    "clear-appdata-out text-[10px] ml-1 flex-shrink-0 max-w-[18rem] truncate";

/// OOB fragment for the per-service clear-appdata status slot.
pub fn clear_appdata_out_oob(service: &str, inner: &str, title: &str) -> String {
    format!(
        r#"<div id="clear-appdata-out-{}" class="{}" title="{}" hx-swap-oob="true">{}</div>"#,
        escape_html(service),
        CLEAR_APPDATA_OUT_CLASSES,
        escape_attr(title),
        inner
    )
}

/// OOB fragment for the Clear appdata button (disabled while in flight).
pub fn clear_appdata_btn_oob(service: &str, appdata: &str, busy: bool) -> String {
    let svc = escape_html(service);
    let path = escape_attr(appdata);
    let confirm = escape_attr(&format!(
        "Stop all related units, permanently delete {} and all contents, then start the units again? This cannot be undone.",
        appdata
    ));
    if busy {
        format!(
            r#"<button id="clear-appdata-btn-{svc}" class="btn btn-error btn-xs btn-disabled" disabled title="{path}" hx-swap-oob="true"><span class="loading loading-spinner loading-xs"></span> Clearing…</button>"#,
            svc = svc,
            path = path,
        )
    } else {
        format!(
            r##"<button id="clear-appdata-btn-{svc}" class="btn btn-error btn-xs" title="Delete {path}" hx-post="/service/{svc}/clear-appdata" hx-swap="none" hx-confirm="{confirm}" hx-disabled-elt="this" hx-swap-oob="true">Clear appdata</button>"##,
            svc = svc,
            path = path,
            confirm = confirm,
        )
    }
}

fn clear_status_pulling(msg: &str) -> (String, String) {
    let title = msg.to_string();
    let inner = format!(
        r#"<span class="inline-flex items-center gap-1 text-info max-w-full"><span class="loading loading-spinner loading-xs flex-shrink-0"></span><span class="truncate">{}</span></span>"#,
        escape_html(msg)
    );
    (inner, title)
}

fn clear_status_ok(msg: &str) -> (String, String) {
    let title = msg.to_string();
    let inner = format!(
        r#"<span class="text-success truncate">✓ {}</span>"#,
        escape_html(msg)
    );
    (inner, title)
}

fn clear_status_err(msg: &str) -> (String, String) {
    let title = msg.to_string();
    let inner = format!(
        r#"<span class="text-error truncate">✗ {}</span>"#,
        escape_html(msg)
    );
    (inner, title)
}

/// Whether `path` is safe to recursively delete as service appdata.
/// Path must come from trusted nix evaluation; this is defense-in-depth.
pub fn is_safe_appdata_path(path: &str, appdata_root: Option<&str>) -> bool {
    if path.is_empty() || path.contains('\0') || !path.starts_with('/') {
        return false;
    }
    let p = Path::new(path);
    if p.components()
        .any(|c| matches!(c, Component::ParentDir | Component::CurDir))
    {
        return false;
    }
    // At least three normal components: /var/lib/openclaw or /var/neo/DATA/AppData/foo
    let depth = p
        .components()
        .filter(|c| matches!(c, Component::Normal(_)))
        .count();
    if depth < 3 {
        return false;
    }
    const FORBIDDEN: &[&str] = &[
        "/", "/bin", "/boot", "/dev", "/etc", "/home", "/lib", "/nix", "/proc", "/root", "/run",
        "/sys", "/tmp", "/usr", "/var", "/var/lib", "/var/log", "/var/neo",
    ];
    if FORBIDDEN.contains(&path) {
        return false;
    }
    if let Some(root) = appdata_root {
        if path == root {
            // Never wipe the entire AppData volume.
            return false;
        }
        // Preferred: strict child of the AppData volume.
        let prefix = if root.ends_with('/') {
            root.to_string()
        } else {
            format!("{}/", root)
        };
        if path.starts_with(&prefix) {
            return true;
        }
    }
    // Paths outside the volume (e.g. openclaw /var/lib/openclaw) still allowed when
    // declared by the service option and deep enough (checked above).
    true
}

fn unit_is_stopped(state: &str) -> bool {
    matches!(state, "inactive" | "failed" | "dead" | "not-found")
}

async fn wait_units_stopped(units: &[String], timeout: Duration) -> Result<(), String> {
    if units.is_empty() {
        return Ok(());
    }
    let deadline = Instant::now() + timeout;
    loop {
        let mut pending = Vec::new();
        for u in units {
            let state = unit_active_state_async(u).await;
            if !unit_is_stopped(&state) {
                pending.push(format!("{}={}", u, state));
            }
        }
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

async fn systemctl_action_blocking(action: &str, unit: &str) -> Result<(), String> {
    let sudo = sudo_cmd();
    let out = AsyncCommand::new(&sudo)
        .args(["systemctl", action, unit, "--no-ask-password"])
        .output()
        .await
        .map_err(|e| format!("systemctl {} {}: {}", action, unit, e))?;
    if out.status.success() {
        Ok(())
    } else {
        let err = String::from_utf8_lossy(&out.stderr);
        // stop of an already-inactive unit is fine
        if action == "stop" && err.contains("not loaded") {
            return Ok(());
        }
        Err(format!(
            "systemctl {} {} failed: {}",
            action,
            unit,
            err.trim()
        ))
    }
}

async fn rm_rf_path(path: &str) -> Result<(), String> {
    let p = Path::new(path);
    if !p.exists() {
        return Ok(());
    }
    let sudo = sudo_cmd();
    let out = AsyncCommand::new(&sudo)
        .args(["rm", "-rf", "--", path])
        .output()
        .await
        .map_err(|e| format!("rm -rf: {}", e))?;
    if out.status.success() {
        Ok(())
    } else {
        let err = String::from_utf8_lossy(&out.stderr);
        Err(format!("rm -rf failed: {}", err.trim()))
    }
}

fn finish_clear_appdata(
    service: &str,
    appdata: &str,
    units: &[String],
    config: &Arc<AppConfig>,
    inner: String,
    title: String,
) {
    let frag = clear_appdata_out_oob(service, &inner, &title);
    let _ = config.unit_updates.send(frag);
    let btn = clear_appdata_btn_oob(service, appdata, false);
    let _ = config.unit_updates.send(btn);
    end_clear_appdata(config, service);
    for u in units {
        broadcast_unit_update(u, config);
        schedule_unit_refresh_burst(u.clone(), Arc::clone(config));
    }
}

/// Background: stop all service units, wait until stopped, rm -rf appdata, start units again.
pub async fn run_clear_appdata(
    service: String,
    appdata: String,
    units: Vec<String>,
    config: Arc<AppConfig>,
) {
    let units: Vec<String> = units.into_iter().filter(|u| unit_name_valid(u)).collect();

    let push = |inner: String, title: String| {
        let frag = clear_appdata_out_oob(&service, &inner, &title);
        let _ = config.unit_updates.send(frag);
    };

    {
        let (inner, title) = clear_status_pulling("stopping units…");
        push(inner, title);
    }

    for u in &units {
        if let Err(e) = systemctl_action_blocking("stop", u).await {
            // Continue stopping others; wait_units_stopped surfaces stuck units.
            eprintln!("web: clear-appdata stop {}: {}", u, e);
        }
        broadcast_unit_update(u, &config);
    }

    if let Err(e) = wait_units_stopped(&units, Duration::from_secs(90)).await {
        let (inner, title) = clear_status_err(&e);
        finish_clear_appdata(&service, &appdata, &units, &config, inner, title);
        return;
    }

    {
        let (inner, title) = clear_status_pulling("removing appdata…");
        push(inner, title);
    }

    if let Err(e) = rm_rf_path(&appdata).await {
        let (inner, title) = clear_status_err(&e);
        // Best-effort restart so services are not left down after a failed delete.
        for u in &units {
            let _ = systemctl_action_blocking("start", u).await;
        }
        finish_clear_appdata(&service, &appdata, &units, &config, inner, title);
        return;
    }

    {
        let (inner, title) = clear_status_pulling("starting units…");
        push(inner, title);
    }

    for u in &units {
        if let Err(e) = systemctl_action_blocking("start", u).await {
            eprintln!("web: clear-appdata start {}: {}", u, e);
        }
        broadcast_unit_update(u, &config);
    }

    let (inner, title) = clear_status_ok("appdata cleared");
    finish_clear_appdata(&service, &appdata, &units, &config, inner, title);
}

#[cfg(test)]
mod tests {
    use super::is_safe_appdata_path;

    #[test]
    fn appdata_path_under_volume() {
        let root = "/var/neo/DATA/AppData";
        assert!(is_safe_appdata_path(
            "/var/neo/DATA/AppData/vaultwarden",
            Some(root)
        ));
        assert!(!is_safe_appdata_path(root, Some(root)));
        assert!(!is_safe_appdata_path(
            "/var/neo/DATA/AppData/../etc",
            Some(root)
        ));
        assert!(!is_safe_appdata_path("/", Some(root)));
        assert!(!is_safe_appdata_path("/var/lib", Some(root)));
    }

    #[test]
    fn appdata_path_outside_volume_deep() {
        assert!(is_safe_appdata_path("/var/lib/openclaw", None));
        assert!(!is_safe_appdata_path("/var/lib", None));
    }
}
