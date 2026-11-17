//! JSON error answer for fetch() clients: a status and `{"ok": false, "error": …}`.
use rocket::http::Status;
use rocket::response::Responder;
use rocket::serde::json::Json;
use rocket::Request;
use serde::Serialize;

#[derive(Debug)]
pub struct JsonError(pub Status, pub String);

impl JsonError {
    pub fn new(status: Status, msg: impl Into<String>) -> Self {
        JsonError(status, msg.into())
    }
}

#[derive(Serialize)]
struct Body {
    ok: bool,
    error: String,
}

impl<'r> Responder<'r, 'static> for JsonError {
    fn respond_to(self, req: &'r Request<'_>) -> rocket::response::Result<'static> {
        let body = Body {
            ok: false,
            error: self.1,
        };
        (self.0, Json(body)).respond_to(req)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn body_shape() {
        let b = serde_json::to_value(Body {
            ok: false,
            error: "nope".into(),
        })
        .unwrap();
        assert_eq!(b, serde_json::json!({"ok": false, "error": "nope"}));
    }
}
