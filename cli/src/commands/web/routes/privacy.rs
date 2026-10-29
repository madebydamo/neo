//! Privacy map: `/configuration/privacy` (neo.privacy.categories × services × guides).
use std::sync::Arc;

use rocket::{get, State};
use rocket_dyn_templates::{Metadata, Template};
use serde::{Deserialize, Serialize};

use super::pages::configuration_shell;
use crate::commands::web::nix::errors::eval_error_ui;
use crate::commands::web::types::{AppConfig, EvalErrorUi};
use crate::commands::web::util::Htmx;

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct PrivacyService {
    pub name: String,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub tagline: String,
    #[serde(default)]
    pub replaces: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct PrivacyGuide {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub minutes: Option<u32>,
    #[serde(default)]
    pub difficulty: String,
}

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct PrivacyCategory {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub icon_tint: bool,
    #[serde(default)]
    pub cloud: String,
    #[serde(default)]
    pub home: String,
    #[serde(default)]
    pub examples: Vec<String>,
    #[serde(default)]
    pub services: Vec<PrivacyService>,
    #[serde(default)]
    pub guides: Vec<PrivacyGuide>,
    #[serde(default)]
    pub home_now: bool,
}

#[derive(Deserialize, Default)]
pub struct PrivacyExtract {
    #[serde(default)]
    pub categories: Vec<PrivacyCategory>,
}

#[derive(Serialize, Default)]
struct PrivacyContext {
    at_home: Vec<PrivacyCategory>,
    in_cloud: Vec<PrivacyCategory>,
    all: Vec<PrivacyCategory>,
    home_count: usize,
    total: usize,
    assistant: bool,
    #[serde(flatten)]
    eval_error: EvalErrorUi,
}

async fn context(config: &AppConfig) -> PrivacyContext {
    let mut ev = config.evaluator.lock().await;
    match ev.extract_privacy().await {
        Ok(x) => {
            let all = x.categories;
            let at_home: Vec<_> = all.iter().filter(|c| c.home_now).cloned().collect();
            let in_cloud: Vec<_> = all.iter().filter(|c| !c.home_now).cloned().collect();
            PrivacyContext {
                home_count: at_home.len(),
                total: all.len(),
                at_home,
                in_cloud,
                all,
                assistant: crate::commands::web::agent::available(),
                ..Default::default()
            }
        }
        Err(e) => {
            eprintln!("web: nix extract_privacy failed: {e:#}");
            PrivacyContext {
                eval_error: eval_error_ui("Nix error while building the privacy map", &e),
                ..Default::default()
            }
        }
    }
}

#[get("/configuration/privacy")]
pub async fn configuration_privacy(
    config: &State<Arc<AppConfig>>,
    htmx: Htmx,
    meta: Metadata<'_>,
) -> Template {
    let ctx = context(config).await;
    if htmx.is_htmx() {
        return Template::render("privacy", ctx);
    }
    let html = meta.render("privacy", &ctx).map(|(_, h)| h);
    configuration_shell(
        config,
        "/configuration/privacy",
        "guides",
        Some("Privacy map".into()),
        html,
    )
    .await
}
