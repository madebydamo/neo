use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;

use rocket::response::content::RawHtml;
use rocket::serde::json::Json;
use rocket::{get, http::Status, post, routes, State};
use rocket_dyn_templates::Template;
use toml_edit::{DocumentMut, Item, Table, Value};

use super::nix_eval::{extract_proxied_services, extract_service_options, extract_services};
use super::structs::{AppConfig, IndexContext, OptionPaneContext};

use crate::commands::paste_settings::paste_settings;
use crate::commands::update::update;

use super::activation;

#[get("/")]
pub fn index(config: &State<Arc<AppConfig>>) -> Template {
    let data = extract_proxied_services(&config.nix_cmd, &config.neo_input);
    Template::render("index", data)
}

#[get("/configuration")]
pub fn configuration(config: &State<Arc<AppConfig>>) -> Template {
    let svcs = extract_services(&config.nix_cmd, &config.neo_input);
    Template::render("configuration", IndexContext { services: svcs })
}

#[get("/option/<service>")]
pub fn option_pane(config: &State<Arc<AppConfig>>, service: &str) -> Template {
    let pane = extract_service_options(&config.nix_cmd, &config.neo_input, service);
    Template::render("option_pane", pane)
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
    let dir = config_dir(&config);
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
    let _ = fs::write(&state_path, serde_json::to_string_pretty(&initial).unwrap_or_default());
    let _ = fs::write(&log_path, format!("activation {} triggered via web at {}\n", id, ts));
    if let Some(other) = activation::find_recent_in_progress_activation() {
        if other != id {
            return RawHtml(format!("<div class=\"alert alert-error text-sm\">Another activation {} in progress (or auto-update). Wait.</div>", other));
        }
    }
    let systemctl_bin = "/run/current-system/sw/bin/systemctl";
    let svc = format!("neo-activate@{}.service", ts);
    let desc = format!("{} {} start --no-block {} ", sudo_cmd, systemctl_bin, svc);
    let _ = crate::commands::execute_command(
        &mut Command::new(&sudo_cmd).args([systemctl_bin, "start", "--no-block", &svc]),
        &desc,
    );
    RawHtml(activation::build_monitor_fragment(&id))
}

#[post("/flake/update")]
pub fn flake_update(config: &State<Arc<AppConfig>>) -> RawHtml<String> {
    let dir = config_dir(&config);
    let dir_str = dir.to_str().unwrap_or(".");
    match update(dir_str, false, &config.nix_cmd) {
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
        save_service,
        changes_indicator,
        changes_summary,
        revert_settings,
        apply_settings,
        flake_update,
        activation_monitor,
        activation_log,
        activation_status,
        activation_current
    ]
}
