use std::path::Path;
use std::sync::Arc;

use rocket::form::Form;
use rocket::response::content::{RawHtml, RawJson};
use rocket::{get, post, FromForm, State};
use rocket_dyn_templates::Template;

use crate::commands::web::diff::{render_changes, RenderOptions};
use crate::commands::web::git::{
    activation_branch_for_rev, collect_changes, enabled_services_at_rev, is_worktree_dirty,
    lock_updates, resolve_rev, settings_semantic, DiffRange,
};
use crate::commands::web::locks::{try_lock, Blocked};
use crate::commands::web::settings::save::refresh_after_settings_change;
use crate::commands::web::trigger::{
    is_activation_in_progress, trigger_activation, trigger_generation_switch,
};
use crate::commands::web::types::AppConfig;
use crate::commands::web::util::{
    branch_ok, err_div, err_span, generation_ok, proc, rev_ok, sudo_cmd,
};
use crate::commands::web::version_tree;
use crate::commands::web::version_tree::notes::NoteTarget;
use crate::utils::locks::{LockSpec, OpInfo};
use crate::utils::{git_cmd, GenerationMode};

/// Versioning tab partial (`/configuration/versioning`); data loads client-side.
pub fn branches_template() -> Template {
    Template::render("branches", serde_json::json!({}))
}

/// Unified version tree: config commits + system generations on one timeline
/// (lanes, commit ↔ generation links, sync status). `limit` = rows to return.
#[get("/versioning/tree?<limit>")]
pub async fn versioning_tree(
    config: &State<Arc<AppConfig>>,
    limit: Option<usize>,
) -> RawJson<String> {
    let dir = config.config_dir();
    let dir_str = dir.to_str().unwrap_or(".").to_string();
    let limit = limit.unwrap_or(version_tree::PAGE);
    let sudo = sudo_cmd();
    let view = proc::blocking(move || version_tree::load(&dir_str, limit, &sudo)).await;
    match view {
        Ok(v) => RawJson(
            serde_json::to_string(&v)
                .unwrap_or_else(|e| serde_json::json!({ "error": e.to_string() }).to_string()),
        ),
        Err(e) => RawJson(serde_json::json!({ "error": e }).to_string()),
    }
}

/// Body of `POST /versioning/notes`: `kind` = `commit` | `generation`,
/// `id` = full commit id | generation number, `note` (empty clears).
#[derive(FromForm)]
pub struct NoteForm {
    kind: String,
    id: String,
    note: String,
}

/// Set or clear the note on a settings version or a system generation.
#[post("/versioning/notes", data = "<form>")]
pub async fn versioning_note(form: Form<NoteForm>) -> RawJson<String> {
    let f = form.into_inner();
    let target = match f.kind.as_str() {
        "commit" => Ok(NoteTarget::Commit(f.id.clone())),
        "generation" => match f.id.parse::<u64>() {
            Ok(n) => version_tree::generation_created(n)
                .map(|c| NoteTarget::Generation(n, c))
                .ok_or_else(|| format!("generation {n} not found")),
            Err(_) => Err("invalid generation".to_string()),
        },
        _ => Err("invalid note target".to_string()),
    };
    let res = match target {
        Ok(t) => proc::blocking(move || {
            version_tree::notes::set(&t, &f.note).map_err(|e| format!("{e:#}"))
        })
        .await
        .and_then(|r| r),
        Err(e) => Err(e),
    };
    RawJson(match res {
        Ok(()) => serde_json::json!({ "ok": true }).to_string(),
        Err(e) => serde_json::json!({ "error": e }).to_string(),
    })
}

/// Enabled/disabled services from `settings.toml` at a revision.
#[get("/versioning/commit/<rev>/services")]
pub async fn versioning_services(config: &State<Arc<AppConfig>>, rev: &str) -> RawJson<String> {
    if !rev_ok(rev) {
        return RawJson(serde_json::json!({"error": "invalid rev"}).to_string());
    }
    let dir = config.config_dir();
    let rev = rev.to_string();
    let services = proc::blocking(move || enabled_services_at_rev(&dir, &rev))
        .await
        .and_then(|r| r);
    match services {
        Ok(s) => RawJson(serde_json::to_string(&s).unwrap_or_else(|_| "{}".to_string())),
        Err(e) => RawJson(serde_json::json!({"error": e}).to_string()),
    }
}

