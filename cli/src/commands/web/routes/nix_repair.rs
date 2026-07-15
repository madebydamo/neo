// Web routes for Nix store repair jobs.
use std::sync::Arc;

use rocket::post;
use rocket::response::content::RawHtml;
use rocket::{get, State};

use crate::commands::web::nix_repair;
use crate::commands::web::structs::AppConfig;
use crate::commands::web::util::{escape_html, repair_id_ok};

fn invalid_id_html(id: &str) -> RawHtml<String> {
    RawHtml(format!(
        r#"<div class="alert alert-error text-sm">invalid repair id: {}</div>"#,
        escape_html(id)
    ))
}

/// Start (or attach to) a store verify+repair job; returns the monitor fragment.
#[post("/nix/repair")]
pub fn nix_repair_start(config: &State<Arc<AppConfig>>) -> RawHtml<String> {
    let id = nix_repair::start_store_verify_repair(config.inner().clone());
    RawHtml(nix_repair::build_repair_monitor_fragment(&id))
}

#[get("/nix/repair/monitor/<id>")]
pub fn nix_repair_monitor(id: &str) -> RawHtml<String> {
    if !repair_id_ok(id) {
        return invalid_id_html(id);
    }
    RawHtml(nix_repair::build_repair_monitor_fragment(id))
}

#[get("/nix/repair/log/<id>")]
pub fn nix_repair_log(id: &str) -> RawHtml<String> {
    if !repair_id_ok(id) {
        return invalid_id_html(id);
    }
    RawHtml(nix_repair::build_repair_log_fragment(id))
}

#[get("/nix/repair/status/<id>")]
pub fn nix_repair_status(id: &str) -> RawHtml<String> {
    if !repair_id_ok(id) {
        return invalid_id_html(id);
    }
    RawHtml(nix_repair::build_repair_status_fragment(id))
}
