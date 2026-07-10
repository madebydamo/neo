use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use super::structs::{AppConfig, BranchInfo, BranchesContext, IndexContext};
use rocket::response::content::RawHtml;
use rocket::response::stream::{Event, EventStream};
use rocket::serde::json::Json;
use rocket::{get, http::Status, post, routes, State};
use rocket_dyn_templates::Template;
use rocket_ws::{Channel, Message, WebSocket};
use tokio::io::{AsyncBufReadExt, BufReader as AsyncBufReader};
use tokio::process::Command as AsyncCommand;
use toml_edit::{DocumentMut, Item, Table, Value};

use crate::commands::log::OperationLog;
use crate::commands::paste_settings::paste_settings;
use crate::commands::{get_current_branch, git_cmd};

use super::activation;

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
    let mut ctx = {
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

fn json_to_toml_value(v: &serde_json::Value) -> Option<Value> {
    match v {
        serde_json::Value::Null => None,
        serde_json::Value::Bool(b) => Some(Value::from(*b)),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Some(Value::from(i))
            } else if let Some(f) = n.as_f64() {
                Some(Value::from(f))
            } else {
                None
            }
        }
        serde_json::Value::String(s) => Some(Value::from(s.clone())),
        serde_json::Value::Array(arr) => {
            let mut tarr = toml_edit::Array::new();
            for item in arr {
                if let Some(tv) = json_to_toml_value(item) {
                    tarr.push(tv);
                }
            }
            Some(Value::from(tarr))
        }
        serde_json::Value::Object(_) => None,
    }
}

fn json_to_toml_item(v: &serde_json::Value) -> Option<Item> {
    match v {
        serde_json::Value::Null => None,
        serde_json::Value::Bool(b) => Some(Item::Value(Value::from(*b))),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Some(Item::Value(Value::from(i)))
            } else if let Some(f) = n.as_f64() {
                Some(Item::Value(Value::from(f)))
            } else {
                None
            }
        }
        serde_json::Value::String(s) => Some(Item::Value(Value::from(s.clone()))),
        serde_json::Value::Array(arr) => {
            let mut tarr = toml_edit::Array::new();
            for item in arr {
                if let Some(tv) = json_to_toml_value(item) {
                    tarr.push(tv);
                }
            }
            Some(Item::Value(Value::from(tarr)))
        }
        serde_json::Value::Object(obj) => {
            let mut ttable = Table::new();
            for (k, val) in obj {
                if let Some(ti) = json_to_toml_item(val) {
                    ttable.insert(k, ti);
                }
            }
            Some(Item::Table(ttable))
        }
    }
}

fn insert_dotted(table: &mut Table, dotted_key: &str, value: Value) {
    let parts: Vec<&str> = dotted_key.split('.').collect();
    if parts.is_empty() {
        return;
    }
    let mut current = table;
    for (i, part) in parts.iter().enumerate() {
        if i == parts.len() - 1 {
            current.insert(part, Item::Value(value));
            return;
        }
        // ensure next level is a table (avoid overlapping borrows)
        {
            let entry = current.entry(part).or_insert(Item::Table(Table::new()));
            if !entry.is_table() {
                *entry = Item::Table(Table::new());
            }
        }
        if let Some(t) = current.get_mut(part).and_then(|e| e.as_table_mut()) {
            current = t;
        } else {
            return;
        }
    }
}

fn config_dir(cfg: &AppConfig) -> PathBuf {
    cfg.settings_path
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."))
}

fn trigger_systemd_run(subcommand: &str, env_var: &str, suffix: &str, log_path: &std::path::Path) {
    let sudo_cmd = std::env::var("SUDO_BINARY_PATH").unwrap_or_else(|_| "sudo".to_string());
    let nix_bin = std::env::var("NIX_BINARY_PATH")
        .unwrap_or_else(|_| "/run/current-system/sw/bin/nix".to_string());
    let unit = format!("neo-{}@{}.service", subcommand, suffix);
    let neo_bin = "/run/current-system/sw/bin/neo";
    let desc = format!("{} systemd-run --unit={} (as homeserver)", sudo_cmd, unit);
    let mut run_cmd = Command::new(&sudo_cmd);
    run_cmd.args([
        "systemd-run",
        "--collect",
        "--no-ask-password",
        "--no-block",
        "--unit",
        &unit,
        "--service-type=oneshot",
        "--uid=homeserver",
        "--gid=homeserver",
        "-E",
        &format!("NIX_BINARY_PATH={}", nix_bin),
        "-E",
        &format!("SUDO_BINARY_PATH={}", sudo_cmd),
        "-E",
        &format!("{}={}", env_var, suffix),
        "-E",
        "PATH=/run/current-system/sw/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin",
        "--property",
        &format!("StandardOutput=append:{}", log_path.to_string_lossy()),
        "--property",
        &format!("StandardError=append:{}", log_path.to_string_lossy()),
        "--property",
        &format!("Description=Neo one-shot {} {}", subcommand, suffix),
        neo_bin,
        subcommand,
    ]);
    let _ = crate::commands::execute_command(&mut run_cmd, &desc);
}

fn trigger_activation(config: &AppConfig) -> RawHtml<String> {
    activation::gc_old_activations();
    let ts = crate::commands::get_timestamp();
    let op = OperationLog::new_activation(&ts);
    op.init_for_web_trigger(&ts);
    if let Some(other) = activation::find_recent_in_progress_activation() {
        if other != op.id() {
            return RawHtml(format!("<div class=\"alert alert-error text-sm\">Another activation {} in progress (or auto-update). Wait.</div>", other));
        }
    }
    trigger_systemd_run(
        "activate",
        "NEO_ACTIVATION_SUFFIX",
        op.suffix(),
        op.log_path(),
    );
    // If launch failed synchronously the state remains "triggered"; the unit would update it on real run.
    RawHtml(activation::build_monitor_fragment(op.id()))
}

fn list_activation_branches(config_path: &str) -> Vec<BranchInfo> {
    let names: Vec<String> = Command::new("git")
        .current_dir(config_path)
        .args([
            "for-each-ref",
            "--format=%(refname:short)",
            "--sort=-committerdate",
            "refs/heads/activation_*",
        ])
        .output()
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .map(|s| s.to_string())
                .collect()
        })
        .unwrap_or_default();
    let cur = get_current_branch(config_path).unwrap_or_default();
    names
        .into_iter()
        .map(|name| BranchInfo {
            name: name.clone(),
            is_current: name == cur,
        })
        .collect()
}

