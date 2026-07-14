use std::convert::Infallible;
use std::path::PathBuf;

use rocket::request::{FromRequest, Outcome, Request};

use super::structs::AppConfig;

pub fn config_dir(cfg: &AppConfig) -> PathBuf {
    cfg.settings_path
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

pub fn sudo_cmd() -> String {
    std::env::var("SUDO_BINARY_PATH").unwrap_or_else(|_| "sudo".to_string())
}

/// True when the client sent `HX-Request: true` (HTMX AJAX / partial load).
pub struct Htmx(pub bool);

#[rocket::async_trait]
impl<'r> FromRequest<'r> for Htmx {
    type Error = Infallible;

    async fn from_request(req: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        let is_htmx = req
            .headers()
            .get_one("HX-Request")
            .map(|v| v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);
        Outcome::Success(Htmx(is_htmx))
    }
}

impl Htmx {
    pub fn is_htmx(&self) -> bool {
        self.0
    }
}
