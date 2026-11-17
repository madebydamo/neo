//! SSH public key cards (`web::ssh`): show and regenerate.

use rocket::response::content::RawHtml;
use rocket::{get, post};

use crate::commands::web::locks::{try_lock, Blocked};
use crate::commands::web::ssh;
use crate::commands::web::util::proc;
use crate::utils::locks::{LockSpec, OpInfo};

/// GET /ssh/<key>/public-key-card — HTML fragment for the config UI.
#[get("/ssh/<key>/public-key-card")]
pub fn ssh_public_key_card(key: &str) -> Option<RawHtml<String>> {
    Some(RawHtml(ssh::card(ssh::spec(key)?)))
}

/// POST /ssh/<key>/regenerate — delete the keypair and re-run its key script.
#[post("/ssh/<key>/regenerate")]
pub async fn ssh_regenerate(key: &str) -> Result<Option<RawHtml<String>>, Blocked> {
    let Some(spec) = ssh::spec(key) else {
        return Ok(None);
    };
    // Not while an activation regenerates the same keys.
    let _lock = try_lock(
        &LockSpec::system_shared(),
        OpInfo::new("ssh-key", format!("Regenerating the {key} key")),
    )?;
    let rotated = proc::blocking_try(move || ssh::rotate_key(spec)).await;
    Ok(Some(RawHtml(ssh::rotated_card(spec, rotated))))
}
