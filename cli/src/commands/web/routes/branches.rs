use std::sync::Arc;

use rocket::response::content::{RawHtml, RawJson};
use rocket::{get, post, State};
use rocket_dyn_templates::Template;

use crate::commands::generation::{list_system_generations_with_sudo, GenerationMode};
use crate::commands::git_cmd;
use crate::commands::web::activation;
use crate::commands::web::git_ops::{
    activation_branch_for_rev, activation_graph, diff_settings, enabled_services_at_rev,
    is_worktree_dirty, list_activation_branches,
};
use crate::commands::web::settings::save::refresh_after_settings_change;
use crate::commands::web::structs::{AppConfig, BranchesContext};
use crate::commands::web::trigger::{trigger_activation, trigger_generation_switch};
use crate::commands::web::util::{
    branch_ok, config_dir, escape_html, generation_ok, rev_ok, sudo_cmd,
};

/// Shared branches / versioning partial (used by `/branches` and `/configuration/versioning`).
pub fn branches_template(config: &AppConfig) -> Template {
    let dir = config_dir(&config.settings_path);
    let dir_str = dir.to_str().unwrap_or(".");
    let brs = list_activation_branches(dir_str);
    Template::render(
        "branches",
        BranchesContext {
            graph: String::new(),
            branches: brs,
        },
    )
}

/// Legacy partial alias (same fragment as `/configuration/versioning` with HX-Request).
#[get("/branches")]
pub fn branches(config: &State<Arc<AppConfig>>) -> Template {
    branches_template(config)
}

