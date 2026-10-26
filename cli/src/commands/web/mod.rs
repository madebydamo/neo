use anyhow::{Context, Result};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use tokio::runtime::Runtime;
use tokio::sync::{broadcast, Mutex};

use rocket::fs::FileServer;
use rocket_dyn_templates::Template;

mod action_bar;
mod diff;
mod git;
mod helper_exec;
mod icons;
mod locks;
mod navigator;
mod nix;
mod nix_repair;
mod ops;
mod plugins;
mod power;
mod resources;
mod routes;
mod schema_cache;
mod settings;
mod trigger;
mod types;
mod units;
mod util;
mod version_tree;
mod zfs;

use action_bar::start_action_bar_watcher;
use locks::start_lock_watcher;
use routes::routes;
use types::AppConfig;

pub fn web(settings_path: PathBuf, nix_cmd: &str, config_path: &str) -> Result<()> {
    // Use the resolved configuration directory for the active profile.
    // We evaluate it (not via git+file wrapper) so that saves to settings.toml
    // and any other on-disk changes are seen by the next getFlake.
    // Canonicalize so the path passed to the repl is absolute and stable
    // for expressions like (/. + configDir).
    let neo_input_for_eval = std::fs::canonicalize(config_path)
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| config_path.to_string());

    let rt = Runtime::new().context("create runtime")?;
    let template_dir = util::template_dir();
    let static_dir = util::static_dir();
    rt.block_on(async move {
        let busy = Arc::new(AtomicBool::new(true));
        let evaluator = nix::NixEvaluator::new(nix_cmd, &neo_input_for_eval, busy.clone())
            .await
            .context("start persistent nix repl for fast evals")?;
        let icons = evaluator.icons.clone();
        let (unit_tx, _unit_rx) = broadcast::channel::<String>(128);
        let app_config = Arc::new(AppConfig {
            settings_path,
            evaluator: Arc::new(Mutex::new(evaluator)),
            eval_busy: busy,
            unit_updates: unit_tx,
            pulls_in_flight: Default::default(),
            clear_appdata_in_flight: Default::default(),
            schema_cache: Default::default(),
            unit_status: Default::default(),
            resources: Arc::new(resources::ResourceSampler::new()),
        });
        eprintln!(
            "web: config dir {} settings {:?}",
            neo_input_for_eval, app_config.settings_path
        );

        // Push action-bar OOB updates (pending changes, reset, nix-busy) over the shared WS
        // whenever state changes — replaces the old client-side every-20s polling.
        start_action_bar_watcher(app_config.clone());
        // Push operation-lock holders (web jobs, CLI, timers) so conflicting buttons disable.
        start_lock_watcher(app_config.clone());

        // Background warm-up: full homeserver flake + settings + option walking can take
        // 30s–10min the first time. Spawn after start so Rocket can bind promptly; first
        // request may still wait on the evaluator mutex if warm-up is incomplete.
        let evaluator_for_warmup = app_config.evaluator.clone();
        tokio::spawn(async move {
            eprintln!("web: starting background warm-up of nix evaluator…");
            if let Some(err) = evaluator_for_warmup.lock().await.warm_up().await {
                eprintln!("web: warm-up navigator extract failed: {err}");
            }
            eprintln!("web: background warm-up complete.");
        });

        let widgets_dir = PathBuf::from(&template_dir).join("options/widgets");
        mount_static_assets(rocket::build(), &static_dir, &widgets_dir)
            .manage(icons)
            .manage(app_config)
            .attach(Template::fairing())
            .configure(rocket::Config::figment().merge(("template_dir", template_dir)))
            .mount("/", routes())
            .launch()
            .await
            .map_err(|e| anyhow::anyhow!("rocket launch failed: {e}"))?;
        Ok::<(), anyhow::Error>(())
    })?;
    Ok(())
}

/// Serve `cli/static` at `/static` and colocated widget JS at `/static/widgets`.
///
/// Both FileServers generate a catch-all `GET /<path..>` at rank 10. `/static/<path..>`
/// overlaps `/static/widgets/<path..>`, so the more specific mount must use a lower
/// rank (matched first) or Rocket refuses to launch with "collisions detected".
fn mount_static_assets(
    rocket: rocket::Rocket<rocket::Build>,
    static_dir: impl AsRef<std::path::Path>,
    widgets_dir: impl AsRef<std::path::Path>,
) -> rocket::Rocket<rocket::Build> {
    rocket
        .mount("/static", FileServer::from(static_dir.as_ref()))
        .mount(
            "/static/widgets",
            FileServer::from(widgets_dir.as_ref()).rank(9),
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket::http::Status;
    use rocket::local::blocking::Client;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT: AtomicU64 = AtomicU64::new(0);

    fn temp_asset_dirs() -> (PathBuf, PathBuf) {
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let root =
            std::env::temp_dir().join(format!("neo-web-static-{}-{}", std::process::id(), n));
        let static_dir = root.join("static");
        let widgets_dir = root.join("widgets");
        fs::create_dir_all(&static_dir).unwrap();
        fs::create_dir_all(&widgets_dir).unwrap();
        fs::write(static_dir.join("option_form.js"), "/* option form */\n").unwrap();
        fs::write(widgets_dir.join("registry.js"), "/* NeoWidgets */\n").unwrap();
        (static_dir, widgets_dir)
    }

    #[test]
    fn widget_js_is_served_without_route_collisions() {
        let (static_dir, widgets_dir) = temp_asset_dirs();
        let rocket = mount_static_assets(rocket::build(), &static_dir, &widgets_dir);
        let client = Client::tracked(rocket).expect("static FileServers must not collide");

        let widget = client.get("/static/widgets/registry.js").dispatch();
        assert_eq!(widget.status(), Status::Ok);
        assert!(
            widget.into_string().unwrap().contains("NeoWidgets"),
            "widget FileServer should serve colocated registry.js"
        );

        let form = client.get("/static/option_form.js").dispatch();
        assert_eq!(form.status(), Status::Ok);
        assert!(form.into_string().unwrap().contains("option form"));
    }
}