fn get_activation_graph(config_path: &str) -> String {
    let out = Command::new("git")
        .current_dir(config_path)
        .args([
            "log",
            "--graph",
            "--no-color",
            "--oneline",
            "--decorate",
            "-25",
            "--branches=activation_*",
        ])
        .output();
    match out {
        Ok(o) => {
            let mut t = String::from_utf8_lossy(&o.stdout).into_owned();
            if !o.stderr.is_empty() {
                t.push_str(&String::from_utf8_lossy(&o.stderr));
            }
            t
        }
        Err(e) => format!("graph error: {}", e),
    }
}

fn worktree_changed_and_summary(cfg: &AppConfig) -> (bool, String) {
    let dir = config_dir(cfg);
    let staged = Command::new("git")
        .current_dir(&dir)
        .args(["diff", "--cached", "--quiet"])
        .status()
        .map(|s| !s.success())
        .unwrap_or(false);
    let unstaged = Command::new("git")
        .current_dir(&dir)
        .args(["diff", "--quiet"])
        .status()
        .map(|s| !s.success())
        .unwrap_or(false);
    let changed = staged || unstaged;
    if !changed {
        return (false, String::new());
    }
    let status = Command::new("git")
        .current_dir(&dir)
        .args(["status", "--porcelain", "-b", "--short"])
        .output();
    let stat = Command::new("git")
        .current_dir(&dir)
        .args(["diff", "--stat", "--no-color"])
        .output();
    let mut text = String::new();
    if let Ok(o) = status {
        text.push_str(&String::from_utf8_lossy(&o.stdout));
    }
    text.push_str("\n");
    if let Ok(o) = stat {
        text.push_str(&String::from_utf8_lossy(&o.stdout));
    }
    (true, text)
}

fn settings_toml_has_diff(cfg: &AppConfig) -> bool {
    let dir = config_dir(cfg);
    let unstaged = Command::new("git")
        .current_dir(&dir)
        .args(["diff", "--quiet", "--", "settings.toml"])
        .status()
        .map(|s| !s.success())
        .unwrap_or(false);
    let staged = Command::new("git")
        .current_dir(&dir)
        .args(["diff", "--cached", "--quiet", "--", "settings.toml"])
        .status()
        .map(|s| !s.success())
        .unwrap_or(false);
    unstaged || staged
}

fn get_settings_toml_diff(cfg: &AppConfig) -> String {
    let dir = config_dir(cfg);
    let output = Command::new("git")
        .current_dir(&dir)
        .args(["diff", "--no-color", "HEAD", "--", "settings.toml"])
        .output();
    match output {
        Ok(o) => {
            let mut t = String::from_utf8_lossy(&o.stdout).into_owned();
            let e = String::from_utf8_lossy(&o.stderr);
            if !e.is_empty() {
                t.push_str(&e);
            }
            t
        }
        Err(e) => format!("git diff error: {}", e),
    }
}

#[post("/save/<service>", data = "<payload>")]
pub fn save_service(
    config: &State<Arc<AppConfig>>,
    service: &str,
    payload: Json<serde_json::Value>,
) -> Status {
    // Guard against the client accidentally sending the literal fallback value
    // (or an empty service). This used to result in [services.service] in the TOML.
    if service.trim().is_empty() || service == "service" {
        eprintln!("web: refusing save for invalid service name {:?}", service);
        return Status::BadRequest;
    }

    let settings_path = &config.settings_path;

    // Ensure the target directory exists (e.g. first save after init into a fresh configPath).
    if let Some(parent) = settings_path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    let content = if settings_path.exists() {
        match fs::read_to_string(settings_path) {
            Ok(c) => c,
            Err(_) => return Status::InternalServerError,
        }
    } else {
        String::new()
    };

    let mut doc: DocumentMut = if content.trim().is_empty() {
        DocumentMut::new()
    } else {
        match content.parse() {
            Ok(d) => d,
            Err(_) => return Status::InternalServerError,
        }
    };

    // Ensure [services] table exists
    if !doc.contains_key("services") {
        doc.insert("services", Item::Table(Table::new()));
    }
    let services_table = match doc.get_mut("services").and_then(|s| s.as_table_mut()) {
        Some(t) => t,
        None => return Status::InternalServerError,
    };

    // Remove previous [services.<service>] entirely (clean slate for this service's overrides)
    services_table.remove(service);

    // If payload has no keys (or not an object), we just removed -> done
    let payload_map = match payload.as_object() {
        Some(m) if !m.is_empty() => m,
        _ => {
            // write back (may have removed the section)
            if let Err(_) = fs::write(settings_path, doc.to_string()) {
                return Status::InternalServerError;
            }
            broadcast_action_bar(&config);
            return Status::Ok;
        }
    };

    // Build new table for the service, handling dotted keys (e.g. "vpn.enabled", "foo.bar.baz")
    let mut svc_table = Table::new();
    for (k, v) in payload_map.iter() {
        if k.contains('.') {
            // Dotted names are always terminal leaf fields (scalar / list / etc.) in the UI schema
            if let Some(tval) = json_to_toml_value(v) {
                insert_dotted(&mut svc_table, k, tval);
            }
        } else if let Some(titem) = json_to_toml_item(v) {
            svc_table.insert(k, titem);
        }
    }

    if !svc_table.is_empty() {
        services_table.insert(service, Item::Table(svc_table));
    }

    if let Err(_) = fs::write(settings_path, doc.to_string()) {
        return Status::InternalServerError;
    }

    let ev = config.evaluator.clone();
    tokio::spawn(async move {
        let mut g = ev.lock().await;
        let _ = g.refresh().await;
    });
    broadcast_action_bar(&config);

    Status::Ok
}