/// Structured activation commit graph for the D3 UI.
#[get("/versioning/graph")]
pub fn versioning_graph(config: &State<Arc<AppConfig>>) -> RawJson<String> {
    let dir = config_dir(&config.settings_path);
    let dir_str = dir.to_str().unwrap_or(".");
    let g = activation_graph(dir_str);
    RawJson(
        serde_json::to_string(&g)
            .unwrap_or_else(|_| r#"{"commits":[],"head":"","currentBranch":""}"#.to_string()),
    )
}

/// Enabled/disabled services from `settings.toml` at a revision.
#[get("/versioning/commit/<rev>/services")]
pub fn versioning_services(config: &State<Arc<AppConfig>>, rev: &str) -> RawJson<String> {
    if !rev_ok(rev) {
        return RawJson(serde_json::json!({"error": "invalid rev"}).to_string());
    }
    let dir = config_dir(&config.settings_path);
    let dir_str = dir.to_str().unwrap_or(".");
    match enabled_services_at_rev(dir_str, rev) {
        Ok(s) => RawJson(serde_json::to_string(&s).unwrap_or_else(|_| "{}".to_string())),
        Err(e) => RawJson(serde_json::json!({"error": e}).to_string()),
    }
}

/// Unified `settings.toml` diff between two revs.
#[get("/versioning/diff?<a>&<b>")]
pub fn versioning_diff(config: &State<Arc<AppConfig>>, a: &str, b: &str) -> RawHtml<String> {
    if !rev_ok(a) || !rev_ok(b) {
        return RawHtml(r#"<div class="text-error text-sm">invalid revision</div>"#.to_string());
    }
    let dir = config_dir(&config.settings_path);
    let dir_str = dir.to_str().unwrap_or(".");
    match diff_settings(dir_str, a, b) {
        Ok(diff) => {
            if diff.trim().is_empty() {
                RawHtml(
                    r#"<div class="text-sm opacity-60">No differences in settings.toml</div>"#
                        .to_string(),
                )
            } else {
                RawHtml(format!(
                    r#"<pre class="text-xs font-mono overflow-auto max-h-[40vh] bg-base-300 p-2 rounded whitespace-pre">{}</pre>"#,
                    escape_html(&diff)
                ))
            }
        }
        Err(e) => RawHtml(format!(
            r#"<div class="text-error text-sm">{}</div>"#,
            escape_html(&e)
        )),
    }
}

/// List NixOS system generations.
#[get("/versioning/generations")]
pub fn versioning_generations() -> RawJson<String> {
    let list = list_system_generations_with_sudo(&sudo_cmd());
    RawJson(
        serde_json::to_string(&list)
            .unwrap_or_else(|_| r#"{"generations":[],"unavailable":true}"#.to_string()),
    )
}

#[post("/git/switch/<br>")]
pub fn git_switch(config: &State<Arc<AppConfig>>, br: &str) -> RawHtml<String> {
    if !branch_ok(br) {
        return RawHtml(format!(
            r#"<span class="text-error text-xs">invalid branch: {}</span>"#,
            escape_html(br)
        ));
    }
    if activation::is_activation_in_progress() {
        return RawHtml(
            "<span class=\"text-error text-xs\">activation in progress — cannot switch</span>"
                .to_string(),
        );
    }
    let dir = config_dir(&config.settings_path);
    let dir_str = dir.to_str().unwrap_or(".");
    if is_worktree_dirty(dir_str) {
        return RawHtml(
            "<span class=\"text-error text-xs\">working tree dirty — commit, revert, or discard changes first</span>"
                .to_string(),
        );
    }
    match git_cmd(dir_str, &["switch", br]) {
        Ok(()) => {
            refresh_after_settings_change(&config);
            RawHtml(format!(
                r#"<span class="text-success text-xs">switched to {}</span>"#,
                escape_html(br)
            ))
        }
        Err(e) => RawHtml(format!(
            r#"<span class="text-error text-xs">switch failed: {}</span>"#,
            escape_html(&e.to_string())
        )),
    }
}

/// Checkout activation branch tip for rev, then trigger full activate.
#[post("/versioning/activate/<rev>")]
pub fn versioning_activate(config: &State<Arc<AppConfig>>, rev: &str) -> RawHtml<String> {
    if !rev_ok(rev) {
        return RawHtml(r#"<span class="text-error text-xs">invalid rev</span>"#.to_string());
    }
    if activation::is_activation_in_progress() {
        return RawHtml(
            "<span class=\"text-error text-xs\">activation already in progress</span>".to_string(),
        );
    }
    let dir = config_dir(&config.settings_path);
    let dir_str = dir.to_str().unwrap_or(".");
    if is_worktree_dirty(dir_str) {
        return RawHtml(
            "<span class=\"text-error text-xs\">working tree dirty — cannot activate from history</span>"
                .to_string(),
        );
    }
    let branch = match activation_branch_for_rev(dir_str, rev) {
        Ok(Some(b)) if branch_ok(&b) => b,
        Ok(None) => {
            return RawHtml(
                r#"<span class="text-error text-xs">activate only allowed on activation branch tips</span>"#
                    .to_string(),
            );
        }
        Ok(Some(_)) => {
            return RawHtml(
                r#"<span class="text-error text-xs">branch name not allowed</span>"#.to_string(),
            );
        }
        Err(e) => {
            return RawHtml(format!(
                r#"<span class="text-error text-xs">{}</span>"#,
                escape_html(&e)
            ));
        }
    };
    if let Err(e) = git_cmd(dir_str, &["switch", &branch]) {
        return RawHtml(format!(
            r#"<span class="text-error text-xs">checkout failed: {}</span>"#,
            escape_html(&e.to_string())
        ));
    }
    refresh_after_settings_change(&config);
    // Reuse existing oneshot activate path (returns HTML for monitor).
    trigger_activation(&config)
}

#[post("/versioning/generations/<n>/switch")]
pub fn versioning_gen_switch(n: u64) -> RawHtml<String> {
    if !generation_ok(n) {
        return RawHtml(
            r#"<span class="text-error text-xs">invalid generation</span>"#.to_string(),
        );
    }
    // Detached oneshot: switch-to-configuration stops neo-web; must not run in-process.
    trigger_generation_switch(n, GenerationMode::Switch)
}
