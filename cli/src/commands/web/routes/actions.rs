use std::path::PathBuf;
use std::sync::Arc;

use rocket::response::content::RawHtml;
use rocket::{post, State};
use toml_edit::DocumentMut;

use crate::commands::log::OperationLog;
use crate::commands::paste_settings::paste_settings;
use crate::commands::web::action_bar::broadcast_action_bar;
use crate::commands::web::activation;
use crate::commands::web::structs::AppConfig;
use crate::commands::web::trigger::{trigger_activation, trigger_systemd_run};
use crate::commands::web::util::config_dir;

#[post("/flake/update")]
pub fn flake_update(config: &State<Arc<AppConfig>>) -> RawHtml<String> {
    activation::gc_old_activations();
    if let Some(id) = activation::find_recent_in_progress_activation() {
        return RawHtml(format!(
            "<div class=\"alert alert-info text-sm\">Activation {} in progress — cannot update</div>",
            id
        ));
    }
    if let Some(id) = activation::find_recent_in_progress_update() {
        return RawHtml(format!(
            "<div class=\"alert alert-info text-sm\">Update {} already in progress</div>",
            id
        ));
    }
    let ts = crate::commands::get_timestamp();
    let op = OperationLog::new_update(&ts);
    op.init_for_web_trigger(&ts);
    trigger_systemd_run("update", "NEO_UPDATE_SUFFIX", op.suffix(), op.log_path());
    broadcast_action_bar(&config);
    RawHtml(activation::build_update_monitor_fragment(op.id()))
}

#[post("/actions/activate")]
pub fn actions_activate(config: &State<Arc<AppConfig>>) -> RawHtml<String> {
    let html = trigger_activation(&config);
    broadcast_action_bar(&config);
    html
}

#[post("/actions/reset")]
pub fn actions_reset(config: &State<Arc<AppConfig>>) -> RawHtml<String> {
    let dir = config_dir(&config);
    let dir_str = dir.to_str().unwrap_or(".");
    let source = PathBuf::from("/etc/neo/settings.toml");
    let dummy = DocumentMut::new();
    let res = paste_settings(dir_str, &source, &dummy, false, &config.nix_cmd);
    if res.is_ok() {
        let ev = config.evaluator.clone();
        tokio::spawn(async move {
            let mut g = ev.lock().await;
            let _ = g.refresh().await;
        });
        broadcast_action_bar(&config);
    }
    match res {
        Ok(()) => RawHtml(
            "<div class=\"alert alert-success text-sm\">Reset done (settings restored from /etc/neo). Close to refresh state.</div>"
                .to_string(),
        ),
        Err(e) => RawHtml(format!(
            "<div class=\"alert alert-error text-sm\">Reset failed: {}</div>",
            e
        )),
    }
}
