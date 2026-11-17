//! Settings → Secrets card routes: the switch between inline → secrets.yaml →
//! encrypted (docs/design/secrets.md). The operations are the ones
//! `neo secrets …` runs (utils/secrets_store.rs); changes apply with the next
//! activation. The card itself: `web::secrets_card`.

use std::sync::Arc;

use rocket::{get, post, State};
use rocket_dyn_templates::Template;

use crate::commands::web::locks::{try_lock, Blocked};
use crate::commands::web::secrets_card::context as card_context;
use crate::commands::web::settings::save::refresh_after_settings_change;
use crate::commands::web::types::AppConfig;
use crate::commands::web::util::proc;
use crate::utils::locks::{LockSpec, OpInfo};
use crate::utils::secrets_store::Store;
use crate::utils::secrets_yaml::PLAIN_FILE;

fn card(config: &AppConfig, message: Option<(&str, &str)>) -> Template {
    Template::render("secrets_card", card_context(config, message))
}

/// GET /secrets/card — the Settings card.
#[get("/secrets/card")]
pub fn secrets_card(config: &State<Arc<AppConfig>>) -> Template {
    card(config, None)
}

fn lock(label: &str) -> Result<crate::utils::locks::LockGuard, Blocked> {
    try_lock(&LockSpec::system_shared(), OpInfo::new("secrets", label))
}

async fn secret_paths(config: &AppConfig) -> Result<Vec<Vec<String>>, String> {
    config
        .eval
        .secret_paths()
        .await
        .map_err(|e| format!("Could not list secret settings: {e:#}"))
}

/// Run a secrets store operation (sops, the root helper) off the async workers.
async fn with_store<F>(config: &AppConfig, f: F) -> Result<String, String>
where
    F: FnOnce(&Store) -> anyhow::Result<String> + Send + 'static,
{
    let store = config.secrets_store();
    proc::blocking_try(move || f(&store).map_err(|e| format!("{e:#}"))).await
}

fn done(config: &AppConfig, result: Result<String, String>) -> Template {
    match result {
        Ok(msg) => {
            refresh_after_settings_change(config);
            card(config, Some(("success", &msg)))
        }
        Err(e) => card(config, Some(("error", &e))),
    }
}

/// POST /secrets/split — settings.toml → secrets.yaml.
#[post("/secrets/split")]
pub async fn secrets_split(config: &State<Arc<AppConfig>>) -> Result<Template, Blocked> {
    let _lock = lock("Moving secrets to secrets.yaml")?;
    let result = match secret_paths(config).await {
        Ok(p) => {
            with_store(config, move |s| {
                s.split(&p).map(|moved| {
                    format!(
                        "Moved {} secret setting(s) to {PLAIN_FILE}. Activate to apply.",
                        moved.len()
                    )
                })
            })
            .await
        }
        Err(e) => Err(e),
    };
    Ok(done(config, result))
}

/// POST /secrets/enable — encrypt (host key + ssh-ed25519 authorized keys).
#[post("/secrets/enable")]
pub async fn secrets_enable(config: &State<Arc<AppConfig>>) -> Result<Template, Blocked> {
    let _lock = lock("Encrypting secrets")?;
    let result = match secret_paths(config).await {
        Ok(p) => {
            with_store(config, move |s| {
                s.enable(&p).map(|r| {
                    format!(
                        "Encrypted {} secret(s) for {} key(s){}. Activate to apply.",
                        r.values,
                        r.recipients,
                        if r.escrowed {
                            ", host key escrowed"
                        } else {
                            ""
                        }
                    )
                })
            })
            .await
        }
        Err(e) => Err(e),
    };
    Ok(done(config, result))
}

/// POST /secrets/disable — back to a plain secrets.yaml.
#[post("/secrets/disable")]
pub async fn secrets_disable(config: &State<Arc<AppConfig>>) -> Result<Template, Blocked> {
    let _lock = lock("Decrypting secrets")?;
    let result = with_store(config, |s| {
        s.disable()
            .map(|n| format!("Decrypted {n} secret(s) into {PLAIN_FILE}. Activate to apply."))
    })
    .await;
    Ok(done(config, result))
}

/// POST /secrets/rekey — re-encrypt for the current keys.
#[post("/secrets/rekey")]
pub async fn secrets_rekey(config: &State<Arc<AppConfig>>) -> Result<Template, Blocked> {
    let _lock = lock("Re-encrypting secrets")?;
    let result = with_store(config, |s| {
        s.rekey()
            .map(|n| format!("Re-encrypted for {n} key(s). Activate to apply."))
    })
    .await;
    Ok(done(config, result))
}
