use std::sync::Arc;

use rocket::response::content::RawHtml;
use rocket::response::stream::{Event, EventStream};
use rocket::{get, post, State};
use tokio::io::{AsyncBufReadExt, BufReader as AsyncBufReader};
use tokio::process::Command as AsyncCommand;

use crate::commands::web::structs::AppConfig;
use crate::commands::web::units::{
    clear_appdata_btn_oob, clear_appdata_out_oob, is_safe_appdata_path, normalize_container_unit,
    perform_unit_action, run_clear_appdata, run_container_pull, schedule_unit_refresh_burst,
    try_begin_clear_appdata, try_begin_pull, unit_controls_oob_fragment, unit_name_valid,
    update_out_oob, UnitAction,
};
use crate::commands::web::util::{escape_html, service_name_ok, sudo_cmd};

/// Shared post-action path: kick systemctl, push OOB once, then burst-refresh while it settles.
/// Buttons use hx-swap="none"; the returned OOB still updates the controls row.
fn unit_action_response(
    action: UnitAction,
    unit: &str,
    config: &State<Arc<AppConfig>>,
) -> RawHtml<String> {
    if !unit_name_valid(unit) {
        return RawHtml(String::new());
    }
    perform_unit_action(action, unit);
    let oob = unit_controls_oob_fragment(unit, config);
    let _ = config.unit_updates.send(oob.clone());
    schedule_unit_refresh_burst(unit.to_string(), Arc::clone(config));
    RawHtml(oob)
}

#[post("/unit/restart/<unit>")]
pub fn unit_restart(unit: &str, config: &State<Arc<AppConfig>>) -> RawHtml<String> {
    unit_action_response(UnitAction::Restart, unit, config)
}

#[post("/unit/start/<unit>")]
pub fn unit_start(unit: &str, config: &State<Arc<AppConfig>>) -> RawHtml<String> {
    unit_action_response(UnitAction::Start, unit, config)
}

#[post("/unit/stop/<unit>")]
pub fn unit_stop(unit: &str, config: &State<Arc<AppConfig>>) -> RawHtml<String> {
    unit_action_response(UnitAction::Stop, unit, config)
}

/// Kick off an async docker pull+restart. Returns immediately with OOB status +
/// disabled ↻ button; progress and completion are pushed over `/ws/status`.
#[post("/container/update/<container>")]
pub fn container_update(container: &str, config: &State<Arc<AppConfig>>) -> RawHtml<String> {
    if !unit_name_valid(container) {
        return RawHtml(String::new());
    }
    let (unit, cname) = normalize_container_unit(container);

    if !try_begin_pull(config, &unit) {
        let out = update_out_oob(
            &unit,
            r#"<span class="inline-flex items-center gap-1 text-info"><span class="loading loading-spinner loading-xs"></span><span>already pulling…</span></span>"#,
            "docker pull already in progress",
        );
        let ctl = unit_controls_oob_fragment(&unit, config);
        return RawHtml(format!("{out}{ctl}"));
    }

    let out = update_out_oob(
        &unit,
        r#"<span class="inline-flex items-center gap-1 text-info"><span class="loading loading-spinner loading-xs"></span><span>starting pull…</span></span>"#,
        "starting docker pull",
    );
    let ctl = unit_controls_oob_fragment(&unit, config);
    let _ = config.unit_updates.send(out.clone());
    let _ = config.unit_updates.send(ctl.clone());

    let cfg = Arc::clone(config);
    tokio::spawn(async move {
        run_container_pull(unit, cname, cfg).await;
    });

    RawHtml(format!("{out}{ctl}"))
}

/// Clear a service's appdata: stop all related units, rm -rf the declared path,
/// then start only units that were running beforehand.
/// Path and units come from trusted flake evaluation (never from the client).
#[post("/service/<service>/clear-appdata")]
pub async fn clear_appdata(service: &str, config: &State<Arc<AppConfig>>) -> RawHtml<String> {
    if !service_name_ok(service) {
        return RawHtml(String::new());
    }

    let pane = {
        let mut ev = config.evaluator.lock().await;
        ev.extract_service_options(service).await
    };

    if let Some(err) = pane.error.as_ref() {
        let (inner, title) = (
            format!(
                r#"<span class="text-error truncate">✗ {}</span>"#,
                escape_html(&format!("eval failed: {}", err))
            ),
            format!("eval failed: {}", err),
        );
        return RawHtml(clear_appdata_out_oob(service, &inner, &title));
    }

    let appdata = match pane.appdata.as_ref() {
        Some(p) if !p.is_empty() => p.clone(),
        _ => {
            let msg = "no appdata path declared for this service";
            let inner = format!(r#"<span class="text-error truncate">✗ {}</span>"#, msg);
            return RawHtml(clear_appdata_out_oob(service, &inner, msg));
        }
    };

    if !is_safe_appdata_path(&appdata, pane.appdata_root.as_deref()) {
        let msg = format!("refusing unsafe appdata path: {}", appdata);
        let inner = format!(
            r#"<span class="text-error truncate">✗ {}</span>"#,
            escape_html(&msg)
        );
        return RawHtml(clear_appdata_out_oob(service, &inner, &msg));
    }

    if !try_begin_clear_appdata(config, service) {
        let out = clear_appdata_out_oob(
            service,
            r#"<span class="inline-flex items-center gap-1 text-info"><span class="loading loading-spinner loading-xs"></span><span>already clearing…</span></span>"#,
            "clear appdata already in progress",
        );
        let btn = clear_appdata_btn_oob(service, &appdata, true);
        return RawHtml(format!("{out}{btn}"));
    }

    let units: Vec<String> = pane.units.iter().map(|u| u.name.clone()).collect();
    let out = clear_appdata_out_oob(
        service,
        r#"<span class="inline-flex items-center gap-1 text-info"><span class="loading loading-spinner loading-xs"></span><span>starting…</span></span>"#,
        "starting clear appdata",
    );
    let btn = clear_appdata_btn_oob(service, &appdata, true);
    let _ = config.unit_updates.send(out.clone());
    let _ = config.unit_updates.send(btn.clone());

    let cfg = Arc::clone(config);
    let svc = service.to_string();
    tokio::spawn(async move {
        run_clear_appdata(svc, appdata, units, cfg).await;
    });

    RawHtml(format!("{out}{btn}"))
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
            let sudo = sudo_cmd();
            let spawn_res = AsyncCommand::new(&sudo)
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