#[post("/save-core/<section>", data = "<payload>")]
pub fn save_core_section(
    config: &State<Arc<AppConfig>>,
    section: &str,
    payload: Json<serde_json::Value>,
) -> Status {
    if section.trim().is_empty() {
        return Status::BadRequest;
    }
    let settings_path = &config.settings_path;
    if let Some(parent) = settings_path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let content = if settings_path.exists() {
        match fs::read_to_string(settings_path) {
            Ok(c) => c,
            Err(_) => return Status::InternalServerError,
        }
    } else {
        String::new()
    };
    let mut doc: DocumentMut = if content.trim().is_empty() {
        DocumentMut::new()
    } else {
        match content.parse() {
            Ok(d) => d,
            Err(_) => return Status::InternalServerError,
        }
    };
    let core_sections = [
        "ssh",
        "volumes",
        "timeZone",
        "uid",
        "gid",
        "hostname",
        "hashedLinuxPassword",
        "core",
    ];
    let is_core = core_sections.contains(&section);
    // Remove possible old top-level location (for renames/migrations).
    // For the aggregate "core" we merge deltas instead of replacing/removing.
    if section != "core" {
        doc.remove(section);
    }
    if is_core {
        // Ensure [core] table
        if !doc.contains_key("core") || !doc.get("core").map_or(false, |c| c.is_table()) {
            doc.insert("core", Item::Table(Table::new()));
        }
        let core_table = doc.get_mut("core").unwrap().as_table_mut().unwrap();
        if section != "core" {
            core_table.remove(section);
        }
        let payload_map = match payload.as_object() {
            Some(m) if !m.is_empty() => m,
            _ => {
                if section == "core" {
                    // No deltas sent for aggregate core (e.g. all at defaults). Leave existing [core] intact.
                    // If we just ensured an empty one, clean it up.
                    if let Some(t) = doc.get("core").and_then(|c| c.as_table()) {
                        if t.is_empty() {
                            doc.remove("core");
                        }
                    }
                    if let Err(_) = fs::write(settings_path, doc.to_string()) {
                        return Status::InternalServerError;
                    }
                    let ev = config.evaluator.clone();
                    tokio::spawn(async move {
                        let mut g = ev.lock().await;
                        let _ = g.refresh().await;
                    });
                    broadcast_action_bar(&config);
                    return Status::Ok;
                }
                core_table.remove(section);
                if let Err(_) = fs::write(settings_path, doc.to_string()) {
                    return Status::InternalServerError;
                }
                let ev = config.evaluator.clone();
                tokio::spawn(async move {
                    let mut g = ev.lock().await;
                    let _ = g.refresh().await;
                });
                broadcast_action_bar(&config);
                return Status::Ok;
            }
        };
        // Scalars under core (timeZone, uid, gid, hostname, hashedLinuxPassword) are values under [core]
        // not bare at root. The extract renames the single field name to the section for them.
        // The aggregate "core" (for the cleaned grid) merges all its fields (scalars + dotted subs) without clearing siblings.
        if payload_map.len() == 1 && payload_map.contains_key(section) {
            if let Some(v) = payload_map.get(section) {
                if let Some(tval) = json_to_toml_value(v) {
                    if section != "core" {
                        core_table.insert(section, Item::Value(tval));
                    }
                }
            }
        } else {
            let mut tbl = Table::new();
            for (k, v) in payload_map.iter() {
                if k.contains('.') {
                    if let Some(tval) = json_to_toml_value(v) {
                        insert_dotted(&mut tbl, k, tval);
                    }
                } else if let Some(titem) = json_to_toml_item(v) {
                    tbl.insert(k, titem);
                }
            }
            if !tbl.is_empty() {
                if section == "core" {
                    // Merge deltas (scalars + dotted sub keys like "volumes.root", "ssh.authorizedKeys")
                    // directly into the core table. This supports editing all core options in one pane
                    // without losing unsent (default) siblings.
                    for (k, item) in tbl.iter() {
                        core_table.insert(k, item.clone());
                    }
                } else {
                    core_table.insert(section, Item::Table(tbl));
                }
            }
        }
    } else {
        // Top-level sections: neo-service, neo-cli, disko (and the aggregate "core" is handled in is_core branch)
        let payload_map = match payload.as_object() {
            Some(m) if !m.is_empty() => m,
            _ => {
                if let Err(_) = fs::write(settings_path, doc.to_string()) {
                    return Status::InternalServerError;
                }
                let ev = config.evaluator.clone();
                tokio::spawn(async move {
                    let mut g = ev.lock().await;
                    let _ = g.refresh().await;
                });
                broadcast_action_bar(&config);
                return Status::Ok;
            }
        };
        let mut tbl = Table::new();
        for (k, v) in payload_map.iter() {
            if k.contains('.') {
                if let Some(tval) = json_to_toml_value(v) {
                    insert_dotted(&mut tbl, k, tval);
                }
            } else if let Some(titem) = json_to_toml_item(v) {
                tbl.insert(k, titem);
            }
        }
        if !tbl.is_empty() {
            doc.insert(section, Item::Table(tbl));
        }
    }
    if let Err(_) = fs::write(settings_path, doc.to_string()) {
        return Status::InternalServerError;
    }

    let ev = config.evaluator.clone();
    tokio::spawn(async move {
        let mut g = ev.lock().await;
        let _ = g.refresh().await;
    });
    broadcast_action_bar(&config);

    Status::Ok
}

/// Compact signature of action-bar state so the watcher only pushes on real changes.
fn action_bar_signature(config: &AppConfig) -> String {
    activation::gc_old_activations();
    let busy = config.eval_busy.load(Ordering::Relaxed);
    let act = activation::find_recent_in_progress_activation().unwrap_or_default();
    let upd = activation::find_recent_in_progress_update().unwrap_or_default();
    let dirty = settings_toml_has_diff(config) || worktree_changed_and_summary(config).0;
    format!("{busy}|{act}|{upd}|{dirty}")
}

fn render_nix_busy_html(config: &AppConfig) -> String {
    if config.eval_busy.load(Ordering::Relaxed) {
        r#"<span class="inline-flex items-center gap-1 text-[10px] text-info opacity-90" title="Nix evaluator working"><span class="loading loading-spinner loading-xs"></span><span class="hidden sm:inline">eval</span></span>"#.to_string()
    } else {
        String::new()
    }
}

