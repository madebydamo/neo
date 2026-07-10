use std::sync::Arc;

use rocket::response::content::RawHtml;
use rocket::{get, post, State};
use rocket_dyn_templates::Template;

use crate::commands::git_cmd;
use crate::commands::web::activation;
use crate::commands::web::git_ops::{get_activation_graph, list_activation_branches};
use crate::commands::web::structs::{AppConfig, BranchesContext};
use crate::commands::web::util::config_dir;

#[get("/branches")]
pub fn branches(config: &State<Arc<AppConfig>>) -> Template {
    let dir = config_dir(&config);
    let dir_str = dir.to_str().unwrap_or(".");
    let graph = get_activation_graph(dir_str);
    let brs = list_activation_branches(dir_str);
    Template::render(
        "branches",
        BranchesContext {
            graph,
            branches: brs,
        },
    )
}

#[post("/git/switch/<br>")]
pub fn git_switch(config: &State<Arc<AppConfig>>, br: &str) -> RawHtml<String> {
    if activation::is_activation_in_progress() {
        return RawHtml(
            "<span class=\"text-error text-xs\">activation in progress — cannot switch</span>"
                .to_string(),
        );
    }
    let dir = config_dir(&config);
    let dir_str = dir.to_str().unwrap_or(".");
    match git_cmd(dir_str, &["switch", br]) {
        Ok(()) => RawHtml(format!(
            "<span class=\"text-success text-xs\">switched to {}</span>",
            br
        )),
        Err(e) => RawHtml(format!(
            "<span class=\"text-error text-xs\">switch failed: {}</span>",
            e
        )),
    }
}