/// Changes between two revs: file tree, semantic settings summary and full diffs.
#[get("/versioning/diff?<a>&<b>")]
pub async fn versioning_diff(config: &State<Arc<AppConfig>>, a: &str, b: &str) -> RawHtml<String> {
    if !rev_ok(a) || !rev_ok(b) {
        return err_div("invalid revision");
    }
    let dir = config.config_dir();
    let (a, b) = (a.to_string(), b.to_string());
    match proc::blocking(move || diff_html(&dir, &a, &b)).await {
        Ok(html) => html,
        Err(e) => err_div(&e),
    }
}

fn diff_html(dir: &Path, a: &str, b: &str) -> RawHtml<String> {
    let revs = resolve_rev(dir, a).and_then(|ra| Ok((ra, resolve_rev(dir, b)?)));
    let (ra, rb) = match revs {
        Ok(r) => r,
        Err(e) => return err_div(&e),
    };
    let range = DiffRange::Revs { from: &ra, to: &rb };
    match collect_changes(dir, range) {
        Ok(set) if set.is_empty() => RawHtml(
            r#"<div class="text-sm text-base-content/60 py-6 text-center">No differences between these versions</div>"#
                .to_string(),
        ),
        Ok(set) => {
            let semantic = settings_semantic(dir, &set, range);
            let inputs = lock_updates(dir, &set, range);
            RawHtml(render_changes(
                &set,
                &RenderOptions {
                    subtitle: None,
                    semantic: semantic.as_ref(),
                    inputs: &inputs,
                },
            ))
        }
        Err(e) => err_div(&format!("{e:#}")),
    }
}

/// Checkout activation branch tip for rev, then trigger full activate.
#[post("/versioning/activate/<rev>")]
pub async fn versioning_activate(
    config: &State<Arc<AppConfig>>,
    rev: &str,
) -> Result<RawHtml<String>, Blocked> {
    if !rev_ok(rev) {
        return Ok(err_span("invalid rev"));
    }
    let (cfg, rev) = (Arc::clone(config), rev.to_string());
    proc::blocking(move || activate_rev(&cfg, &rev))
        .await
        .map_err(Blocked)?
}

fn activate_rev(config: &AppConfig, rev: &str) -> Result<RawHtml<String>, Blocked> {
    // Held across the checkout; released right before the activation takes it.
    let lock = try_lock(
        &LockSpec::system_change(),
        OpInfo::new("activation", "Version restore"),
    )?;
    if is_activation_in_progress() {
        return Ok(err_span("activation already in progress"));
    }
    let dir = config.config_dir();
    if is_worktree_dirty(&dir) {
        return Ok(err_span(
            "working tree dirty — cannot activate from history",
        ));
    }
    let branch = match activation_branch_for_rev(&dir, rev) {
        Ok(Some(b)) if branch_ok(&b) => b,
        Ok(None) => return Ok(err_span("activate only allowed on activation branch tips")),
        Ok(Some(_)) => return Ok(err_span("branch name not allowed")),
        Err(e) => return Ok(err_span(&e)),
    };
    if let Err(e) = git_cmd(dir.to_str().unwrap_or("."), &["switch", &branch]) {
        return Ok(err_span(&format!("checkout failed: {e:#}")));
    }
    refresh_after_settings_change(config);
    drop(lock);
    // Reuse existing oneshot activate path (returns HTML for monitor).
    trigger_activation()
}

#[post("/versioning/generations/<n>/switch")]
pub async fn versioning_gen_switch(n: u64) -> Result<RawHtml<String>, Blocked> {
    if !generation_ok(n) {
        return Ok(err_span("invalid generation"));
    }
    // Detached oneshot: switch-to-configuration stops neo-web; must not run in-process.
    proc::blocking(move || trigger_generation_switch(n, GenerationMode::Switch))
        .await
        .map_err(Blocked)?
}