fn render_pending_changes_html(config: &AppConfig) -> String {
    activation::gc_old_activations();
    if let Some(id) = activation::find_recent_in_progress_activation() {
        return format!(
            "<button class=\"btn btn-warning btn-xs animate-pulse\" onclick=\"var m=document.getElementById('changes-modal');m.querySelector('h3').textContent='Activation progress';m.showModal();htmx.ajax('GET','/activation/monitor/{}',{{target:'#changes-body',swap:'innerHTML'}})\">Activation — view</button>",
            id
        );
    }
    if let Some(id) = activation::find_recent_in_progress_update() {
        return format!(
            "<button class=\"btn btn-info btn-xs animate-pulse\" onclick=\"var m=document.getElementById('changes-modal');m.querySelector('h3').textContent='Update progress';m.showModal();htmx.ajax('GET','/update/monitor/{}',{{target:'#changes-body',swap:'innerHTML'}})\">Update — view</button>",
            id
        );
    }
    let (changed, _) = worktree_changed_and_summary(config);
    if changed {
        "<button class=\"btn btn-warning btn-xs\" onclick=\"var m=document.getElementById('changes-modal');m.querySelector('h3').textContent='Pending changes';m.showModal();htmx.ajax('GET','/changes/summary',{target:'#changes-body',swap:'innerHTML'})\">Changes — review</button>".to_string()
    } else {
        "<span class=\"text-[10px] opacity-40\">clean</span>".to_string()
    }
}

fn render_reset_button_html(config: &AppConfig) -> String {
    let dirty = settings_toml_has_diff(config) || worktree_changed_and_summary(config).0;
    if !dirty {
        return String::new();
    }
    // After-request only opens the modal; action-bar refresh is pushed over WS.
    // Use r## so embedded "#id" attributes do not terminate the raw string.
    r##"<button hx-post="/actions/reset" hx-target="#changes-body" hx-swap="innerHTML" hx-confirm="Reset settings from last applied (/etc/neo)?" hx-on::after-request="var m=document.getElementById('changes-modal');if(m){m.querySelector('h3').textContent='Reset';m.showModal();}" class="btn btn-xs btn-ghost">↩<span class="hidden sm:inline ml-1">Reset</span></button>"##.to_string()
}

/// Inner HTML of `#action-bar-dynamic` (appearing middle section: busy, pending, reset).
fn render_action_bar_dynamic_inner(config: &AppConfig) -> String {
    format!(
        r#"{}{}{}"#,
        render_nix_busy_html(config),
        render_pending_changes_html(config),
        render_reset_button_html(config),
    )
}

/// Full OOB fragment for the action bar middle section (htmx ws extension applies it).
fn action_bar_oob_fragment(config: &AppConfig) -> String {
    format!(
        r#"<div id="action-bar-dynamic" class="flex items-center gap-2" hx-swap-oob="true">{}</div>"#,
        render_action_bar_dynamic_inner(config)
    )
}

fn broadcast_action_bar(config: &AppConfig) {
    let _ = config.unit_updates.send(action_bar_oob_fragment(config));
}

/// Background task: detect action-bar state changes and push OOB HTML to WS clients.
pub fn start_action_bar_watcher(config: Arc<AppConfig>) {
    tokio::spawn(async move {
        let mut last = String::new();
        loop {
            // Cheap loop: busy is atomic; git/activation checked each tick. Only send on change.
            let sig = action_bar_signature(&config);
            if sig != last {
                last = sig;
                broadcast_action_bar(&config);
            }
            tokio::time::sleep(Duration::from_millis(750)).await;
        }
    });
}

#[get("/changes/action-bar")]
pub fn changes_action_bar(config: &State<Arc<AppConfig>>) -> RawHtml<String> {
    RawHtml(format!(
        r#"<div id="action-bar-dynamic" class="flex items-center gap-2">{}</div>"#,
        render_action_bar_dynamic_inner(&config)
    ))
}

#[get("/changes/indicator")]
pub fn changes_indicator(config: &State<Arc<AppConfig>>) -> RawHtml<String> {
    RawHtml(render_pending_changes_html(&config))
}

#[get("/changes/reset-button")]
pub fn reset_button(config: &State<Arc<AppConfig>>) -> RawHtml<String> {
    RawHtml(render_reset_button_html(&config))
}

#[get("/changes/summary")]
pub fn changes_summary(config: &State<Arc<AppConfig>>) -> RawHtml<String> {
    let body = if settings_toml_has_diff(&config) {
        let diff = get_settings_toml_diff(&config);
        let esc = diff
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;");
        format!("<div class=\"mb-2 text-warning text-sm\">Pending changes to settings.toml (git diff)</div><pre class=\"text-xs overflow-auto max-h-[50vh] bg-base-300 p-2 rounded whitespace-pre\">{}</pre><div class=\"flex gap-2 mt-3\"><button hx-post=\"/changes/revert\" hx-target=\"#changes-body\" hx-swap=\"innerHTML\" class=\"btn btn-sm btn-ghost\">Revert (paste-settings)</button><button hx-post=\"/changes/apply\" hx-target=\"#changes-body\" hx-swap=\"innerHTML\" hx-confirm=\"Run full activation (write-flake + nixos-rebuild)? This can take several minutes.\" class=\"btn btn-sm btn-error\">Apply (activate)</button></div>", esc)
    } else {
        let (changed, summary) = worktree_changed_and_summary(&config);
        if changed {
            let esc = summary
                .replace('&', "&amp;")
                .replace('<', "&lt;")
                .replace('>', "&gt;");
            format!("<div class=\"mb-2 text-warning text-sm\">Other files changed in working tree</div><pre class=\"text-xs overflow-auto max-h-[50vh] bg-base-300 p-2 rounded whitespace-pre\">{}</pre><div class=\"flex gap-2 mt-3\"><button hx-post=\"/changes/revert\" hx-target=\"#changes-body\" hx-swap=\"innerHTML\" class=\"btn btn-sm btn-ghost\">Revert (paste-settings)</button><button hx-post=\"/changes/apply\" hx-target=\"#changes-body\" hx-swap=\"innerHTML\" hx-confirm=\"Run full activation (write-flake + nixos-rebuild)? This can take several minutes.\" class=\"btn btn-sm btn-error\">Apply (activate)</button></div>", esc)
        } else {
            "<div class=\"text-sm\">Working tree clean. No pending changes.</div>".to_string()
        }
    };
    RawHtml(body)
}

