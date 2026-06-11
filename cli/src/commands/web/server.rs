use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;

use rocket::response::content::RawHtml;
use rocket::serde::json::Json;
use rocket::{get, http::Status, post, routes, State};
use rocket_dyn_templates::Template;
use toml_edit::{DocumentMut, Item, Table, Value};

use super::nix_eval::{
    extract_neo_section, extract_neo_theme, extract_proxied_services, extract_service_options,
    extract_services,
};
use super::structs::{AppConfig, BranchInfo, BranchesContext, IndexContext};

use crate::commands::init::init;
use crate::commands::nuke::nuke;
use crate::commands::paste_settings::paste_settings;
use crate::commands::update::update;
use crate::commands::{get_current_branch, git_cmd};

use super::activation;

#[get("/")]
pub fn index(config: &State<Arc<AppConfig>>) -> Template {
    let mut data = extract_proxied_services(&config.nix_cmd, &config.neo_input);
    data.theme = extract_neo_theme(&config.nix_cmd, &config.neo_input);
    Template::render("index", data)
}

#[get("/configuration")]
pub fn configuration(config: &State<Arc<AppConfig>>) -> Template {
    let svcs = extract_services(&config.nix_cmd, &config.neo_input);
    let theme = extract_neo_theme(&config.nix_cmd, &config.neo_input);
    Template::render(
        "configuration",
        IndexContext {
            services: svcs,
            theme,
        },
    )
}

#[get("/option/<service>")]
pub fn option_pane(config: &State<Arc<AppConfig>>, service: &str) -> Template {
    let pane = extract_service_options(&config.nix_cmd, &config.neo_input, service);
    Template::render("option_pane", pane)
}

