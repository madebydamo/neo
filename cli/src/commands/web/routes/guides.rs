//! Guides tab: `/configuration/guides` (cards) and `/configuration/guides/<id>`.
use std::collections::HashMap;
use std::sync::Arc;

use rocket::{get, State};
use rocket_dyn_templates::{Metadata, Template};
use serde::Serialize;

use super::pages::configuration_shell;
use crate::commands::web::guides::{self, Guide, GuidesExtract, RenderCtx, TocEntry};
use crate::commands::web::nix::errors::eval_error_ui;
use crate::commands::web::types::{AppConfig, EvalErrorUi};
use crate::commands::web::util::{service_name_ok, Htmx};

#[derive(Serialize, Default)]
struct GuidesGridContext {
    guides: Vec<Guide>,
    tags: Vec<String>,
    disabled: bool,
    #[serde(flatten)]
    eval_error: EvalErrorUi,
}

#[derive(Serialize, Default)]
struct GuidePageContext {
    guide: Option<Guide>,
    html: String,
    toc: Vec<TocEntry>,
    related: Vec<Guide>,
    missing: Vec<String>,
    assistant: bool,
    #[serde(flatten)]
    eval_error: EvalErrorUi,
}

pub async fn load_guides(config: &AppConfig) -> Result<GuidesExtract, EvalErrorUi> {
    let mut ev = config.evaluator.lock().await;
    ev.extract_guides().await.map_err(|e| {
        eprintln!("web: nix extract_guides failed: {e:#}");
        eval_error_ui("Nix error while extracting guides", &e)
    })
}

/// Guides that use `service`, for the option pane strip (empty on errors).
pub async fn guides_for_service(config: &AppConfig, service: &str) -> Vec<Guide> {
    match load_guides(config).await {
        Ok(x) => guides::for_service(service, &x.guides)
            .into_iter()
            .cloned()
            .collect(),
        Err(_) => vec![],
    }
}

/// Topic pills shown above the guides grid.
const MAX_FILTER_TAGS: usize = 8;

async fn grid_context(config: &AppConfig) -> GuidesGridContext {
    match load_guides(config).await {
        Ok(x) => {
            // Filter row: only topics shared by several guides, most used
            // first; one-off tags stay searchable but get no pill.
            let mut counts: std::collections::BTreeMap<&str, usize> = Default::default();
            for t in x.guides.iter().flat_map(|g| &g.tags) {
                *counts.entry(t.as_str()).or_default() += 1;
            }
            let mut ranked: Vec<(&str, usize)> =
                counts.into_iter().filter(|(_, n)| *n >= 2).collect();
            ranked.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
            let tags: Vec<String> = ranked
                .into_iter()
                .take(MAX_FILTER_TAGS)
                .map(|(t, _)| t.to_string())
                .collect();
            GuidesGridContext {
                disabled: !x.enabled,
                guides: x.guides,
                tags,
                ..Default::default()
            }
        }
        Err(eval_error) => GuidesGridContext {
            eval_error,
            ..Default::default()
        },
    }
}

async fn page_context(config: &AppConfig, id: &str) -> GuidePageContext {
    let x = match load_guides(config).await {
        Ok(x) => x,
        Err(eval_error) => {
            return GuidePageContext {
                eval_error,
                ..Default::default()
            }
        }
    };
    let Some(guide) = x.guides.iter().find(|g| g.id == id).cloned() else {
        return GuidePageContext::default();
    };
    let icons: HashMap<String, String> = x
        .guides
        .iter()
        .flat_map(|g| g.services.iter().chain(g.optional.iter()))
        .filter_map(|s| s.icon.clone().map(|i| (s.name.clone(), i)))
        .collect();
    let (html, toc) = guides::render(
        &guide.body,
        &RenderCtx {
            broken: &guide.broken_links,
            service_icons: &icons,
            assistant: x.hermes_enabled,
        },
    );
    let related = guides::related(&guide, &x.guides, 3)
        .into_iter()
        .cloned()
        .collect();
    GuidePageContext {
        missing: guide.missing(),
        guide: Some(guide),
        html,
        toc,
        related,
        assistant: crate::commands::web::agent::available(),
        ..Default::default()
    }
}

#[get("/configuration/guides")]
pub async fn configuration_guides(
    config: &State<Arc<AppConfig>>,
    htmx: Htmx,
    meta: Metadata<'_>,
) -> Template {
    let ctx = grid_context(config).await;
    if htmx.is_htmx() {
        return Template::render("guides_grid", ctx);
    }
    let html = meta.render("guides_grid", &ctx).map(|(_, h)| h);
    configuration_shell(config, "/configuration/guides", "guides", None, html).await
}

#[get("/configuration/guides/<id>")]
pub async fn configuration_guide(
    config: &State<Arc<AppConfig>>,
    id: &str,
    htmx: Htmx,
    meta: Metadata<'_>,
) -> Template {
    let ctx = if service_name_ok(id) {
        page_context(config, id).await
    } else {
        GuidePageContext::default()
    };
    if htmx.is_htmx() {
        return Template::render("guide", ctx);
    }
    let title = ctx.guide.as_ref().map(|g| g.title.clone());
    let html = meta.render("guide", &ctx).map(|(_, h)| h);
    configuration_shell(
        config,
        &format!("/configuration/guides/{id}"),
        "guides",
        title,
        html,
    )
    .await
}