#[post("/changes/revert")]
pub fn revert_settings(config: &State<Arc<AppConfig>>) -> RawHtml<String> {
    let dir = config_dir(&config);
    let dir_str = dir.to_str().unwrap_or(".");
    let source = PathBuf::from("/etc/neo/settings.toml");
    let dummy = DocumentMut::new();
    let res = paste_settings(dir_str, &source, &dummy, false, &config.nix_cmd);
    if res.is_ok() {
        let ev = config.evaluator.clone();
        tokio::spawn(async move {
            let mut g = ev.lock().await;
            let _ = g.refresh().await;
        });
        broadcast_action_bar(&config);
    }
    match res {
        Ok(()) => RawHtml("<div class=\"alert alert-success text-sm\">Reverted via paste-settings. Close and reload options to see state.</div><div class=\"mt-2\"><button onclick=\"document.getElementById('changes-modal').close()\" class=\"btn btn-sm\">Close</button></div>".to_string()),
        Err(e) => RawHtml(format!("<div class=\"alert alert-error text-sm\">Revert failed: {}</div>", e))
    }
}

#[post("/changes/apply")]
pub fn apply_settings(config: &State<Arc<AppConfig>>) -> RawHtml<String> {
    let html = trigger_activation(&config);
    broadcast_action_bar(&config);
    html
}

#[post("/flake/update")]
pub fn flake_update(config: &State<Arc<AppConfig>>) -> RawHtml<String> {
    activation::gc_old_activations();
    if let Some(id) = activation::find_recent_in_progress_activation() {
        return RawHtml(format!(
            "<div class=\"alert alert-info text-sm\">Activation {} in progress — cannot update</div>",
            id
        ));
    }
    if let Some(id) = activation::find_recent_in_progress_update() {
        return RawHtml(format!(
            "<div class=\"alert alert-info text-sm\">Update {} already in progress</div>",
            id
        ));
    }
    let ts = crate::commands::get_timestamp();
    let op = OperationLog::new_update(&ts);
    op.init_for_web_trigger(&ts);
    trigger_systemd_run("update", "NEO_UPDATE_SUFFIX", op.suffix(), op.log_path());
    broadcast_action_bar(&config);
    RawHtml(activation::build_update_monitor_fragment(op.id()))
}

#[post("/actions/activate")]
pub fn actions_activate(config: &State<Arc<AppConfig>>) -> RawHtml<String> {
    let html = trigger_activation(&config);
    broadcast_action_bar(&config);
    html
}

#[post("/actions/reset")]
pub fn actions_reset(config: &State<Arc<AppConfig>>) -> RawHtml<String> {
    let dir = config_dir(&config);
    let dir_str = dir.to_str().unwrap_or(".");
    let source = PathBuf::from("/etc/neo/settings.toml");
    let dummy = DocumentMut::new();
    let res = paste_settings(dir_str, &source, &dummy, false, &config.nix_cmd);
    if res.is_ok() {
        let ev = config.evaluator.clone();
        tokio::spawn(async move {
            let mut g = ev.lock().await;
            let _ = g.refresh().await;
        });
        broadcast_action_bar(&config);
    }
    match res {
        Ok(()) => RawHtml("<div class=\"alert alert-success text-sm\">Reset done (settings restored from /etc/neo). Close to refresh state.</div><div class=\"mt-2\"><button onclick=\"document.getElementById('changes-modal').close()\" class=\"btn btn-sm\">Close</button></div>".to_string()),
        Err(e) => RawHtml(format!("<div class=\"alert alert-error text-sm\">Reset failed: {}</div>", e))
    }
}

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

#[get("/activation/monitor/<id>")]
pub fn activation_monitor(id: &str) -> RawHtml<String> {
    RawHtml(activation::build_monitor_fragment(id))
}

#[get("/activation/log/<id>")]
pub fn activation_log(id: &str) -> RawHtml<String> {
    RawHtml(activation::build_log_fragment(id))
}

#[get("/activation/status/<id>")]
pub fn activation_status(id: &str) -> RawHtml<String> {
    RawHtml(activation::build_status_fragment(id))
}

#[get("/activation/current")]
pub fn activation_current() -> RawHtml<String> {
    if let Some(id) = activation::find_recent_in_progress_activation() {
        RawHtml(activation::build_monitor_fragment(&id))
    } else {
        RawHtml("<div class=\"text-xs\">no active activation</div>".to_string())
    }
}

#[get("/update/monitor/<id>")]
pub fn update_monitor(id: &str) -> RawHtml<String> {
    RawHtml(activation::build_update_monitor_fragment(id))
}

#[get("/update/log/<id>")]
pub fn update_log(id: &str) -> RawHtml<String> {
    RawHtml(activation::build_log_fragment(id))
}

#[get("/update/status/<id>")]
pub fn update_status(id: &str) -> RawHtml<String> {
    RawHtml(activation::build_update_status_fragment(id))
}

fn sudo_cmd() -> String {
    std::env::var("SUDO_BINARY_PATH").unwrap_or_else(|_| "sudo".to_string())
}

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn unit_name_valid(unit: &str) -> bool {
    !unit.is_empty()
        && unit.len() <= 256
        && unit
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-@._".contains(c))
}

/// Query systemctl is-active for a unit (sync; used from HTTP handlers and render).
fn unit_active_state(unit: &str) -> String {
    let sudo = sudo_cmd();
    Command::new(&sudo)
        .args(["systemctl", "is-active", unit])
        .output()
        .map(|o| {
            let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
            if s.is_empty() {
                "unknown".into()
            } else {
                s
            }
        })
        .unwrap_or_else(|_| "unknown".into())
}

async fn unit_active_state_async(unit: &str) -> String {
    let sudo = sudo_cmd();
    match AsyncCommand::new(&sudo)
        .args(["systemctl", "is-active", unit])
        .output()
        .await
    {
        Ok(o) => {
            let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
            if s.is_empty() {
                "unknown".into()
            } else {
                s
            }
        }
        Err(_) => "unknown".into(),
    }
}

