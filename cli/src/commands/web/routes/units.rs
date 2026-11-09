use std::sync::Arc;

use rocket::response::content::RawHtml;
use rocket::response::stream::{Event, EventStream};
use rocket::serde::json::Json;
use rocket::{get, post, State};
use tokio::io::{AsyncBufReadExt, BufReader as AsyncBufReader};

use crate::commands::web::locks::{try_lock, Blocked};
use crate::commands::web::types::AppConfig;
use crate::commands::web::units::{
    begin_clear_appdata, begin_pull, clear_appdata_btn_oob, clear_appdata_out_oob, map_services,
    normalize_container_unit, pane_lock_spec, perform_unit_action, run_clear_appdata,
    run_container_pull, schedule_unit_refresh_burst, trusted_appdata, unit_controls_oob_fragment,
    unit_name_valid, update_out_oob, ClearJob, ServiceStatusRequest, ServiceStatusResponse,
    UnitAction,
};
use crate::commands::web::util::{escape_html, proc, service_name_ok, status_err, status_pulling};
use crate::utils::locks::{LockSpec, OpInfo};

/// Shared post-action path: kick systemctl, push OOB once, then burst-refresh while it settles.
/// Buttons use hx-swap="none"; the returned OOB still updates the controls row.
/// Refused (409) while the unit is locked, e.g. its service's data is being restored.
/// A failed systemctl shows in the row's status slot.
async fn unit_action_response(
    action: UnitAction,
    unit: &str,
    config: &State<Arc<AppConfig>>,
) -> Result<RawHtml<String>, Blocked> {
    if !unit_name_valid(unit) {
        return Ok(RawHtml(String::new()));
    }
    let lock = try_lock(
        &LockSpec::unit(unit),
        OpInfo::new("unit", format!("systemctl {} {unit}", action.as_str())),
    )?;
    let result = perform_unit_action(action, unit).await;
    drop(lock);
    let out = match &result {
        Ok(()) => update_out_oob(unit, "", ""),
        Err(e) => {
            let (inner, title) = status_err(e);
            update_out_oob(unit, &inner, &title)
        }
    };
    let oob = unit_controls_oob_fragment(unit, config).await;
    let _ = config.unit_updates.send(out.clone());
    let _ = config.unit_updates.send(oob.clone());
    schedule_unit_refresh_burst(unit.to_string(), Arc::clone(config));
    Ok(RawHtml(format!("{out}{oob}")))
}

#[post("/unit/restart/<unit>")]
pub async fn unit_restart(
    unit: &str,
    config: &State<Arc<AppConfig>>,
) -> Result<RawHtml<String>, Blocked> {
    unit_action_response(UnitAction::Restart, unit, config).await
}

#[post("/unit/start/<unit>")]
pub async fn unit_start(
    unit: &str,
    config: &State<Arc<AppConfig>>,
) -> Result<RawHtml<String>, Blocked> {
    unit_action_response(UnitAction::Start, unit, config).await
}

#[post("/unit/stop/<unit>")]
pub async fn unit_stop(
    unit: &str,
    config: &State<Arc<AppConfig>>,
) -> Result<RawHtml<String>, Blocked> {
    unit_action_response(UnitAction::Stop, unit, config).await
}

/// Kick off an async docker pull+restart. Returns immediately with OOB status +
/// disabled ↻ button; progress and completion are pushed over `/ws/status`.
#[post("/container/update/<container>")]
pub async fn container_update(
    container: &str,
    config: &State<Arc<AppConfig>>,
) -> Result<RawHtml<String>, Blocked> {
    if !unit_name_valid(container) {
        return Ok(RawHtml(String::new()));
    }
    let (unit, cname) = normalize_container_unit(container);

    // Held for the whole pull + restart (moved into the job).
    let lock = try_lock(
        &LockSpec::unit(&unit),
        OpInfo::new("pull", format!("Image update of {unit}")),
    )?;
    begin_pull(config, &unit);

    let (inner, _) = status_pulling("starting pull…");
    let out = update_out_oob(&unit, &inner, "starting docker pull");
    let ctl = unit_controls_oob_fragment(&unit, config).await;
    let _ = config.unit_updates.send(out.clone());
    let _ = config.unit_updates.send(ctl.clone());

    let cfg = Arc::clone(config);
    tokio::spawn(async move {
        let _lock = lock;
        run_container_pull(unit, cname, cfg).await;
    });

    Ok(RawHtml(format!("{out}{ctl}")))
}

