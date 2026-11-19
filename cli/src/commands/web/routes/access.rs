//! Access page: `/configuration/access` (docs/design/access-and-defaults.md).
//!
//! Every way into the server in one place: the default access for apps
//! (`neo.access`), each app's places, login page and SSO, the open ports and the
//! server's own logins. The structure comes from the evaluation
//! (`neo.exposure.report`); the values a user can change here are read from
//! settings.toml, so a click shows its result before the next evaluation.
//! Changes go through `apply_edit` (a patch), like the settings form.
use std::sync::Arc;

use rocket::form::Form;
use rocket::{get, post, FromForm, State};
use rocket_dyn_templates::{Metadata, Template};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use toml_edit::DocumentMut;

use super::pages::configuration_shell;
use crate::commands::web::guides::option_url;
use crate::commands::web::nix::errors::eval_error_ui;
use crate::commands::web::nix::sections::humanize;
use crate::commands::web::settings::edit::{apply_edit, Edit, PatchOp};
use crate::commands::web::settings::save::load_settings_doc;
use crate::commands::web::settings::toml_item_to_json;
use crate::commands::web::types::{AppConfig, EvalErrorUi, EvalMark};
use crate::commands::web::util::{service_name_ok, Htmx};

/// Places an app can be opened from, in the order of the matrix columns.
const PLACES: [(&str, &str); 4] = [
    ("local", "Home network"),
    ("tailscale", "Tailscale"),
    ("netbird", "NetBird"),
    ("web", "Internet"),
];
/// Defaults of the `neo.access` options (nix/modules/core/access.nix).
const DEFAULT_LOGIN: bool = true;
const DEFAULT_SSO: bool = false;

fn all_places() -> Vec<String> {
    PLACES.iter().map(|(id, _)| id.to_string()).collect()
}

