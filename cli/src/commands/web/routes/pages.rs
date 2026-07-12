use std::sync::Arc;

use rocket::{get, State};
use rocket_dyn_templates::Template;

use crate::commands::web::structs::{AppConfig, IndexContext, NavigatorContext};

/// Instant shell for the navigator — no nix-eval. Services load via `/nav-services` (htmx).
#[get("/")]
pub fn index() -> Template {
    Template::render(
        "index",
        NavigatorContext {
            theme: "lofi".to_string(),
            ..Default::default()
        },
    )
}

/// Proxied service icons for the navigator sidebar (nix-eval; loaded after the page shell).
#[get("/nav-services")]
pub async fn nav_services(config: &State<Arc<AppConfig>>) -> Template {
    let data = {
        let mut ev = config.evaluator.lock().await;
        ev.extract_proxied_services().await
    };
    Template::render("nav_services", data)
}

#[get("/configuration")]
pub async fn configuration(config: &State<Arc<AppConfig>>) -> Template {
    let ctx = {
        let mut ev = config.evaluator.lock().await;
        let mut ctx = ev.extract_services().await;
        ctx.theme = ev.extract_neo_theme().await;
        ctx
    };
    Template::render("configuration", ctx)
}

#[get("/option/<service>")]
pub async fn option_pane(config: &State<Arc<AppConfig>>, service: &str) -> Template {
    let pane = {
        let mut ev = config.evaluator.lock().await;
        ev.extract_service_options(service).await
    };
    {
        let mut cache = config.schema_cache.write().await;
        cache.put(false, service, pane.options.clone());
    }
    Template::render("option_pane", pane)
}

#[get("/services-grid")]
pub async fn services_grid(config: &State<Arc<AppConfig>>) -> Template {
    let ctx = {
        let mut ev = config.evaluator.lock().await;
        ev.extract_services().await
    };
    Template::render("services_grid", ctx)
}

#[get("/core-grid")]
pub fn core_grid(_config: &State<Arc<AppConfig>>) -> Template {
    Template::render(
        "core_grid",
        IndexContext::default(),
    )
}

#[get("/core/<section>")]
pub async fn core_pane(config: &State<Arc<AppConfig>>, section: &str) -> Template {
    let pane = {
        let mut ev = config.evaluator.lock().await;
        ev.extract_neo_section(section).await
    };
    {
        let mut cache = config.schema_cache.write().await;
        cache.put(true, section, pane.options.clone());
    }
    Template::render("option_pane", pane)
}
