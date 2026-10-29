// Full-page and HTMX partial routes for the navigator and configuration UI.
use std::path::PathBuf;
use std::sync::Arc;

use rocket::http::ContentType;
use rocket::{get, State};
use rocket_dyn_templates::{Metadata, Template};

use crate::commands::web::icons::IconSet;
use crate::commands::web::navigator;
use crate::commands::web::routes::branches::branches_template;
use crate::commands::web::schema_cache::load_pane;
use crate::commands::web::types::{
    AppConfig, ConfigurationPageContext, IndexContext, NavigatorContext, OptionPaneContext,
};
use crate::commands::web::util::Htmx;

/// Web app manifest at a root URL with the correct MIME type (Seerr-style).
/// Loaded from STATIC_DIR at runtime. FileServer also lacks a `.webmanifest` MIME map.
#[get("/site.webmanifest")]
pub fn site_webmanifest() -> Option<(ContentType, String)> {
    let path = PathBuf::from(crate::commands::web::util::static_dir()).join("manifest.json");
    let body = std::fs::read_to_string(path).ok()?;
    Some((ContentType::new("application", "manifest+json"), body))
}

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

/// Proxied service icons for the navigator sidebar (loaded after the page shell).
/// Reads the running generation's build-time file; nix-eval only when it is absent.
#[get("/nav-services")]
pub async fn nav_services(config: &State<Arc<AppConfig>>, icons: &State<Arc<IconSet>>) -> Template {
    if let Some(data) = navigator::from_build_file(icons) {
        return Template::render("nav_services", data);
    }
    let data = {
        let mut ev = config.evaluator.lock().await;
        ev.extract_proxied_services().await
    };
    Template::render("nav_services", data)
}

pub(super) async fn configuration_shell(
    config: &State<Arc<AppConfig>>,
    initial_content_url: &str,
    initial_tab: &str,
    initial_detail: Option<String>,
    initial_content_html: Option<String>,
) -> Template {
    let ctx = {
        let mut ev = config.evaluator.lock().await;
        let services = ev.extract_services().await;
        let theme = ev.extract_neo_theme().await;
        ConfigurationPageContext {
            theme,
            eval_error: services.eval_error,
            initial_content_url: initial_content_url.to_string(),
            initial_tab: initial_tab.to_string(),
            initial_detail,
            initial_content_html,
            assistant: crate::commands::web::agent::available(),
        }
    };
    Template::render("configuration", ctx)
}

async fn pane_context(
    config: &State<Arc<AppConfig>>,
    is_core: bool,
    name: &str,
) -> OptionPaneContext {
    load_pane(config, is_core, name).await
}

async fn pane_template(config: &State<Arc<AppConfig>>, is_core: bool, name: &str) -> Template {
    Template::render("option_pane", pane_context(config, is_core, name).await)
}

/// Render the option/core pane to HTML for embedding in a full-page shell (deep links).
fn render_pane_html(meta: &Metadata<'_>, pane: &OptionPaneContext) -> Option<String> {
    meta.render("option_pane", pane).map(|(_, html)| html)
}

async fn services_grid_template(config: &State<Arc<AppConfig>>) -> Template {
    let ctx = {
        let mut ev = config.evaluator.lock().await;
        ev.extract_services().await
    };
    Template::render("services_grid", ctx)
}

fn core_grid_template() -> Template {
    Template::render("core_grid", IndexContext::default())
}

/// Shared body for `/configuration` and `/configuration/services` (identical UX).
async fn configuration_services_body(config: &State<Arc<AppConfig>>, htmx: Htmx) -> Template {
    if htmx.is_htmx() {
        services_grid_template(config).await
    } else {
        configuration_shell(config, "/configuration", "services", None, None).await
    }
}

/// Configuration home — full shell for browser navigation; services grid partial for HTMX.
/// Canonical services URL is `/configuration` (breadcrumb / logo go here, not `/configuration/services`).
#[get("/configuration")]
pub async fn configuration(config: &State<Arc<AppConfig>>, htmx: Htmx) -> Template {
    configuration_services_body(config, htmx).await
}

/// Services grid — full shell for browser navigation, partial for HTMX.
#[get("/configuration/services")]
pub async fn configuration_services(config: &State<Arc<AppConfig>>, htmx: Htmx) -> Template {
    configuration_services_body(config, htmx).await
}

/// Core settings grid — full shell or partial.
#[get("/configuration/settings")]
pub async fn configuration_settings(config: &State<Arc<AppConfig>>, htmx: Htmx) -> Template {
    if htmx.is_htmx() {
        core_grid_template()
    } else {
        configuration_shell(config, "/configuration/settings", "settings", None, None).await
    }
}

/// Versioning / branches — full shell or partial.
#[get("/configuration/versioning")]
pub async fn configuration_versioning(config: &State<Arc<AppConfig>>, htmx: Htmx) -> Template {
    if htmx.is_htmx() {
        branches_template()
    } else {
        configuration_shell(
            config,
            "/configuration/versioning",
            "versioning",
            None,
            None,
        )
        .await
    }
}

/// Service option pane — full shell (SSR pane) or HTMX partial.
#[get("/configuration/option/<service>")]
pub async fn configuration_option(
    config: &State<Arc<AppConfig>>,
    service: &str,
    htmx: Htmx,
    meta: Metadata<'_>,
) -> Template {
    let mut pane = pane_context(config, false, service).await;
    pane.guides = super::guides::guides_for_service(config, service).await;
    pane.assistant = crate::commands::web::agent::available();
    if htmx.is_htmx() {
        Template::render("option_pane", pane)
    } else {
        let html = render_pane_html(&meta, &pane);
        configuration_shell(
            config,
            &format!("/configuration/option/{service}"),
            "services",
            Some(service.to_string()),
            html,
        )
        .await
    }
}

/// Core section option pane — full shell (SSR pane) or HTMX partial.
#[get("/configuration/core/<section>")]
pub async fn configuration_core(
    config: &State<Arc<AppConfig>>,
    section: &str,
    htmx: Htmx,
    meta: Metadata<'_>,
) -> Template {
    if htmx.is_htmx() {
        pane_template(config, true, section).await
    } else {
        let pane = pane_context(config, true, section).await;
        let html = render_pane_html(&meta, &pane);
        configuration_shell(
            config,
            &format!("/configuration/core/{section}"),
            "settings",
            Some(section.to_string()),
            html,
        )
        .await
    }
}