/// Clear a service's appdata: stop its units and target, rm -rf the declared path,
/// then start only what was running beforehand.
/// Path and units come from trusted flake evaluation (never from the client).
#[post("/service/<service>/clear-appdata")]
pub async fn clear_appdata(
    service: &str,
    config: &State<Arc<AppConfig>>,
) -> Result<RawHtml<String>, Blocked> {
    if !service_name_ok(service) {
        return Ok(RawHtml(String::new()));
    }
    let (pane, appdata) = match trusted_appdata(config, service).await {
        Ok(v) => v,
        Err(e) => {
            let (inner, title) = status_err(&e);
            return Ok(RawHtml(clear_appdata_out_oob(service, &inner, &title)));
        }
    };

    // Held until the job ends.
    let lock = try_lock(
        &pane_lock_spec(service, &pane),
        OpInfo::new("clear-appdata", format!("Clearing app data of {service}")),
    )?;
    begin_clear_appdata(config, service);

    let (inner, _) = status_pulling("starting…");
    let out = clear_appdata_out_oob(service, &inner, "starting clear appdata");
    let btn = clear_appdata_btn_oob(service, &appdata, true);
    let _ = config.unit_updates.send(out.clone());
    let _ = config.unit_updates.send(btn.clone());

    let job = ClearJob {
        service: service.to_string(),
        appdata,
        units: pane.units.iter().map(|u| u.name.clone()).collect(),
        group_unit: pane.group_unit.clone(),
    };
    tokio::spawn(run_clear_appdata(job, lock, Arc::clone(config)));

    Ok(RawHtml(format!("{out}{btn}")))
}

/// SSE endpoint for live journalctl follow in the logs dialog.
/// Client uses native EventSource; first ~100 lines + subsequent live appends.
#[get("/sse/logs/<unit>")]
pub async fn sse_logs(unit: &str) -> EventStream![] {
    let unit = unit.to_string();
    EventStream! {
        if !unit_name_valid(&unit) {
            yield Event::data("invalid unit name for logs");
        } else {
            let spawn_res = proc::sudo()
                .args([
                    "journalctl",
                    "-u",
                    &unit,
                    "-n",
                    "100",
                    "-f",
                    "--no-pager",
                    "-o",
                    "short-iso",
                ])
                .kill_on_drop(true)
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn();
            let child_opt = match spawn_res {
                Ok(c) => Some(c),
                Err(e) => {
                    yield Event::data(format!("spawn error: {}", e));
                    None
                }
            };
            if let Some(mut child) = child_opt {
                match child.stdout.take() {
                    Some(stdout) => {
                        let mut lines = AsyncBufReader::new(stdout).lines();

                        loop {
                            match lines.next_line().await {
                                Ok(Some(line)) => {
                                    yield Event::data(escape_html(&line));
                                }
                                Ok(None) => break,
                                Err(e) => {
                                    yield Event::data(format!("[read err] {}", e));
                                    break;
                                }
                            }
                        }
                    }
                    None => {
                        yield Event::data("spawn error: missing piped stdout");
                    }
                }
                // child auto-killed by kill_on_drop on drop
            }
        }
    }
}

/// Status dots for every service card on the page in one round-trip.
///
/// The page posts `{services: {name: {units, timers}}}`; all units go into ONE
/// batched `systemctl show` (via the few-second single-flight cache), and the result
/// is grouped back per service. Never fails: without systemctl every unit is
/// `unknown`, and the client renders grey dots.
#[post("/status/services", data = "<body>")]
pub async fn services_status(
    config: &State<Arc<AppConfig>>,
    body: Json<ServiceStatusRequest>,
) -> Json<ServiceStatusResponse> {
    let req = body.into_inner();
    let states = config.unit_status.get(&req.all_units()).await;
    Json(map_services(&req, &states))
}
