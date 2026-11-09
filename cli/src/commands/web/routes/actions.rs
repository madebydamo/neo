use std::sync::Arc;

use rocket::response::content::RawHtml;
use rocket::{post, State};

use crate::commands::web::locks::Blocked;
use crate::commands::web::routes::changes::{apply_or_activate, discard_response};
use crate::commands::web::trigger::trigger_update;
use crate::commands::web::types::AppConfig;
use crate::commands::web::util::proc;

#[post("/flake/update")]
pub async fn flake_update() -> Result<RawHtml<String>, Blocked> {
    proc::blocking(trigger_update).await.map_err(Blocked)?
}

#[post("/actions/activate")]
pub async fn actions_activate() -> Result<RawHtml<String>, Blocked> {
    apply_or_activate().await
}

#[post("/actions/reset")]
pub async fn actions_reset(config: &State<Arc<AppConfig>>) -> Result<RawHtml<String>, Blocked> {
    discard_response(config).await
}