/// Build the inner content (dot + state + buttons) for a unit controls area.
/// Used for OOB WS pushes and composed into full divs.
///
/// Buttons stay stable across transitional states so restart/stop never "vanish"
/// while systemctl --no-block is still settling (the live WS watcher re-renders
/// as soon as ActiveState changes).
fn render_unit_controls_content_with_state(unit: &str, active: &str) -> String {
    let is_container = unit.starts_with("docker-");

    let dot_cls = match active {
        "active" => "bg-success",
        "inactive" => "bg-base-300",
        "activating" | "deactivating" | "reloading" => "bg-info animate-pulse",
        "failed" => "bg-error",
        _ => "bg-warning",
    };

    let u = escape_html(unit);
    let state_label = escape_html(active);
    // Basic JS string escape for onclick arg (single quotes in unit names are rare for units)
    let u_js = u.replace('\'', "\\'");

    let mut inner = String::new();
    inner.push_str(&format!(
        r#"<span class="inline-block w-2 h-2 rounded-full flex-shrink-0 {}" title="{}"></span>"#,
        dot_cls, u
    ));
    inner.push_str(&format!(
        r#"<span class="text-[10px] opacity-60 font-mono min-w-[4.5rem]" title="ActiveState">{}</span>"#,
        state_label
    ));

    // Stable control set: inactive/failed → start; anything running/transitional → stop+restart.
    // failed also keeps restart so a retry is one click.
    match active {
        "inactive" => {
            inner.push_str(&format!(
                r##"<button class="btn btn-ghost btn-xs h-5 min-h-0 px-1.5" hx-post="/unit/start/{u}" hx-swap="none" title="systemctl start">▶</button>"##,
                u = u
            ));
        }
        "failed" => {
            inner.push_str(&format!(
                r##"<button class="btn btn-ghost btn-xs h-5 min-h-0 px-1.5" hx-post="/unit/start/{u}" hx-swap="none" title="systemctl start">▶</button>"##,
                u = u
            ));
            inner.push_str(&format!(
                r##"<button class="btn btn-ghost btn-xs h-5 min-h-0 px-1.5" hx-post="/unit/restart/{u}" hx-swap="none" title="systemctl restart">⟳</button>"##,
                u = u
            ));
        }
        _ => {
            // active | activating | deactivating | reloading | unknown
            inner.push_str(&format!(
                r##"<button class="btn btn-ghost btn-xs h-5 min-h-0 px-1.5" hx-post="/unit/stop/{u}" hx-swap="none" title="systemctl stop">⏹</button>"##,
                u = u
            ));
            inner.push_str(&format!(
                r##"<button class="btn btn-ghost btn-xs h-5 min-h-0 px-1.5" hx-post="/unit/restart/{u}" hx-swap="none" title="systemctl restart">⟳</button>"##,
                u = u
            ));
        }
    }

    // logs always opens dialog (live via SSE)
    inner.push_str(&format!(
        r#"<button class="btn btn-ghost btn-xs h-5 min-h-0 px-1.5" onclick="openUnitLogs('{}')" title="open live logs dialog (infinitely scrollable)">logs</button>"#,
        u_js
    ));

    if is_container {
        inner.push_str(&format!(
            r##"<button class="btn btn-accent btn-xs h-5 min-h-0 px-1.5" hx-post="/container/update/{u}" hx-target="closest .unit-row .update-out-inline" hx-swap="innerHTML" title="docker pull (current running image) + restart">↻</button>"##,
            u = u
        ));
    }

    inner
}

/// Full unit-controls div (with id) for bootstrap GET.
fn render_unit_controls(unit: &str) -> RawHtml<String> {
    let active = unit_active_state(unit);
    let content = render_unit_controls_content_with_state(unit, &active);
    let u = escape_html(unit);
    RawHtml(format!(
        r#"<div id="unit-controls-{u}" class="unit-controls flex items-center gap-1 flex-shrink-0" data-active-state="{}">{content}</div>"#,
        escape_html(&active)
    ))
}

/// OOB fragment for htmx ws (and action HTTP responses).
fn unit_controls_oob_fragment(unit: &str) -> String {
    let active = unit_active_state(unit);
    unit_controls_oob_fragment_with_state(unit, &active)
}

fn unit_controls_oob_fragment_with_state(unit: &str, active: &str) -> String {
    format!(
        r#"<div id="unit-controls-{}" class="unit-controls flex items-center gap-1 flex-shrink-0" data-active-state="{}" hx-swap-oob="true">{}</div>"#,
        escape_html(unit),
        escape_html(active),
        render_unit_controls_content_with_state(unit, active)
    )
}

/// Broadcast an OOB swap fragment for a unit's controls to all connected WS clients.
fn broadcast_unit_update(unit: &str, config: &AppConfig) {
    let _ = config.unit_updates.send(unit_controls_oob_fragment(unit));
}

/// After a non-blocking systemctl action, ActiveState may lag for a few seconds.
/// Push a short burst of refreshes so the UI settles without waiting for the next
/// watcher tick alone (and even if the pane only did a one-shot HTTP OOB).
fn schedule_unit_refresh_burst(unit: String, config: Arc<AppConfig>) {
    if !unit_name_valid(&unit) {
        return;
    }
    tokio::spawn(async move {
        for delay_ms in [150_u64, 400, 900, 1800, 3500] {
            tokio::time::sleep(Duration::from_millis(delay_ms)).await;
            broadcast_unit_update(&unit, &config);
        }
    });
}

#[get("/unit/status/<unit>")]
pub fn unit_status(unit: &str) -> RawHtml<String> {
    if !unit_name_valid(unit) {
        return RawHtml(
            r#"<div class="unit-controls text-[10px] text-error">invalid unit</div>"#.into(),
        );
    }
    render_unit_controls(unit)
}

#[get("/unit/logs/<unit>")]
pub fn unit_logs(unit: &str) -> RawHtml<String> {
    let sudo = sudo_cmd();
    let out = Command::new(&sudo)
        .args([
            "journalctl",
            "-u",
            unit,
            "--no-pager",
            "-n",
            "100",
            "-o",
            "short-iso",
        ])
        .output();
    let text = match out {
        Ok(o) => {
            let mut t = String::from_utf8_lossy(&o.stdout).to_string();
            if !o.stderr.is_empty() {
                t.push_str("\n[stderr]\n");
                t.push_str(&String::from_utf8_lossy(&o.stderr));
            }
            t
        }
        Err(e) => format!("journalctl error: {}", e),
    };
    RawHtml(format!(
        r#"<pre class="text-[10px] bg-base-300 p-1 mt-1 max-h-64 overflow-auto font-mono whitespace-pre-wrap">{}</pre>"#,
        escape_html(&text)
    ))
}

