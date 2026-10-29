use std::sync::Arc;

use rocket::http::Status;
use rocket::serde::json::Json;
use rocket::{post, State};

use crate::commands::web::locks::Blocked;
use crate::commands::web::settings::edit::{apply_edit, Edit};
use crate::commands::web::types::AppConfig;
use crate::commands::web::util::{core_section_ok, service_name_ok};

#[post("/save/<service>", data = "<payload>")]
pub async fn save_service(
    config: &State<Arc<AppConfig>>,
    service: &str,
    payload: Json<serde_json::Value>,
) -> Result<Status, Blocked> {
    if !service_name_ok(service) {
        eprintln!("web: refusing save for invalid service name {:?}", service);
        return Ok(Status::BadRequest);
    }
    apply_edit(
        config,
        Edit::ReplaceService {
            service: service.to_string(),
            payload: payload.into_inner(),
        },
    )
    .await
}

#[post("/save-core/<section>", data = "<payload>")]
pub async fn save_core_section(
    config: &State<Arc<AppConfig>>,
    section: &str,
    payload: Json<serde_json::Value>,
) -> Result<Status, Blocked> {
    if !core_section_ok(section) {
        eprintln!("web: refusing save for unknown core section {:?}", section);
        return Ok(Status::BadRequest);
    }
    apply_edit(
        config,
        Edit::ReplaceCore {
            section: section.to_string(),
            payload: payload.into_inner(),
        },
    )
    .await
}
