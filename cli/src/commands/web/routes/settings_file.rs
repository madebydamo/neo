//! Working-tree settings.toml download / upload.

use std::io::Cursor;
use std::sync::Arc;

use rocket::data::{Data, ToByteUnit};
use rocket::http::{ContentType, Header, Status};
use rocket::response::content::RawHtml;
use rocket::response::{Responder, Response};
use rocket::{get, post, Request, State};

use crate::commands::web::settings::file::{export_settings_toml, import_settings_toml};
use crate::commands::web::settings::save::refresh_after_settings_change;
use crate::commands::web::structs::AppConfig;
use crate::commands::web::util::{alert_html, AlertKind};

pub struct TomlAttachment {
    body: String,
}

impl<'r> Responder<'r, 'static> for TomlAttachment {
    fn respond_to(self, _: &'r Request<'_>) -> rocket::response::Result<'static> {
        Response::build()
            .header(ContentType::new("application", "toml"))
            .header(Header::new(
                "Content-Disposition",
                r#"attachment; filename="settings.toml""#,
            ))
            .sized_body(self.body.len(), Cursor::new(self.body))
            .ok()
    }
}

#[get("/settings/download")]
pub fn download_settings(config: &State<Arc<AppConfig>>) -> Result<TomlAttachment, Status> {
    match export_settings_toml(&config.settings_path) {
        Ok(body) => Ok(TomlAttachment { body }),
        Err(_) => Err(Status::NotFound),
    }
}

#[post("/settings/upload", data = "<data>")]
pub async fn upload_settings(data: Data<'_>, config: &State<Arc<AppConfig>>) -> RawHtml<String> {
    let raw = match data.open(2.mebibytes()).into_string().await {
        Ok(s) if s.is_complete() => s.into_inner(),
        Ok(_) => {
            return RawHtml(alert_html(
                AlertKind::Error,
                "Uploaded file is too large (max 2 MiB)",
            ));
        }
        Err(e) => {
            return RawHtml(alert_html(
                AlertKind::Error,
                &format!("Could not read upload: {e}"),
            ));
        }
    };
    match import_settings_toml(&config.settings_path, &raw) {
        Ok(()) => {
            refresh_after_settings_change(config);
            RawHtml(alert_html(
                AlertKind::Success,
                "Replaced working-tree settings.toml. Review pending changes and apply to activate.",
            ))
        }
        Err(e) => RawHtml(alert_html(AlertKind::Error, &e)),
    }
}