fn perform_unit_action(action: &str, unit: &str) {
    if !unit_name_valid(unit) {
        return;
    }
    let sudo = sudo_cmd();
    let _ = Command::new(&sudo)
        .args(["systemctl", action, unit, "--no-block", "--no-ask-password"])
        .status();
}

/// Shared post-action path: kick systemctl, push OOB once, then burst-refresh while it settles.
/// Buttons use hx-swap="none"; the returned OOB still updates the controls row.
fn unit_action_response(action: &str, unit: &str, config: &State<Arc<AppConfig>>) -> RawHtml<String> {
    if !unit_name_valid(unit) {
        return RawHtml(String::new());
    }
    perform_unit_action(action, unit);
    let oob = unit_controls_oob_fragment(unit);
    let _ = config.unit_updates.send(oob.clone());
    schedule_unit_refresh_burst(unit.to_string(), Arc::clone(config));
    RawHtml(oob)
}

#[post("/unit/restart/<unit>")]
pub fn unit_restart(unit: &str, config: &State<Arc<AppConfig>>) -> RawHtml<String> {
    unit_action_response("restart", unit, config)
}

#[post("/unit/start/<unit>")]
pub fn unit_start(unit: &str, config: &State<Arc<AppConfig>>) -> RawHtml<String> {
    unit_action_response("start", unit, config)
}

#[post("/unit/stop/<unit>")]
pub fn unit_stop(unit: &str, config: &State<Arc<AppConfig>>) -> RawHtml<String> {
    unit_action_response("stop", unit, config)
}

#[post("/container/update/<container>")]
pub fn container_update(container: &str, config: &State<Arc<AppConfig>>) -> RawHtml<String> {
    // Normalize: accept "foo" or "docker-foo"; use bare name for inspect/restart
    let cname = if container.starts_with("docker-") {
        &container[7..]
    } else {
        container
    };
    // Inspect current image ref from the running container (works for :latest and pinned)
    let inspect = Command::new("docker")
        .args(["inspect", "--format", "{{.Config.Image}}", cname])
        .output();
    let img = match inspect {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).trim().to_string(),
        Ok(o) => {
            return RawHtml(format!(
                r#"<span class="text-error text-xs">inspect failed: {}</span>"#,
                escape_html(&String::from_utf8_lossy(&o.stderr))
            ))
        }
        Err(e) => {
            return RawHtml(format!(
                r#"<span class="text-error text-xs">docker error: {}</span>"#,
                e
            ))
        }
    };
    if img.is_empty() {
        return RawHtml(r#"<span class="text-error text-xs">no image from inspect</span>"#.into());
    }
    let pull = Command::new("docker").args(["pull", &img]).output();
    let pull_out = match pull {
        Ok(o) => String::from_utf8_lossy(&o.stdout).to_string(),
        Err(e) => format!("pull error: {}", e),
    };
    // Restart via sudo to let the unit manage it (use docker- prefix for unit)
    let sudo = sudo_cmd();
    let _ = Command::new(&sudo)
        .args([
            "systemctl",
            "restart",
            &format!("docker-{}", cname),
            "--no-block",
            "--no-ask-password",
        ])
        .status();
    // Live unit-control updates over WS (burst while restart settles).
    let unit_for_watch = if container.starts_with("docker-") {
        container.to_string()
    } else {
        format!("docker-{}", cname)
    };
    broadcast_unit_update(&unit_for_watch, &config);
    schedule_unit_refresh_burst(unit_for_watch, Arc::clone(config));
    RawHtml(format!(
        r#"<div class="text-xs"><div>pulled: {}</div><pre class="text-[9px] max-h-32 overflow-auto">{}</pre><div class="text-success">restarted docker-{}</div></div>"#,
        escape_html(&img),
        escape_html(&pull_out),
        escape_html(cname)
    ))
}

/// SSE endpoint for live journalctl follow in the logs dialog.
/// Client uses native EventSource; first ~100 lines + subsequent live appends.
#[get("/sse/logs/<unit>")]
pub async fn sse_logs(unit: &str) -> EventStream![] {
    let unit = unit.to_string();
    EventStream! {
        let valid = unit.chars().all(|c| c.is_alphanumeric() || "-@._".contains(c));
        if !valid {
            yield Event::data("invalid unit name for logs");
        }
        if valid {
            let sudo = sudo_cmd();
            let spawn_res = AsyncCommand::new(&sudo)
                .args([
                    "journalctl",
                    "-u",
                    &unit,
                    "-n",
                    "100",
                    "-f",
                    "--no-pager",
                    "-o",
                    "short-iso",
                ])
                .kill_on_drop(true)
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn();
            let mut child_opt = match spawn_res {
                Ok(c) => Some(c),
                Err(e) => {
                    yield Event::data(format!("spawn error: {}", e));
                    None
                }
            };
            if let Some(mut child) = child_opt {
                let stdout = child.stdout.take().expect("piped stdout");
                let mut lines = AsyncBufReader::new(stdout).lines();

                loop {
                    match lines.next_line().await {
                        Ok(Some(line)) => {
                            yield Event::data(escape_html(&line));
                        }
                        Ok(None) => break,
                        Err(e) => {
                            yield Event::data(format!("[read err] {}", e));
                            break;
                        }
                    }
                }
                // child auto-killed by kill_on_drop on drop
            }
        }
    }
}

/// Parse a client WS control message.
/// Supported forms:
///   {"op":"watch","units":["docker-foo","bar"]}
///   {"op":"unwatch","units":[...]}
///   {"op":"watch_replace","units":[...]}  // drop previous interest, watch only these
/// Unknown / non-JSON messages are ignored (htmx may send form-shaped JSON).
fn parse_ws_unit_command(text: &str) -> Option<(String, Vec<String>)> {
    let v: serde_json::Value = serde_json::from_str(text).ok()?;
    let op = v.get("op")?.as_str()?.to_string();
    if op != "watch" && op != "unwatch" && op != "watch_replace" {
        return None;
    }
    let units = v
        .get("units")?
        .as_array()?
        .iter()
        .filter_map(|u| u.as_str().map(|s| s.to_string()))
        .filter(|u| unit_name_valid(u))
        .collect::<Vec<_>>();
    Some((op, units))
}

