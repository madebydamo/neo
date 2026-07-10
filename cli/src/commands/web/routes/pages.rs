use std::sync::Arc;

use rocket::{get, State};
use rocket_dyn_templates::Template;

use crate::commands::web::structs::{AppConfig, IndexContext};

#[get("/")]
pub async fn index(config: &State<Arc<AppConfig>>) -> Template {
    let (mut data, theme) = {
        let mut ev = config.evaluator.lock().await;
        let data = ev.extract_proxied_services().await;
        let theme = ev.extract_neo_theme().await;
        (data, theme)
    };
    data.theme = theme;
    Template::render("index", data)
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
        IndexContext {
            services: vec![],
            ..Default::default()
        },
    )
}

#[get("/core/<section>")]
pub async fn core_pane(config: &State<Arc<AppConfig>>, section: &str) -> Template {
    let pane = {
        let mut ev = config.evaluator.lock().await;
        ev.extract_neo_section(section).await
    };
    Template::render("option_pane", pane)
}
