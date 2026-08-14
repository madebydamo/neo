use anyhow::{Context, Result};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use tokio::runtime::Runtime;
use tokio::sync::{broadcast, Mutex};

use rocket::fs::FileServer;
use rocket_dyn_templates::Template;
use toml_edit::DocumentMut;

mod action_bar;
mod activation;
mod git;
mod git_ops;
mod helper_exec;
mod nix;
mod nix_repair;
mod ops;
mod plugins;
mod routes;
mod schema_cache;
mod settings;
mod structs;
mod trigger;
mod types;
mod units;
mod util;

use action_bar::start_action_bar_watcher;
use routes::routes;
use types::AppConfig;
use util::InFlightSet;

pub fn web(
    _doc: &DocumentMut,
    settings_path: PathBuf,
    nix_cmd: &str,
    config_path: &str,
) -> Result<()> {
    // Use the resolved configuration directory for the active profile.
    // We evaluate it (not via git+file wrapper) so that saves to settings.toml
    // and any other on-disk changes are seen by the next getFlake.
    // Canonicalize so the path passed to the repl is absolute and stable
    // for expressions like (/. + configDir).
    let neo_input_for_eval = std::fs::canonicalize(config_path)
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| config_path.to_string());

    let rt = Runtime::new().context("create runtime")?;
    let template_dir = option_env!("TEMPLATE_DIR")
        .unwrap_or("templates")
        .to_string();
    let static_dir = option_env!("STATIC_DIR").unwrap_or("static").to_string();
    let nix_cmd_for_eval = nix_cmd.to_string();
    rt.block_on(async move {
        let busy = Arc::new(AtomicBool::new(true));
        let evaluator =
            nix::NixEvaluator::new(&nix_cmd_for_eval, &neo_input_for_eval, busy.clone())
                .await
                .context("start persistent nix repl for fast evals")?;
        let (unit_tx, _unit_rx) = broadcast::channel::<String>(128);
        let app_config = Arc::new(AppConfig {
            nix_cmd: nix_cmd_for_eval,
            neo_input: neo_input_for_eval,
            settings_path,
            evaluator: Arc::new(Mutex::new(evaluator)),
            eval_busy: busy,
            unit_updates: unit_tx,
            pulls_in_flight: Arc::new(InFlightSet::new()),
            clear_appdata_in_flight: Arc::new(InFlightSet::new()),
            schema_cache: Arc::new(tokio::sync::RwLock::new(
                schema_cache::SchemaCache::default(),
            )),
        });
        eprintln!(
            "web: config dir {} settings {:?}",
            app_config.neo_input, app_config.settings_path
        );

        // Push action-bar OOB updates (pending changes, reset, nix-busy) over the shared WS
        // whenever state changes — replaces the old client-side every-20s polling.
        start_action_bar_watcher(app_config.clone());

        // Background warm-up: full homeserver flake + settings + option walking can take
        // 30s–10min the first time. Spawn after start so Rocket can bind promptly; first
        // request may still wait on the evaluator mutex if warm-up is incomplete.
        let evaluator_for_warmup = app_config.evaluator.clone();
        tokio::spawn(async move {
            eprintln!("web: starting background warm-up of nix evaluator…");
            {
                let mut ev = evaluator_for_warmup.lock().await;
                let nav = ev.extract_proxied_services().await;
                if let Some(err) = nav.eval_error.error.as_ref() {
                    eprintln!("web: warm-up navigator extract failed: {err}");
                }
                let _ = ev.extract_neo_theme().await;
            }
            eprintln!("web: background warm-up complete.");
        });

        rocket::build()
            .manage(app_config)
            .attach(Template::fairing())
            .configure(rocket::Config::figment().merge(("template_dir", template_dir)))
            .mount("/static", FileServer::from(static_dir))
            .mount("/", routes())
            .launch()
            .await
            .map_err(|e| anyhow::anyhow!("rocket launch failed: {e}"))?;
        Ok::<(), anyhow::Error>(())
    })?;
    Ok(())
}
