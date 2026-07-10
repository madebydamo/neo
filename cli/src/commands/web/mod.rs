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
mod git_ops;
mod nix;
mod routes;
mod settings;
mod structs;
mod trigger;
mod units;
mod util;

use action_bar::start_action_bar_watcher;
use routes::routes;
use structs::AppConfig;

pub fn web(doc: &DocumentMut, settings_path: PathBuf, nix_cmd: &str, section: &str) -> Result<()> {
    let raw_config_dir = doc
        .get(&section)
        .and_then(|t| t.get("configPath"))
        .and_then(|u| u.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .unwrap_or(".".to_string());

    // Use the raw configuration directory (ground truth) directly.
    // We evaluate it (not via git+file wrapper) so that saves to settings.toml
    // and any other on-disk changes are seen by the next getFlake.
    // Canonicalize so the path passed to the repl is absolute and stable
    // for expressions like (/. + configDir).
    let neo_input_for_eval = std::fs::canonicalize(&raw_config_dir)
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or(raw_config_dir.clone());

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
        let (unit_tx, _unit_rx) = broadcast::channel::<String>(64);
        let app_config = Arc::new(AppConfig {
            nix_cmd: nix_cmd_for_eval,
            neo_input: neo_input_for_eval,
            settings_path,
            evaluator: Arc::new(Mutex::new(evaluator)),
            eval_busy: busy,
            unit_updates: unit_tx,
        });
        println!("{:?}", app_config);

        // Push action-bar OOB updates (pending changes, reset, nix-busy) over the shared WS
        // whenever state changes — replaces the old client-side every-20s polling.
        start_action_bar_watcher(app_config.clone());

        // Background warm-up: the heavy evaluation (builtins.getFlake on the real on-disk
        // configuration directory + full nixosConfiguration module system + readFile of the
        // live settings.toml via templates/homeserver/modules/settings.nix + walking options
        // for types/defaults/rank/icon/enabled in the extract_*.nix files) used to happen
        // synchronously inside NixEvaluator::new(). That blocked for 30-120s and prevented
        // the "Rocket has launched" line from ever appearing promptly.
        //
        // We now spawn it as a background task *after* printing AppConfig and *before*
        // awaiting the rocket launch. Result:
        //   - "started (pid ...)"
        //   - AppConfig { ... }
        //   - "Rocket has launched from http://127.0.0.1:8000"
        // appear quickly (matching the sequence the user wants).
        // The first browser request may still experience the cost if it arrives before the
        // task finishes (the Mutex will serialize it), but once the warm-up completes all
        // subsequent extracts (index, grids, panes, etc.) are fast because the repl has
        // memoized the results under `f`. On error/timeout we now return explicit error HTML (with reload) instead of silent empty data or indefinite spinners.
        let evaluator_for_warmup = app_config.evaluator.clone();
        tokio::spawn(async move {
            eprintln!("web: starting background warm-up of nix evaluator (full homeserver flake + settings.toml read + option walking; this can take 30s-10min the first time or after GC/stale locks)...");
            {
                let mut ev = evaluator_for_warmup.lock().await;
                let _ = ev.extract_proxied_services().await;
                let _ = ev.extract_neo_theme().await;
            }
            eprintln!("web: background warm-up complete. The web UI should now load fast.");
        });

        let _ = rocket::build()
            .manage(app_config)
            .attach(Template::fairing())
            .configure(rocket::Config::figment().merge(("template_dir", template_dir)))
            .mount("/static", FileServer::from(static_dir))
            .mount("/", routes())
            .launch()
            .await;
        Ok::<(), anyhow::Error>(())
    })?;
    Ok(())
}