#[get("/services-grid")]
pub fn services_grid(config: &State<Arc<AppConfig>>) -> Template {
    let svcs = extract_services(&config.nix_cmd, &config.neo_input);
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

fn trigger_activation(config: &AppConfig) -> RawHtml<String> {
    let dir = config_dir(config);
    let _dir_str = dir.to_str().unwrap_or(".");
    let sudo_cmd = std::env::var("SUDO_BINARY_PATH").unwrap_or_else(|_| "sudo".to_string());
    let ts = crate::commands::get_timestamp();
    let id = format!("activation_{}", ts);
    let act_dir = activation::activation_dir();
    let _ = fs::create_dir_all(&act_dir);
    let state_path = act_dir.join(format!("{}.json", id));
    let log_path = act_dir.join(format!("{}.log", id));
    let initial = serde_json::json!({
        "id": id,
        "status": "in_progress",
        "phase": "triggered",
        "started_at": ts,
        "log_path": log_path.to_string_lossy(),
    });
    let _ = fs::write(
        &state_path,
        serde_json::to_string_pretty(&initial).unwrap_or_default(),
    );
    let _ = fs::write(
        &log_path,
        format!("activation {} triggered via web at {}\n", id, ts),
    );
    if let Some(other) = activation::find_recent_in_progress_activation() {
        if other != id {
            return RawHtml(format!("<div class=\"alert alert-error text-sm\">Another activation {} in progress (or auto-update). Wait.</div>", other));
        }
    }
    let nix_bin = std::env::var("NIX_BINARY_PATH")
        .unwrap_or_else(|_| "/run/current-system/sw/bin/nix".to_string());
    let unit = format!("neo-activate@{}.service", ts);
    let neo_bin = "/run/current-system/sw/bin/neo";
    let desc = format!("{} systemd-run --unit={} (as homeserver)", sudo_cmd, unit);
    let mut run_cmd = Command::new(&sudo_cmd);
    run_cmd.args([
        "systemd-run",
        "--collect",
        "--no-ask-password",
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
        &format!("NEO_ACTIVATION_SUFFIX={}", ts),
        "-E",
        "PATH=/run/current-system/sw/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin",
        "--property",
        &format!("StandardOutput=append:{}", log_path.to_string_lossy()),
        "--property",
        &format!("StandardError=append:{}", log_path.to_string_lossy()),
        "--property",
        &format!("Description=Neo one-shot activation {}", ts),
        neo_bin,
        "activate",
    ]);
    let _ = crate::commands::execute_command(&mut run_cmd, &desc);
    RawHtml(activation::build_monitor_fragment(&id))
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

fn settings_changed_and_diff(cfg: &AppConfig) -> (bool, String) {
    let dir = config_dir(cfg);
    let file = "settings.toml";
    let status = Command::new("git")
        .current_dir(&dir)
        .args(["diff", "--quiet", "--", file])
        .status();
    let changed = status.map(|s| !s.success()).unwrap_or(false);
    if !changed {
        return (false, String::new());
    }
    let output = Command::new("git")
        .current_dir(&dir)
        .args(["diff", "--no-color", "--", file])
        .output();
    let text = match output {
        Ok(o) => {
            let mut t = String::from_utf8_lossy(&o.stdout).into_owned();
            let e = String::from_utf8_lossy(&o.stderr);
            if !e.is_empty() {
                t.push_str(&e);
            }
            t
        }
        Err(e) => format!("git diff error: {}", e),
    };
    (true, text)
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
                    return Status::Ok;
                }
                core_table.remove(section);
                if let Err(_) = fs::write(settings_path, doc.to_string()) {
                    return Status::InternalServerError;
                }
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
    Status::Ok
}

#[get("/changes/indicator")]
pub fn changes_indicator(config: &State<Arc<AppConfig>>) -> RawHtml<String> {
    activation::gc_old_activations();
    if let Some(id) = activation::find_recent_in_progress_activation() {
        let btn = format!(
            "<button class=\"btn btn-warning btn-sm animate-pulse\" onclick=\"document.getElementById('changes-modal').showModal();htmx.ajax('GET','/activation/monitor/{}',{{target:'#changes-body',swap:'innerHTML'}})\">Activation {} in progress — view</button>",
            id, id
        );
        return RawHtml(btn);
    }
    let (changed, _) = settings_changed_and_diff(&config);
    let content = if changed {
        "<button class=\"btn btn-warning btn-sm\" onclick=\"document.getElementById('changes-modal').showModal();htmx.ajax('GET','/changes/summary',{target:'#changes-body',swap:'innerHTML'})\">Settings changed — review &amp; apply</button>"
    } else {
        "<span class=\"text-xs opacity-50\">Settings in sync with applied</span>"
    };
    RawHtml(content.to_string())
}

#[get("/changes/summary")]
pub fn changes_summary(config: &State<Arc<AppConfig>>) -> RawHtml<String> {
    let (changed, diff) = settings_changed_and_diff(&config);
    let body = if changed {
        let esc = diff
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;");
        format!("<div class=\"mb-2 text-warning text-sm\">Pending changes to settings.toml (git diff)</div><pre class=\"text-xs overflow-auto max-h-[50vh] bg-base-300 p-2 rounded whitespace-pre\">{}</pre><div class=\"flex gap-2 mt-3\"><button hx-post=\"/changes/revert\" hx-target=\"#changes-body\" hx-swap=\"innerHTML\" class=\"btn btn-sm btn-ghost\">Revert (paste-settings)</button><button hx-post=\"/changes/apply\" hx-target=\"#changes-body\" hx-swap=\"innerHTML\" hx-confirm=\"Run full activation (write-flake + nixos-rebuild)? This can take several minutes.\" hx-on::after-request=\"var i=document.getElementById('pending-changes');if(i)htmx.ajax('GET','/changes/indicator',{{target:'#pending-changes',swap:'innerHTML'}})\" class=\"btn btn-sm btn-error\">Apply (activate)</button></div>", esc)
    } else {
        "<div class=\"text-sm\">Settings match the last applied version. No pending changes.</div>"
            .to_string()
    };
    RawHtml(body)
}

#[post("/changes/revert")]
pub fn revert_settings(config: &State<Arc<AppConfig>>) -> RawHtml<String> {
    let dir = config_dir(&config);
    let dir_str = dir.to_str().unwrap_or(".");
    let source = PathBuf::from("/etc/neo/settings.toml");
    let dummy = DocumentMut::new();
    match paste_settings(dir_str, &source, &dummy, false, &config.nix_cmd) {
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
    let dir = config_dir(&config);
    let dir_str = dir.to_str().unwrap_or(".");
    let nix_cmd = &config.nix_cmd;
    let content = fs::read_to_string(&config.settings_path).unwrap_or_default();
    let doc: DocumentMut = content.parse().unwrap_or_else(|_| DocumentMut::new());
    let section = if PathBuf::from("/etc/neo/settings.toml").exists() {
        "neo-service"
    } else {
        "neo-cli"
    };
    match update(dir_str, &doc, section, false, nix_cmd) {
        Ok(()) => RawHtml(
            "<span class=\"text-success text-[10px]\">flake update done (direct)</span>"
                .to_string(),
        ),
        Err(e) => RawHtml(format!(
            "<span class=\"text-error text-[10px]\">update failed: {}</span>",
            e
        )),
    }
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
    match paste_settings(dir_str, &source, &dummy, false, &config.nix_cmd) {
        Ok(()) => RawHtml(
            "<span class=\"text-success text-[10px]\">reset (paste from /etc) done</span>"
                .to_string(),
        ),
        Err(e) => RawHtml(format!(
            "<span class=\"text-error text-[10px]\">reset failed: {}</span>",
            e
        )),
    }
}

#[post("/actions/hard-reset")]
pub fn actions_hard_reset(config: &State<Arc<AppConfig>>) -> RawHtml<String> {
    let dir = config_dir(&config);
    let dir_str = dir.to_str().unwrap_or(".");
    let nix_cmd = &config.nix_cmd;
    let content = fs::read_to_string(&config.settings_path).unwrap_or_default();
    let doc: DocumentMut = content.parse().unwrap_or_else(|_| DocumentMut::new());
    let section = if PathBuf::from("/etc/neo/settings.toml").exists() {
        "neo-service"
    } else {
        "neo-cli"
    };
    let nuke_res = nuke(dir_str, false, nix_cmd);
    let init_res = init(dir_str, &doc, section, false, nix_cmd);
    match (nuke_res, init_res) {
        (Ok(()), Ok(())) => RawHtml("<span class=\"text-success text-[10px]\">hard reset (nuke+init) done — reload dashboard</span>".to_string()),
        (Err(e), _) | (_, Err(e)) => RawHtml(format!("<span class=\"text-error text-[10px]\">hard reset error: {}</span>", e)),
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
pub fn core_pane(config: &State<Arc<AppConfig>>, section: &str) -> Template {
    let pane = extract_neo_section(&config.nix_cmd, &config.neo_input, section);
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

pub fn routes() -> Vec<rocket::Route> {
    routes![
        index,
        configuration,
        option_pane,
        services_grid,
        save_service,
        save_core_section,
        changes_indicator,
        changes_summary,
        revert_settings,
        apply_settings,
        flake_update,
        actions_activate,
        actions_reset,
        actions_hard_reset,
        branches,
        git_switch,
        core_grid,
        core_pane,
        activation_monitor,
        activation_log,
        activation_status,
        activation_current
    ]
}
