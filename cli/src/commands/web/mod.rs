use anyhow::{Context, Result};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::runtime::Runtime;

use rocket_dyn_templates::Template;
use toml_edit::DocumentMut;

mod nix_eval;
mod server;
mod structs;

use server::routes;
use structs::AppConfig;

pub fn web(doc: &DocumentMut, settings_path: PathBuf, nix_cmd: &str, section: &str) -> Result<()> {
    let neo_input = doc
        .get(&section)
        .and_then(|t| t.get("configPath"))
        .and_then(|u| u.as_str())
        .map(|u| format!("git+file:{}", u))
        .filter(|s| !s.is_empty())
        .unwrap_or("github:madebydamo/neo".to_string())
        .to_string();
    let app_config = Arc::new(AppConfig {
        nix_cmd: nix_cmd.to_string(),
        neo_input,
        settings_path,
    });
    println!("{:?}", app_config);
    let rt = Runtime::new().context("create runtime")?;
    let template_dir = option_env!("TEMPLATE_DIR")
        .unwrap_or("templates")
        .to_string();
    rt.block_on(async move {
        let _ = rocket::build()
            .manage(app_config)
            .attach(Template::fairing())
            .configure(rocket::Config::figment().merge(("template_dir", template_dir)))
            .mount("/", routes())
            .launch()
            .await;
        Ok::<(), anyhow::Error>(())
    })?;
    Ok(())
}