// ---- evaluation report (neo.exposure.report) --------------------------------

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct AccessReport {
    #[serde(default)]
    pub apps: Vec<ReportApp>,
    #[serde(default)]
    pub ports: Vec<ReportPort>,
    #[serde(default)]
    pub places: ReportPlaces,
    #[serde(default)]
    pub ssh: ReportSsh,
    #[serde(default, rename = "loginPage")]
    pub login_page: ReportLoginPage,
    #[serde(default)]
    pub domain: Option<String>,
    #[serde(default)]
    pub undescribed: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct ReportApp {
    pub name: String,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub hosts: Vec<String>,
    #[serde(default)]
    pub ingress: Vec<String>,
    /// Settings path the places follow (`access.defaultIngress`); None = fixed by the app.
    #[serde(default, rename = "ingressFrom")]
    pub ingress_from: Option<String>,
    #[serde(default)]
    pub auth: ReportAuth,
    #[serde(default)]
    pub sso: Option<ReportSso>,
    #[serde(default, rename = "ownLogin")]
    pub own_login: Option<String>,
    #[serde(default)]
    pub who: ReportWho,
}

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct ReportAuth {
    #[serde(default)]
    pub available: bool,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub from: Option<String>,
    #[serde(default, rename = "publicPaths")]
    pub public_paths: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct ReportSso {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub from: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct ReportWho {
    #[serde(default)]
    pub users: Vec<String>,
    #[serde(default)]
    pub groups: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct ReportPort {
    pub protocol: String,
    pub port: u32,
    #[serde(default)]
    pub interface: Option<String>,
    #[serde(default)]
    pub container: Option<String>,
    #[serde(default)]
    pub described: bool,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub service: Option<String>,
    #[serde(default)]
    pub login: String,
    #[serde(default)]
    pub open: bool,
}

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct ReportPlaces {
    #[serde(default)]
    pub local: bool,
    #[serde(default)]
    pub tailscale: bool,
    #[serde(default)]
    pub netbird: bool,
    #[serde(default)]
    pub relay: bool,
}

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct ReportSsh {
    #[serde(default)]
    pub ports: Vec<u32>,
    #[serde(default)]
    pub users: Vec<String>,
    #[serde(default)]
    pub keys: Vec<String>,
    #[serde(default, rename = "passwordLogin")]
    pub password_login: bool,
    #[serde(default, rename = "consolePassword")]
    pub console_password: bool,
    #[serde(default, rename = "netbirdSsh")]
    pub netbird_ssh: bool,
}

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct ReportLoginPage {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub users: Vec<String>,
    #[serde(default)]
    pub providers: Vec<String>,
    #[serde(default)]
    pub groups: Vec<String>,
    #[serde(default, rename = "oidcApps")]
    pub oidc_apps: Vec<String>,
}

// ---- page context -----------------------------------------------------------

/// One toggle: its state and the patch a click sends.
#[derive(Serialize, Default)]
struct Toggle {
    id: String,
    label: String,
    on: bool,
    /// The place is not set up on this server (e.g. Tailscale off).
    inactive: bool,
    path: String,
    /// JSON value a click writes.
    value: String,
}

#[derive(Serialize, Default)]
struct Defaults {
    places: Vec<Toggle>,
    login: Toggle,
    sso: Toggle,
    /// SSO needs the login page.
    sso_blocked: bool,
}

#[derive(Serialize, Default)]
struct Setting {
    /// The value is set for this app (not the default).
    changed: bool,
    /// The app fixes the value itself (no default to follow).
    fixed: bool,
}

#[derive(Serialize, Default)]
struct AppRow {
    name: String,
    title: String,
    icon: Option<String>,
    url: Option<String>,
    host: Option<String>,
    settings_url: String,
    places: Vec<Toggle>,
    places_state: Setting,
    login: Option<Toggle>,
    login_state: Setting,
    sso: Option<Toggle>,
    sso_state: Setting,
    own_login: Option<String>,
    public_paths: usize,
    who: String,
    /// Some value is set for this app: show "Use defaults".
    any_changed: bool,
    risk: Option<String>,
}

#[derive(Serialize, Default)]
struct PortRow {
    port: String,
    label: String,
    login: String,
    open: bool,
    described: bool,
    scope: String,
    service: Option<String>,
    service_url: Option<String>,
}

#[derive(Serialize, Default)]
struct KeyRow {
    kind: String,
    comment: String,
    tail: String,
}

#[derive(Serialize, Default)]
struct Warning {
    level: &'static str,
    text: String,
    link: Option<String>,
    link_label: Option<String>,
}

#[derive(Serialize, Default)]
struct AccessContext {
    defaults: Defaults,
    apps: Vec<AppRow>,
    place_headers: Vec<Toggle>,
    ports: Vec<PortRow>,
    ssh: ReportSsh,
    ssh_keys: Vec<KeyRow>,
    ssh_keys_url: String,
    console_password_url: String,
    login_page: ReportLoginPage,
    login_page_settings_url: String,
    warnings: Vec<Warning>,
    domain: Option<String>,
    relay: bool,
    /// A click that could not be saved (or settings that could not be read).
    save_error: Option<String>,
    assistant: bool,
    #[serde(flatten)]
    eval_error: EvalErrorUi,
    eval: EvalMark,
}

// ---- settings.toml lookups --------------------------------------------------

fn setting(doc: &DocumentMut, path: &str) -> Option<Value> {
    let mut item = doc.as_item();
    for key in path.split('.') {
        item = item.get(key)?;
    }
    toml_item_to_json(item)
}

fn places_value(v: Option<Value>) -> Option<Vec<String>> {
    let list = v?.as_array()?.clone();
    Some(
        list.iter()
            .filter_map(|x| x.as_str().map(str::to_string))
            .collect(),
    )
}

/// The places in matrix order (so a stored list and a default compare equal).
fn ordered(list: &[String]) -> Vec<String> {
    PLACES
        .iter()
        .filter(|(id, _)| list.iter().any(|x| x == id))
        .map(|(id, _)| id.to_string())
        .collect()
}

fn toggled(list: &[String], id: &str) -> Vec<String> {
    let mut l: Vec<String> = list.iter().filter(|x| *x != id).cloned().collect();
    if !list.iter().any(|x| x == id) {
        l.push(id.to_string());
    }
    ordered(&l)
}

struct Effective {
    places: Vec<String>,
    login: bool,
    sso: bool,
}

fn defaults_of(doc: &DocumentMut) -> Effective {
    Effective {
        places: places_value(setting(doc, "access.defaultIngress"))
            .map(|l| ordered(&l))
            .unwrap_or_else(all_places),
        login: setting(doc, "access.defaultLogin")
            .and_then(|v| v.as_bool())
            .unwrap_or(DEFAULT_LOGIN),
        sso: setting(doc, "access.defaultSso")
            .and_then(|v| v.as_bool())
            .unwrap_or(DEFAULT_SSO),
    }
}

/// `ssh-ed25519 AAAA… comment` → type, comment and the key's last characters.
fn key_row(key: &str) -> KeyRow {
    let mut parts = key.split_whitespace();
    let kind = parts.next().unwrap_or_default().to_string();
    let blob = parts.next().unwrap_or_default();
    let comment = parts.collect::<Vec<_>>().join(" ");
    let tail: String = blob
        .chars()
        .rev()
        .take(10)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    KeyRow {
        kind: kind.trim_start_matches("ssh-").to_string(),
        comment,
        tail,
    }
}

fn who_label(app: &ReportApp, login_on: bool, sso_on: bool, lp: &ReportLoginPage) -> String {
    if !login_on && !sso_on {
        return match &app.own_login {
            Some(own) => own.clone(),
            None => "Everyone who can reach it".into(),
        };
    }
    let mut parts = Vec::new();
    let users = &app.who.users;
    if !lp.users.is_empty() {
        if users.len() == lp.users.len() {
            parts.push(if users.len() == 1 {
                "Your 1 user".to_string()
            } else {
                format!("All {} users", users.len())
            });
        } else if users.is_empty() {
            parts.push("No user".into());
        } else {
            parts.push(users.join(", "));
        }
    }
    if !lp.providers.is_empty() {
        if lp.groups.is_empty() {
            parts.push(format!("{} accounts", lp.providers.join(", ")));
        } else if !app.who.groups.is_empty() {
            parts.push(format!("groups {}", app.who.groups.join(", ")));
        }
    }
    if parts.is_empty() {
        "Nobody yet: add users to the login page".into()
    } else {
        parts.join(" · ")
    }
}

fn build(report: &AccessReport, doc: &DocumentMut) -> AccessContext {
    let d = defaults_of(doc);
    let places_on = |id: &str| match id {
        "tailscale" => report.places.tailscale,
        "netbird" => report.places.netbird,
        _ => true,
    };
    let lp = &report.login_page;

    let defaults = Defaults {
        places: PLACES
            .iter()
            .map(|(id, label)| Toggle {
                id: id.to_string(),
                label: label.to_string(),
                on: d.places.iter().any(|x| x == id),
                inactive: !places_on(id),
                path: "access.defaultIngress".into(),
                value: json!(toggled(&d.places, id)).to_string(),
            })
            .collect(),
        login: Toggle {
            id: "login".into(),
            label: "Ask for login".into(),
            on: d.login,
            path: "access.defaultLogin".into(),
            value: json!(!d.login).to_string(),
            ..Default::default()
        },
        sso: Toggle {
            id: "sso".into(),
            label: "Sign in with the login page account".into(),
            on: d.sso,
            path: "access.defaultSso".into(),
            value: json!(!d.sso).to_string(),
            ..Default::default()
        },
        sso_blocked: !lp.enabled && !d.sso,
    };

    let mut warnings = Vec::new();
    let mut apps: Vec<AppRow> = report
        .apps
        .iter()
        .map(|a| {
            let base = format!("services.{}", a.name);
            // Places
            let set_places = places_value(setting(doc, &format!("{base}.ingress")));
            let places_fixed = a.ingress_from.is_none();
            let places = match (&set_places, places_fixed) {
                (Some(l), _) => ordered(l),
                (None, false) => d.places.clone(),
                (None, true) => ordered(&a.ingress),
            };
            // Login page
            let set_login = setting(doc, &format!("{base}.auth.enabled")).and_then(|v| v.as_bool());
            let login_fixed = a.auth.from.is_none();
            let login_on = a.auth.available
                && set_login.unwrap_or(if login_fixed { a.auth.enabled } else { d.login });
            // SSO
            let set_sso = setting(doc, &format!("{base}.sso")).and_then(|v| v.as_bool());
            let sso_on = a.sso.as_ref().is_some_and(|s| {
                set_sso.unwrap_or(if s.from.is_none() { s.enabled } else { d.sso })
            });

            let risk = if places.iter().any(|p| p == "web")
                && !login_on
                && !sso_on
                && a.own_login.is_none()
            {
                Some("Anyone on the internet can open it without logging in.".to_string())
            } else {
                None
            };
            if let Some(r) = &risk {
                // An app without any login by design (a public pastebin) is a
                // warning; a login someone turned off is an error.
                let by_design = !a.auth.available || (login_fixed && set_login.is_none());
                warnings.push(Warning {
                    level: if by_design { "warning" } else { "error" },
                    text: format!("{}: {r}", humanize(&a.name)),
                    link: None,
                    link_label: None,
                });
            }
            let title = humanize(&a.name);
            AppRow {
                places: PLACES
                    .iter()
                    .map(|(id, label)| Toggle {
                        id: id.to_string(),
                        label: label.to_string(),
                        on: places.iter().any(|x| x == id),
                        inactive: !places_on(id),
                        path: format!("{base}.ingress"),
                        value: json!(toggled(&places, id)).to_string(),
                    })
                    .collect(),
                places_state: Setting {
                    changed: set_places.is_some(),
                    fixed: places_fixed,
                },
                login: a.auth.available.then(|| Toggle {
                    id: "login".into(),
                    label: "Login page".into(),
                    on: login_on,
                    path: format!("{base}.auth.enabled"),
                    value: json!(!login_on).to_string(),
                    ..Default::default()
                }),
                login_state: Setting {
                    changed: set_login.is_some(),
                    fixed: login_fixed,
                },
                sso: a.sso.as_ref().map(|_| Toggle {
                    id: "sso".into(),
                    label: "Sign in with the login page".into(),
                    on: sso_on,
                    inactive: !lp.enabled,
                    path: format!("{base}.sso"),
                    value: json!(!sso_on).to_string(),
                }),
                sso_state: Setting {
                    changed: set_sso.is_some(),
                    fixed: a.sso.as_ref().is_some_and(|s| s.from.is_none()),
                },
                any_changed: (set_places.is_some() && !places_fixed)
                    || (set_login.is_some() && !login_fixed)
                    || (set_sso.is_some() && a.sso.as_ref().is_some_and(|s| s.from.is_some())),
                own_login: a.own_login.clone(),
                public_paths: if login_on {
                    a.auth.public_paths.len()
                } else {
                    0
                },
                who: who_label(a, login_on, sso_on, lp),
                host: a.hosts.first().cloned(),
                url: a.url.clone(),
                icon: a.icon.clone(),
                settings_url: format!("/configuration/option/{}?focus=ingress", a.name),
                name: a.name.clone(),
                title,
                risk,
            }
        })
        .collect();
    apps.sort_by_key(|a| a.title.to_lowercase());

    let login_needed = apps.iter().any(|a| a.login.as_ref().is_some_and(|t| t.on));
    if login_needed && !lp.enabled {
        warnings.push(Warning {
            level: "error",
            text: "Some apps ask for login, but the login page (tinyauth) is off, so they cannot open.".into(),
            link: Some("/configuration/option/tinyauth".into()),
            link_label: Some("Login page settings".into()),
        });
    }
    if report.ssh.password_login {
        warnings.push(Warning {
            level: "warning",
            text: "SSH accepts passwords, because no SSH key is set. Add your key so only it can log in.".into(),
            link: Some(option_url("core.ssh.authorizedKeys")),
            link_label: Some("Add an SSH key".into()),
        });
    }
    for p in &report.undescribed {
        warnings.push(Warning {
            level: "warning",
            text: format!("Port {p} is open, but Neo does not know what uses it."),
            link: None,
            link_label: None,
        });
    }

    let ports = report
        .ports
        .iter()
        .map(|p| PortRow {
            port: format!("{} {}", p.port, p.protocol.to_uppercase()),
            label: p.label.clone(),
            login: if p.described {
                p.login.clone()
            } else {
                "Unknown".into()
            },
            open: p.open,
            described: p.described,
            scope: match (&p.interface, &p.container) {
                (Some(i), _) if i == "wt0" => "NetBird only".into(),
                (Some(i), _) if i.starts_with("tailscale") => "Tailscale only".into(),
                (Some(i), _) if i.starts_with("ve-") => "This server only".into(),
                (Some(i), _) => format!("Only on {i}"),
                (None, _) => "Every network".into(),
            },
            service_url: p
                .service
                .as_ref()
                .map(|s| format!("/configuration/option/{s}")),
            service: p.service.as_ref().map(|s| humanize(s)),
        })
        .collect();

    AccessContext {
        defaults,
        place_headers: PLACES
            .iter()
            .map(|(id, label)| Toggle {
                id: id.to_string(),
                label: label.to_string(),
                inactive: !places_on(id),
                ..Default::default()
            })
            .collect(),
        apps,
        ports,
        ssh_keys: report.ssh.keys.iter().map(|k| key_row(k)).collect(),
        ssh: report.ssh.clone(),
        ssh_keys_url: option_url("core.ssh.authorizedKeys"),
        console_password_url: option_url("core.hashedLinuxPassword"),
        login_page: lp.clone(),
        login_page_settings_url: "/configuration/option/tinyauth".into(),
        warnings,
        domain: report.domain.clone(),
        relay: report.places.relay,
        assistant: crate::commands::web::agent::available(),
        ..Default::default()
    }
}

async fn context(config: &AppConfig, error: Option<String>) -> AccessContext {
    let (r, eval) = config.eval.access().await;
    let mut ctx = match r {
        Ok(report) => match load_settings_doc(&config.settings_path) {
            Ok(doc) => build(&report, &doc),
            Err(e) => AccessContext {
                save_error: Some(format!("Could not read the settings: {}", e.message())),
                ..Default::default()
            },
        },
        Err(e) => {
            eprintln!("web: nix extract_access failed: {e:#}");
            AccessContext {
                eval_error: eval_error_ui("Nix error while building the access overview", &e),
                ..Default::default()
            }
        }
    };
    if error.is_some() {
        ctx.save_error = error;
    }
    ctx.eval = eval;
    ctx
}

#[get("/configuration/access")]
pub async fn configuration_access(
    config: &State<Arc<AppConfig>>,
    htmx: Htmx,
    meta: Metadata<'_>,
) -> Template {
    let ctx = context(config, None).await;
    if htmx.is_htmx() {
        return Template::render("access", ctx);
    }
    let html = meta.render("access", &ctx).map(|(_, h)| h);
    configuration_shell(
        config,
        "/configuration/access",
        "settings",
        Some("Access".into()),
        html,
    )
    .await
}

#[derive(FromForm)]
pub struct AccessSet {
    /// Settings path (`access.defaultIngress`, `services.immich.ingress`, …).
    path: String,
    /// JSON value to write, or empty to follow the default again.
    #[field(default = String::new())]
    value: String,
    /// `follow`: remove the app's places, login and SSO values (path = `services.<app>`).
    #[field(default = String::new())]
    op: String,
}

/// What a path may hold and what it falls back to when removed.
enum Field {
    Places,
    Bool,
}

/// Allowed paths: the three defaults and an app's places, login and SSO.
fn field_of(path: &str) -> Option<Field> {
    match path {
        "access.defaultIngress" => return Some(Field::Places),
        "access.defaultLogin" | "access.defaultSso" => return Some(Field::Bool),
        _ => {}
    }
    let rest = path.strip_prefix("services.")?;
    let (svc, key) = rest.split_once('.')?;
    if !service_name_ok(svc) {
        return None;
    }
    match key {
        "ingress" => Some(Field::Places),
        "auth.enabled" | "sso" => Some(Field::Bool),
        _ => None,
    }
}

fn split(path: &str) -> Vec<String> {
    path.split('.').map(str::to_string).collect()
}

/// The patch for one click. A value equal to what the setting would follow
/// anyway is removed instead, so the setting keeps following the default.
fn ops_for(
    report: &AccessReport,
    doc: &DocumentMut,
    form: &AccessSet,
) -> Result<Vec<PatchOp>, String> {
    if form.op == "follow" {
        let app = form
            .path
            .strip_prefix("services.")
            .filter(|a| report.apps.iter().any(|x| x.name == *a))
            .ok_or("Unknown app")?;
        let a = report
            .apps
            .iter()
            .find(|x| x.name == app)
            .ok_or("Unknown app")?;
        let mut ops = Vec::new();
        if a.ingress_from.is_some() {
            ops.push(format!("services.{app}.ingress"));
        }
        if a.auth.from.is_some() {
            ops.push(format!("services.{app}.auth.enabled"));
        }
        if a.sso.as_ref().is_some_and(|s| s.from.is_some()) {
            ops.push(format!("services.{app}.sso"));
        }
        return Ok(ops
            .into_iter()
            .map(|p| PatchOp::Unset {
                path: split(&p),
                secret: false,
            })
            .collect());
    }

    let field = field_of(&form.path).ok_or("This setting cannot be changed here")?;
    let value: Value = serde_json::from_str(&form.value).map_err(|_| "Invalid value")?;
    let value = match field {
        Field::Places => {
            let list = places_value(Some(value)).ok_or("Expected a list of places")?;
            if list.iter().any(|p| !PLACES.iter().any(|(id, _)| id == p)) {
                return Err("Unknown place".into());
            }
            json!(ordered(&list))
        }
        Field::Bool => json!(value.as_bool().ok_or("Expected on or off")?),
    };

    // What the setting follows when it has no value of its own.
    let d = defaults_of(doc);
    let follows: Option<Value> = match form.path.as_str() {
        "access.defaultIngress" => Some(json!(all_places())),
        "access.defaultLogin" => Some(json!(DEFAULT_LOGIN)),
        "access.defaultSso" => Some(json!(DEFAULT_SSO)),
        p => {
            let (app, key) = p
                .strip_prefix("services.")
                .and_then(|r| r.split_once('.'))
                .ok_or("Unknown setting")?;
            let a = report
                .apps
                .iter()
                .find(|x| x.name == app)
                .ok_or("Unknown app")?;
            match key {
                "ingress" => a.ingress_from.as_ref().map(|_| json!(d.places)),
                "auth.enabled" if a.auth.available => a.auth.from.as_ref().map(|_| json!(d.login)),
                "auth.enabled" => return Err("This app has no login page setting".into()),
                "sso" => match &a.sso {
                    Some(s) => s.from.as_ref().map(|_| json!(d.sso)),
                    None => return Err("This app cannot sign in with the login page".into()),
                },
                _ => return Err("Unknown setting".into()),
            }
        }
    };
    let path = split(&form.path);
    Ok(vec![if follows.as_ref() == Some(&value) {
        PatchOp::Unset {
            path,
            secret: false,
        }
    } else {
        PatchOp::Set { path, value }
    }])
}

#[post("/access/set", data = "<form>")]
pub async fn access_set(config: &State<Arc<AppConfig>>, form: Form<AccessSet>) -> Template {
    let (r, _) = config.eval.access().await;
    let error = match r {
        Err(e) => Some(format!("The overview could not be evaluated: {e:#}")),
        Ok(report) => match load_settings_doc(&config.settings_path) {
            Err(e) => Some(e.message().to_string()),
            Ok(doc) => match ops_for(&report, &doc, &form) {
                Err(msg) => Some(msg),
                Ok(ops) if ops.is_empty() => None,
                Ok(ops) => apply_edit(config, Edit::Patch { ops })
                    .await
                    .err()
                    .map(|e| e.message().to_string()),
            },
        },
    };
    Template::render("access", context(config, error).await)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report() -> AccessReport {
        AccessReport {
            apps: vec![
                ReportApp {
                    name: "immich".into(),
                    ingress: all_places(),
                    ingress_from: Some("access.defaultIngress".into()),
                    auth: ReportAuth {
                        available: true,
                        enabled: true,
                        from: Some("access.defaultLogin".into()),
                        ..Default::default()
                    },
                    ..Default::default()
                },
                ReportApp {
                    name: "gitea".into(),
                    ingress: vec!["local".into()],
                    ingress_from: None,
                    auth: ReportAuth {
                        available: true,
                        enabled: false,
                        from: None,
                        ..Default::default()
                    },
                    own_login: Some("Gitea accounts".into()),
                    ..Default::default()
                },
            ],
            ..Default::default()
        }
    }

    fn form(path: &str, value: &str) -> AccessSet {
        AccessSet {
            path: path.into(),
            value: value.into(),
            op: String::new(),
        }
    }

    #[test]
    fn same_as_default_unsets_and_other_values_set() {
        let doc: DocumentMut = "[access]\ndefaultIngress = [\"local\", \"netbird\"]\n"
            .parse()
            .unwrap();
        let ops = ops_for(
            &report(),
            &doc,
            &form("services.immich.ingress", r#"["netbird","local"]"#),
        )
        .unwrap();
        assert!(
            matches!(&ops[0], PatchOp::Unset { path, .. } if path.join(".") == "services.immich.ingress")
        );
        let ops = ops_for(
            &report(),
            &doc,
            &form("services.immich.ingress", r#"["local"]"#),
        )
        .unwrap();
        assert!(matches!(&ops[0], PatchOp::Set { value, .. } if value == &json!(["local"])));
        // A fixed value never unsets: there is no default to follow.
        let ops = ops_for(
            &report(),
            &doc,
            &form("services.gitea.ingress", r#"["local"]"#),
        )
        .unwrap();
        assert!(matches!(&ops[0], PatchOp::Set { .. }));
    }

    #[test]
    fn refuses_other_paths_and_bad_values() {
        let doc = DocumentMut::new();
        assert!(ops_for(&report(), &doc, &form("core.ssh.authorizedKeys", "[]")).is_err());
        assert!(ops_for(
            &report(),
            &doc,
            &form("services.immich.ingress", r#"["moon"]"#)
        )
        .is_err());
        assert!(ops_for(
            &report(),
            &doc,
            &form("services.immich.auth.enabled", "\"yes\"")
        )
        .is_err());
        assert!(ops_for(&report(), &doc, &form("services.immich.sso", "true")).is_err());
    }

    #[test]
    fn effective_values_follow_settings_before_the_evaluation() {
        let doc: DocumentMut = "[access]\ndefaultIngress = [\"local\"]\ndefaultLogin = false\n"
            .parse()
            .unwrap();
        let ctx = build(&report(), &doc);
        let immich = ctx.apps.iter().find(|a| a.name == "immich").unwrap();
        let on: Vec<_> = immich
            .places
            .iter()
            .filter(|p| p.on)
            .map(|p| p.id.as_str())
            .collect();
        assert_eq!(on, ["local"]);
        assert!(!immich.login.as_ref().unwrap().on);
        assert!(!immich.places_state.changed);
        // Gitea fixes its places and login: it keeps them.
        let gitea = ctx.apps.iter().find(|a| a.name == "gitea").unwrap();
        assert!(gitea.places_state.fixed);
        assert_eq!(gitea.who, "Gitea accounts");
    }

    #[test]
    fn internet_without_any_login_is_a_warning() {
        let doc: DocumentMut = "[access]\ndefaultLogin = false\n".parse().unwrap();
        let ctx = build(&report(), &doc);
        assert!(ctx
            .apps
            .iter()
            .find(|a| a.name == "immich")
            .unwrap()
            .risk
            .is_some());
        assert!(ctx.warnings.iter().any(|w| w.text.starts_with("Immich")));
    }

    #[test]
    fn follow_removes_only_inherited_values() {
        let doc = DocumentMut::new();
        let f = AccessSet {
            path: "services.gitea".into(),
            value: String::new(),
            op: "follow".into(),
        };
        assert!(ops_for(&report(), &doc, &f).unwrap().is_empty());
        let f = AccessSet {
            path: "services.immich".into(),
            value: String::new(),
            op: "follow".into(),
        };
        assert_eq!(ops_for(&report(), &doc, &f).unwrap().len(), 2);
    }
}
