use rocket::get;
use rocket::response::content::RawHtml;

use crate::commands::web::activation;
use crate::commands::web::util::{activation_id_ok, escape_html};

fn invalid_id_html(id: &str) -> RawHtml<String> {
    RawHtml(format!(
        r#"<div class="alert alert-error text-sm">invalid activation id: {}</div>"#,
        escape_html(id)
    ))
}

#[get("/activation/monitor/<id>")]
pub fn activation_monitor(id: &str) -> RawHtml<String> {
    if !activation_id_ok(id) {
        return invalid_id_html(id);
    }
    RawHtml(activation::build_monitor_fragment(id))
}

#[get("/activation/log/<id>")]
pub fn activation_log(id: &str) -> RawHtml<String> {
    if !activation_id_ok(id) {
        return invalid_id_html(id);
    }
    RawHtml(activation::build_log_fragment(id))
}

#[get("/activation/status/<id>")]
pub fn activation_status(id: &str) -> RawHtml<String> {
    if !activation_id_ok(id) {
        return invalid_id_html(id);
    }
    RawHtml(activation::build_status_fragment(id))
}

#[get("/activation/current")]
pub fn activation_current() -> RawHtml<String> {
    if let Some(id) = activation::find_recent_in_progress_activation() {
        RawHtml(activation::build_monitor_fragment(&id))
    } else {
        RawHtml("<div class=\"text-xs\">no active activation</div>".to_string())
    }
}

#[get("/update/monitor/<id>")]
pub fn update_monitor(id: &str) -> RawHtml<String> {
    if !activation_id_ok(id) {
        return invalid_id_html(id);
    }
    RawHtml(activation::build_update_monitor_fragment(id))
}

#[get("/update/log/<id>")]
pub fn update_log(id: &str) -> RawHtml<String> {
    if !activation_id_ok(id) {
        return invalid_id_html(id);
    }
    RawHtml(activation::build_log_fragment(id))
}

#[get("/update/status/<id>")]
pub fn update_status(id: &str) -> RawHtml<String> {
    if !activation_id_ok(id) {
        return invalid_id_html(id);
    }
    RawHtml(activation::build_update_status_fragment(id))
}
