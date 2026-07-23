// Full-page and HTMX partial routes for the navigator and configuration UI.
use std::path::PathBuf;
use std::sync::Arc;

use rocket::http::ContentType;
use rocket::response::{Redirect, Responder};
use rocket::{get, State};
use rocket_dyn_templates::Template;

use crate::commands::web::routes::branches::branches_template;
use crate::commands::web::structs::{
    AppConfig, ConfigurationPageContext, IndexContext, NavigatorContext,
};
use crate::commands::web::util::Htmx;

/// Full shell template or a fragment / redirect (non-HTMX deep links).
#[derive(Responder)]
pub enum ShellOrPartial {
    Template(Template),
    Redirect(Redirect),
}

/// Web app manifest at a root URL with the correct MIME type (Seerr-style).
/// Loaded from STATIC_DIR at runtime — crane's cargo source filter omits `static/`, so
/// `include_str!` would fail in the nix build. FileServer also lacks a `.webmanifest` MIME map.
#[get("/site.webmanifest")]
pub fn site_webmanifest() -> Option<(ContentType, String)> {
    let static_dir = option_env!("STATIC_DIR").unwrap_or("static");
    let path = PathBuf::from(static_dir).join("manifest.json");
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

/// Proxied service icons for the navigator sidebar (nix-eval; loaded after the page shell).
#[get("/nav-services")]
pub async fn nav_services(config: &State<Arc<AppConfig>>) -> Template {
    let data = {
        let mut ev = config.evaluator.lock().await;
        ev.extract_proxied_services().await
    };
    Template::render("nav_services", data)
}

async fn configuration_shell(
    config: &State<Arc<AppConfig>>,
    initial_content_url: &str,
    initial_tab: &str,
    initial_detail: Option<String>,
) -> Template {
    let ctx = {
        let mut ev = config.evaluator.lock().await;
        let services = ev.extract_services().await;
        let theme = ev.extract_neo_theme().await;
        ConfigurationPageContext {
            theme,
            error: services.error,
            error_kind: services.error_kind,
            can_store_repair: services.can_store_repair,
            can_flake_update: services.can_flake_update,
            initial_content_url: initial_content_url.to_string(),
            initial_tab: initial_tab.to_string(),
            initial_detail,
        }
    };
    Template::render("configuration", ctx)
}

async fn option_pane_template(config: &State<Arc<AppConfig>>, service: &str) -> Template {
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

async fn core_pane_template(config: &State<Arc<AppConfig>>, section: &str) -> Template {
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
        configuration_shell(config, "/configuration", "services", None).await
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
        configuration_shell(config, "/configuration/settings", "settings", None).await
    }
}

/// Versioning / branches — full shell or partial.
#[get("/configuration/versioning")]
pub async fn configuration_versioning(config: &State<Arc<AppConfig>>, htmx: Htmx) -> Template {
    if htmx.is_htmx() {
        branches_template(config)
    } else {
        configuration_shell(config, "/configuration/versioning", "versioning", None).await
    }
}

/// Service option pane — full shell or partial.
#[get("/configuration/option/<service>")]
pub async fn configuration_option(
    config: &State<Arc<AppConfig>>,
    service: &str,
    htmx: Htmx,
) -> Template {
    if htmx.is_htmx() {
        option_pane_template(config, service).await
    } else {
        configuration_shell(
            config,
            &format!("/configuration/option/{service}"),
            "services",
            Some(service.to_string()),
        )
        .await
    }
}

/// Core section option pane — full shell or partial.
#[get("/configuration/core/<section>")]
pub async fn configuration_core(
    config: &State<Arc<AppConfig>>,
    section: &str,
    htmx: Htmx,
) -> Template {
    if htmx.is_htmx() {
        core_pane_template(config, section).await
    } else {
        configuration_shell(
            config,
            &format!("/configuration/core/{section}"),
            "settings",
            Some(section.to_string()),
        )
        .await
    }
}

/// Legacy partial alias; non-HTMX browsers redirect to the canonical shell URL.
#[get("/option/<service>")]
pub async fn option_pane(
    config: &State<Arc<AppConfig>>,
    service: &str,
    htmx: Htmx,
) -> ShellOrPartial {
    if htmx.is_htmx() {
        ShellOrPartial::Template(option_pane_template(config, service).await)
    } else {
        ShellOrPartial::Redirect(Redirect::to(format!("/configuration/option/{service}")))
    }
}

/// Legacy services grid partial (HTMX); non-HTMX → shell.
#[get("/services-grid")]
pub async fn services_grid(config: &State<Arc<AppConfig>>, htmx: Htmx) -> ShellOrPartial {
    if htmx.is_htmx() {
        ShellOrPartial::Template(services_grid_template(config).await)
    } else {
        ShellOrPartial::Redirect(Redirect::to("/configuration"))
    }
}

/// Legacy core grid partial (HTMX); non-HTMX → shell.
#[get("/core-grid")]
pub fn core_grid(htmx: Htmx) -> ShellOrPartial {
    if htmx.is_htmx() {
        ShellOrPartial::Template(core_grid_template())
    } else {
        ShellOrPartial::Redirect(Redirect::to("/configuration/settings"))
    }
}

/// Legacy core section partial; non-HTMX → shell.
#[get("/core/<section>")]
pub async fn core_pane(
    config: &State<Arc<AppConfig>>,
    section: &str,
    htmx: Htmx,
) -> ShellOrPartial {
    if htmx.is_htmx() {
        ShellOrPartial::Template(core_pane_template(config, section).await)
    } else {
        ShellOrPartial::Redirect(Redirect::to(format!("/configuration/core/{section}")))
    }
}
