use std::sync::Arc;

use rocket::{get, routes, State};
use rocket_dyn_templates::Template;

use super::nix_eval::{extract_service_options, extract_services};
use super::structs::{AppConfig, IndexContext, OptionContext};

#[get("/")]
pub fn index(config: &State<Arc<AppConfig>>) -> Template {
    let svcs = extract_services(&config.nix_cmd, &config.neo_input);
    Template::render("index", IndexContext { services: svcs })
}

#[get("/option/<service>")]
pub fn option_pane(config: &State<Arc<AppConfig>>, service: &str) -> Template {
    let opts = extract_service_options(&config.nix_cmd, &config.neo_input, service);
    let options_json = serde_json::to_string(&opts).unwrap_or_else(|_| "[]".to_string());
    Template::render(
        "option_pane",
        OptionContext {
            service: service.to_string(),
            options: opts,
            options_json,
        },
    )
}

pub fn routes() -> Vec<rocket::Route> {
    routes![index, option_pane]
}
