//! Per-user UI preferences, keyed on the tinyauth user (`Remote-User`, set by SWAG
//! from the auth request). Today: the navigator sidebar layout (static/sidebar_layout.js),
//! so order and folders follow the user across devices. Without a user (auth off,
//! `just web` on the host) the routes answer 404 and the browser keeps its own copy.
use std::collections::BTreeMap;
use std::convert::Infallible;
use std::path::PathBuf;
use std::sync::Mutex;

use rocket::http::Status;
use rocket::request::{FromRequest, Outcome, Request};
use rocket::serde::json::Json;
use rocket::{delete, get, put};
use serde_json::Value;

use crate::commands::web::util::{appdata_dir, state_file};

const FILE: &str = "sidebar_layouts.json";
/// A layout is a short list of service names and groups; anything larger is not one.
const MAX_LAYOUT_BYTES: usize = 32 * 1024;
const MAX_USER_LEN: usize = 256;

/// The signed-in tinyauth user, or `None` when the request carries none.
pub struct RemoteUser(Option<String>);

#[rocket::async_trait]
impl<'r> FromRequest<'r> for RemoteUser {
    type Error = Infallible;

    async fn from_request(req: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        let user = req
            .headers()
            .get_one("Remote-User")
            .map(str::trim)
            .filter(|u| !u.is_empty() && u.len() <= MAX_USER_LEN)
            .map(str::to_string);
        Outcome::Success(RemoteUser(user))
    }
}

type Layouts = BTreeMap<String, Value>;

/// In neo's appdata, so snapshots and backups keep it. Read on every request:
/// Clear appdata and snapshot restore change the file under the running neo-web.
fn store_path() -> Option<PathBuf> {
    appdata_dir().map(|d| d.join(FILE))
}

/// Serializes read-modify-write of the layouts file.
static WRITE_LOCK: Mutex<()> = Mutex::new(());

fn load() -> Layouts {
    store_path()
        .and_then(|p| state_file::load_json(&p))
        .unwrap_or_default()
}

/// Apply `f` to the stored layouts and write them back if it changed something.
fn update(f: impl FnOnce(&mut Layouts) -> bool) -> anyhow::Result<()> {
    use anyhow::Context;
    let _guard = WRITE_LOCK
        .lock()
        .map_err(|_| anyhow::anyhow!("layouts lock poisoned"))?;
    let path = store_path().context("no appdata directory for sidebar layouts")?;
    let mut layouts = load();
    if !f(&mut layouts) {
        return Ok(());
    }
    let body = serde_json::to_string(&layouts).context("serialize sidebar layouts")?;
    state_file::write_atomic(&path, &body, 0o600)
}

fn status(res: anyhow::Result<()>) -> Status {
    match res {
        Ok(()) => Status::NoContent,
        Err(e) => {
            eprintln!("web: sidebar layout not saved: {e:#}");
            Status::InternalServerError
        }
    }
}

/// Only a JSON array of bounded size is stored; the client sanitizes the entries.
fn layout_ok(layout: &Value) -> bool {
    layout.is_array() && layout.to_string().len() <= MAX_LAYOUT_BYTES
}

#[get("/prefs/sidebar")]
pub fn sidebar_get(user: RemoteUser) -> Result<Json<Value>, Status> {
    let user = user.0.ok_or(Status::NotFound)?;
    Ok(Json(serde_json::json!({
        "layout": load().remove(&user).unwrap_or(Value::Null),
    })))
}

#[put("/prefs/sidebar", data = "<layout>")]
pub fn sidebar_put(user: RemoteUser, layout: Json<Value>) -> Status {
    let Some(user) = user.0 else {
        return Status::NotFound;
    };
    let layout = layout.into_inner();
    if !layout_ok(&layout) {
        return Status::UnprocessableEntity;
    }
    status(update(|l| {
        l.insert(user, layout.clone()).as_ref() != Some(&layout)
    }))
}

/// Reset: the user gets the server order again on every device.
#[delete("/prefs/sidebar")]
pub fn sidebar_delete(user: RemoteUser) -> Status {
    let Some(user) = user.0 else {
        return Status::NotFound;
    };
    status(update(|l| l.remove(&user).is_some()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_bounded_arrays_are_layouts() {
        assert!(layout_ok(
            &serde_json::json!(["pihole", {"id": "group:x", "items": ["a"]}])
        ));
        assert!(!layout_ok(&serde_json::json!({"layout": []})));
        assert!(!layout_ok(&serde_json::json!("pihole")));
        let big: Vec<String> = (0..4000).map(|i| format!("service-{i}")).collect();
        assert!(!layout_ok(&serde_json::json!(big)));
    }
}
