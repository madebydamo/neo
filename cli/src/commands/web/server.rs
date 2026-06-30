use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;

use super::structs::{AppConfig, BranchInfo, BranchesContext, IndexContext};
use rocket::response::content::RawHtml;
use rocket::serde::json::Json;
use rocket::{get, http::Status, post, routes, State};
use rocket_dyn_templates::Template;
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
    let (svcs, theme) = {
        let mut ev = config.evaluator.lock().await;
        let svcs = ev.extract_services().await;
        let theme = ev.extract_neo_theme().await;
        (svcs, theme)
    };
    Template::render(
        "configuration",
        IndexContext {
            services: svcs,
            theme,
        },
    )
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
    let svcs = {
        let mut ev = config.evaluator.lock().await;
        ev.extract_services().await
    };
    Template::render(
        "services_grid",
        IndexContext {
            services: svcs,
            ..Default::default()
        },
    )
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

    Status::Ok
}

#[get("/changes/indicator")]
pub fn changes_indicator(config: &State<Arc<AppConfig>>) -> RawHtml<String> {
    activation::gc_old_activations();
    if let Some(id) = activation::find_recent_in_progress_activation() {
        let btn = format!(
            "<button class=\"btn btn-warning btn-xs animate-pulse\" onclick=\"var m=document.getElementById('changes-modal');m.querySelector('h3').textContent='Activation progress';m.showModal();htmx.ajax('GET','/activation/monitor/{}',{{target:'#changes-body',swap:'innerHTML'}})\">Activation — view</button>",
            id
        );
        return RawHtml(btn);
    }
    if let Some(id) = activation::find_recent_in_progress_update() {
        let btn = format!(
            "<button class=\"btn btn-info btn-xs animate-pulse\" onclick=\"var m=document.getElementById('changes-modal');m.querySelector('h3').textContent='Update progress';m.showModal();htmx.ajax('GET','/update/monitor/{}',{{target:'#changes-body',swap:'innerHTML'}})\">Update — view</button>",
            id
        );
        return RawHtml(btn);
    }
    let (changed, _) = worktree_changed_and_summary(&config);
    let content = if changed {
        "<button class=\"btn btn-warning btn-xs\" onclick=\"var m=document.getElementById('changes-modal');m.querySelector('h3').textContent='Pending changes';m.showModal();htmx.ajax('GET','/changes/summary',{target:'#changes-body',swap:'innerHTML'})\">Changes — review</button>"
    } else {
        "<span class=\"text-[10px] opacity-40\">clean</span>"
    };
    RawHtml(content.to_string())
}
#[get("/changes/reset-button")]

pub fn reset_button(config: &State<Arc<AppConfig>>) -> RawHtml<String> {
    activation::gc_old_activations();
    // Only render the reset button when there is actually pending work.
    // This makes the button appear/disappear and prevents useless clicks.
    let has_settings_diff = settings_toml_has_diff(&config);
    let (tree_changed, _) = worktree_changed_and_summary(&config);
    let dirty = has_settings_diff || tree_changed;
    if !dirty {
        return RawHtml(String::new());
    }
    let btn = "<button hx-post=\"/actions/reset\" hx-target=\"#changes-body\" hx-swap=\"innerHTML\" hx-confirm=\"Reset settings from last applied (/etc/neo)?\" hx-on::after-request=\"var m=document.getElementById('changes-modal');if(m){m.querySelector('h3').textContent='Reset';m.showModal();} var i=document.getElementById('pending-changes');if(i)htmx.ajax('GET','/changes/indicator',{target:'#pending-changes',swap:'innerHTML'}); var r=document.getElementById('reset-button-container');if(r)htmx.ajax('GET','/changes/reset-button',{target:'#reset-button-container',swap:'innerHTML'});\" class=\"btn btn-xs btn-ghost\">↩<span class=\"hidden sm:inline ml-1\">Reset</span></button>";
    RawHtml(btn.to_string())
}

#[get("/changes/summary")]
pub fn changes_summary(config: &State<Arc<AppConfig>>) -> RawHtml<String> {
    let body = if settings_toml_has_diff(&config) {
        let diff = get_settings_toml_diff(&config);
        let esc = diff
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;");
        format!("<div class=\"mb-2 text-warning text-sm\">Pending changes to settings.toml (git diff)</div><pre class=\"text-xs overflow-auto max-h-[50vh] bg-base-300 p-2 rounded whitespace-pre\">{}</pre><div class=\"flex gap-2 mt-3\"><button hx-post=\"/changes/revert\" hx-target=\"#changes-body\" hx-swap=\"innerHTML\" class=\"btn btn-sm btn-ghost\">Revert (paste-settings)</button><button hx-post=\"/changes/apply\" hx-target=\"#changes-body\" hx-swap=\"innerHTML\" hx-confirm=\"Run full activation (write-flake + nixos-rebuild)? This can take several minutes.\" hx-on::after-request=\"var i=document.getElementById('pending-changes');if(i)htmx.ajax('GET','/changes/indicator',{{target:'#pending-changes',swap:'innerHTML'}})\" class=\"btn btn-sm btn-error\">Apply (activate)</button></div>", esc)
    } else {
        let (changed, summary) = worktree_changed_and_summary(&config);
        if changed {
            let esc = summary
                .replace('&', "&amp;")
                .replace('<', "&lt;")
                .replace('>', "&gt;");
            format!("<div class=\"mb-2 text-warning text-sm\">Other files changed in working tree</div><pre class=\"text-xs overflow-auto max-h-[50vh] bg-base-300 p-2 rounded whitespace-pre\">{}</pre><div class=\"flex gap-2 mt-3\"><button hx-post=\"/changes/revert\" hx-target=\"#changes-body\" hx-swap=\"innerHTML\" class=\"btn btn-sm btn-ghost\">Revert (paste-settings)</button><button hx-post=\"/changes/apply\" hx-target=\"#changes-body\" hx-swap=\"innerHTML\" hx-confirm=\"Run full activation (write-flake + nixos-rebuild)? This can take several minutes.\" hx-on::after-request=\"var i=document.getElementById('pending-changes');if(i)htmx.ajax('GET','/changes/indicator',{{target:'#pending-changes',swap:'innerHTML'}})\" class=\"btn btn-sm btn-error\">Apply (activate)</button></div>", esc)
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
    }
    match res {
        Ok(()) => RawHtml("<div class=\"alert alert-success text-sm\">Reverted via paste-settings. Close and reload options to see state.</div><div class=\"mt-2\"><button onclick=\"document.getElementById('changes-modal').close()\" class=\"btn btn-sm\">Close</button></div>".to_string()),
        Err(e) => RawHtml(format!("<div class=\"alert alert-error text-sm\">Revert failed: {}</div>", e))
    }
}

#[post("/changes/apply")]
pub fn apply_settings(config: &State<Arc<AppConfig>>) -> RawHtml<String> {
    trigger_activation(&config)
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
    RawHtml(activation::build_update_monitor_fragment(op.id()))
}

#[post("/actions/activate")]
pub fn actions_activate(config: &State<Arc<AppConfig>>) -> RawHtml<String> {
    trigger_activation(&config)
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

pub fn routes() -> Vec<rocket::Route> {
    routes![
        index,
        configuration,
        option_pane,
        services_grid,
        save_service,
        save_core_section,
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
    ]
}