/// WebSocket endpoint for htmx ws extension (hx-ext="ws" ws-connect="/ws/status").
///
/// - Forwards broadcast OOB fragments (action bar + unit control bursts from actions).
/// - Accepts client `watch` / `unwatch` / `watch_replace` messages listing systemd units;
///   while those units are watched *and this socket is open*, a per-connection poller
///   re-checks ActiveState (~500ms) and pushes OOB HTML only when it changes.
/// - Survives broadcast lag (skips) so a busy action-bar channel cannot kill the socket.
#[get("/ws/status")]
pub async fn ws_status(ws: WebSocket, config: &State<Arc<AppConfig>>) -> Channel<'static> {
    let mut rx = config.unit_updates.subscribe();
    let initial_bar = action_bar_oob_fragment(config);
    ws.channel(move |mut stream| {
        Box::pin(async move {
            use rocket::futures::{SinkExt, StreamExt};
            use std::collections::{HashMap, HashSet};

            // Immediate action-bar snapshot so the navbar is correct before the watcher ticks.
            if stream
                .send(Message::Text(initial_bar.into()))
                .await
                .is_err()
            {
                return Ok(());
            }

            let mut watched: HashSet<String> = HashSet::new();
            // unit -> last ActiveState string we pushed (skip identical re-renders)
            let mut last_state: HashMap<String, String> = HashMap::new();
            let mut tick = tokio::time::interval(Duration::from_millis(500));
            tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            // Don't fire immediately; first poll after 500ms (bootstrap GET already filled UI).
            tick.tick().await;

            loop {
                tokio::select! {
                    client_msg = stream.next() => {
                        match client_msg {
                            Some(Ok(Message::Text(text))) => {
                                if let Some((op, units)) = parse_ws_unit_command(&text) {
                                    match op.as_str() {
                                        "watch_replace" => {
                                            watched.clear();
                                            last_state.clear();
                                            for u in units {
                                                watched.insert(u);
                                            }
                                        }
                                        "watch" => {
                                            for u in units {
                                                watched.insert(u);
                                            }
                                        }
                                        "unwatch" => {
                                            for u in &units {
                                                watched.remove(u);
                                                last_state.remove(u);
                                            }
                                        }
                                        _ => {}
                                    }
                                    // Immediate snapshot for newly watched units so the pane
                                    // does not wait a full tick after open/reconnect.
                                    for u in watched.iter().cloned().collect::<Vec<_>>() {
                                        let active = unit_active_state_async(&u).await;
                                        let prev = last_state.get(&u);
                                        if prev.map(|p| p.as_str()) != Some(active.as_str()) {
                                            last_state.insert(u.clone(), active.clone());
                                            let frag =
                                                unit_controls_oob_fragment_with_state(&u, &active);
                                            if stream
                                                .send(Message::Text(frag.into()))
                                                .await
                                                .is_err()
                                            {
                                                return Ok(());
                                            }
                                        }
                                    }
                                }
                            }
                            Some(Ok(Message::Close(_))) | Some(Err(_)) | None => break,
                            Some(Ok(_)) => { /* ping/pong/binary — ignore */ }
                        }
                    }
                    _ = tick.tick(), if !watched.is_empty() => {
                        // Live poll only for units this browser pane registered.
                        for u in watched.iter().cloned().collect::<Vec<_>>() {
                            let active = unit_active_state_async(&u).await;
                            let changed =
                                last_state.get(&u).map(|p| p.as_str()) != Some(active.as_str());
                            if changed {
                                last_state.insert(u.clone(), active.clone());
                                let frag = unit_controls_oob_fragment_with_state(&u, &active);
                                if stream.send(Message::Text(frag.into())).await.is_err() {
                                    return Ok(());
                                }
                            }
                        }
                    }
                    update = rx.recv() => {
                        match update {
                            Ok(fragment) => {
                                // Action-bar + burst unit updates from HTTP handlers.
                                // Keep last_state coherent so the poller does not re-send
                                // the same ActiveState right after a broadcast.
                                if let Some((unit, state)) = extract_unit_state_from_oob(&fragment)
                                {
                                    if watched.contains(&unit) {
                                        last_state.insert(unit, state);
                                    }
                                }
                                if stream.send(Message::Text(fragment.into())).await.is_err() {
                                    break;
                                }
                            }
                            // Lagged: drop missed messages and keep the socket alive.
                            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                            Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                        }
                    }
                }
            }
            // Connection closed → watched set drops with this task (no more polling).
            Ok(())
        })
    })
}

/// Best-effort parse of `id="unit-controls-…"` + `data-active-state="…"` from an OOB fragment.
fn extract_unit_state_from_oob(fragment: &str) -> Option<(String, String)> {
    let id_marker = r#"id="unit-controls-"#;
    let state_marker = r#"data-active-state=""#;
    let id_start = fragment.find(id_marker)? + id_marker.len();
    let id_end = fragment[id_start..].find('"')? + id_start;
    let unit = fragment[id_start..id_end].to_string();
    let state_start = fragment.find(state_marker)? + state_marker.len();
    let state_end = fragment[state_start..].find('"')? + state_start;
    let state = fragment[state_start..state_end].to_string();
    if unit_name_valid(&unit) {
        Some((unit, state))
    } else {
        None
    }
}

pub fn routes() -> Vec<rocket::Route> {
    routes![
        index,
        configuration,
        option_pane,
        services_grid,
        save_service,
        save_core_section,
        changes_action_bar,
        changes_indicator,
        reset_button,
        changes_summary,
        revert_settings,
        apply_settings,
        flake_update,
        actions_activate,
        actions_reset,
        branches,
        git_switch,
        core_grid,
        core_pane,
        activation_monitor,
        activation_log,
        activation_status,
        activation_current,
        update_monitor,
        update_log,
        update_status,
        unit_status,
        unit_logs,
        unit_restart,
        unit_start,
        unit_stop,
        container_update,
        sse_logs,
        ws_status,
    ]
}
