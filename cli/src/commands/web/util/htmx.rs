use std::convert::Infallible;

use rocket::request::{FromRequest, Outcome, Request};

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
