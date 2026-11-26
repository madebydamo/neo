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
    #[serde(default, alias = "loginPage")]
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
    #[serde(default, alias = "ingressFrom")]
    pub ingress_from: Option<String>,
    #[serde(default)]
    pub auth: ReportAuth,
    #[serde(default)]
    pub sso: Option<ReportSso>,
    #[serde(default, alias = "ownLogin")]
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
    #[serde(default, alias = "publicPaths")]
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
    /// Id in neo.exposure.ports (None for a port nobody describes).
    #[serde(default)]
    pub id: Option<String>,
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
    /// Docker binds the port to this address only ("" = every address).
    #[serde(default)]
    pub address: String,
    /// What the host firewall does with it: allowed | interface | docker | blocked | off.
    #[serde(default)]
    pub firewall: String,
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
    /// Keys of other computers: homeserver only, never a decryptor.
    #[serde(default, alias = "machineKeys")]
    pub machine_keys: Vec<String>,
    #[serde(default, alias = "passwordLogin")]
    pub password_login: bool,
    #[serde(default, alias = "consolePassword")]
    pub console_password: bool,
    #[serde(default, alias = "netbirdSsh")]
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
    #[serde(default, alias = "oidcApps")]
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
    /// On, and with it anyone on the internet gets in without a login.
    danger: bool,
    /// Open on purpose: a public website, or the app's own login guards it.
    info: bool,
}

/// One quick filter above the lists: matches rows whose tags contain `id`.
#[derive(Serialize, Default, Debug)]
struct Filter {
    id: &'static str,
    label: &'static str,
    help: &'static str,
    apps: usize,
    ports: usize,
}

/// Quick filters, in order (tags: `app_tags`, `access_ports::port_tags`).
const FILTERS: [(&str, &str, &str); 9] = [
    (
        "risk",
        "Problems",
        "Apps whose login was turned off while the internet reaches them, and ports Neo cannot explain",
    ),
    (
        "bucket-public",
        "Public websites",
        "Open to everyone on the internet, on purpose",
    ),
    (
        "bucket-own",
        "App's own login",
        "On the internet; the app asks for its own login instead of the login page",
    ),
    (
        "internet-nologin",
        "Internet without login page",
        "Reachable from the internet and the login page does not guard it",
    ),
    ("internet", "On the internet", "Reachable from the internet"),
    (
        "nologin",
        "No login page",
        "The login page does not guard it (the app may still have its own login)",
    ),
    (
        "openpaths",
        "Open paths",
        "Some addresses of the app skip the login page (share links, phone apps)",
    ),
    (
        "skipsfw",
        "Skips the firewall",
        "Docker publishes the port before the firewall sees it",
    ),
    (
        "changed",
        "Changed for an app",
        "Set for this app instead of following the defaults",
    ),
];

/// Quick-filter tags of an app, space-separated.
fn app_tags(internet: bool, login_on: bool, open_paths: bool, risk: bool, changed: bool) -> String {
    let mut t = Vec::new();
    if internet {
        t.push("internet");
    }
    if !login_on {
        t.push("nologin");
    }
    if internet && !login_on {
        t.push("internet-nologin");
    }
    if open_paths {
        t.push("openpaths");
    }
    if risk {
        t.push("risk");
    }
    if changed {
        t.push("changed");
    }
    t.join(" ")
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
    /// Without a login on purpose (public website).
    public_by_design: bool,
    /// Exposure group on the access map (`access_map::bucket_of`).
    bucket: &'static str,
    /// Quick-filter tags.
    tags: String,
}

#[derive(Serialize, Default)]
struct KeyRow {
    kind: String,
    comment: String,
    tail: String,
    /// A machine key (core.ssh.machineKeys).
    machine: bool,
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
    ports: super::access_ports::PortsFlow,
    filters: Vec<Filter>,
    map: super::access_map::AccessMap,
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
fn key_row(key: &str, machine: bool) -> KeyRow {
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
        machine,
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
                danger: false,
                info: false,
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

            // No login on purpose: the app has none, or its module fixes it
            // off and nobody changed that (a public website). Not a problem.
            let public_by_design =
                !a.auth.available || (login_fixed && set_login.is_none() && !login_on);
            let risk = if places.iter().any(|p| p == "web")
                && !login_on
                && !sso_on
                && a.own_login.is_none()
                && !public_by_design
            {
                Some("Anyone on the internet can open it without logging in.".to_string())
            } else {
                None
            };
            let title = humanize(&a.name);
            let internet = places.iter().any(|p| p == "web");
            let any_changed = (set_places.is_some() && !places_fixed)
                || (set_login.is_some() && !login_fixed)
                || (set_sso.is_some() && a.sso.as_ref().is_some_and(|s| s.from.is_some()));
            let open_paths = login_on && !a.auth.public_paths.is_empty();
            let tags = app_tags(internet, login_on, open_paths, risk.is_some(), any_changed);
            AppRow {
                tags,
                public_by_design,
                bucket: "",
                places: PLACES
                    .iter()
                    .map(|(id, label)| Toggle {
                        id: id.to_string(),
                        label: label.to_string(),
                        on: places.iter().any(|x| x == id),
                        inactive: !places_on(id),
                        path: format!("{base}.ingress"),
                        value: json!(toggled(&places, id)).to_string(),
                        danger: *id == "web" && internet && !login_on && !sso_on,
                        info: false,
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
                    danger: internet && !login_on && !sso_on,
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
                    danger: false,
                    info: false,
                }),
                sso_state: Setting {
                    changed: set_sso.is_some(),
                    fixed: a.sso.as_ref().is_some_and(|s| s.from.is_none()),
                },
                any_changed,
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
    let ports = super::access_ports::build(&report.ports, &report.places);
    let map_apps: Vec<super::access_map::MapApp> = apps
        .iter()
        .map(|a| super::access_map::MapApp {
            name: a.name.clone(),
            title: a.title.clone(),
            icon: a.icon.clone(),
            places: a
                .places
                .iter()
                .filter(|t| t.on)
                .map(|t| t.id.clone())
                .collect(),
            login_on: a.login.as_ref().is_some_and(|t| t.on),
            sso_on: a.sso.as_ref().is_some_and(|t| t.on),
            own_login: a.own_login.is_some(),
            public_by_design: a.public_by_design,
            open_paths: a.public_paths,
        })
        .collect();
    for (a, m) in apps.iter_mut().zip(&map_apps) {
        let bucket = super::access_map::bucket_of(m, &report.places);
        a.tags = format!("{} bucket-{bucket}", a.tags).trim().to_string();
        a.bucket = bucket;
        if bucket == "public" || bucket == "own" {
            // Open on purpose: blue, not red.
            for t in a.places.iter_mut().filter(|t| t.id == "web") {
                t.info = true;
                t.danger = false;
            }
            if let Some(t) = a.login.as_mut() {
                t.info = true;
                t.danger = false;
            }
        }
    }
    let map = super::access_map::build(&map_apps, &report.ports, &report.places, &report.ssh);
    let has = |tags: &str, id: &str| tags.split(' ').any(|t| t == id);
    let filters = FILTERS
        .iter()
        .map(|(id, label, help)| Filter {
            id,
            label,
            help,
            apps: apps.iter().filter(|a| has(&a.tags, id)).count(),
            ports: ports.ports.iter().filter(|p| has(&p.tags, id)).count(),
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
        filters,
        map,
        ssh_keys: report
            .ssh
            .keys
            .iter()
            .map(|k| key_row(k, false))
            .chain(report.ssh.machine_keys.iter().map(|k| key_row(k, true)))
            .collect(),
        ssh: report.ssh.clone(),
        ssh_keys_url: option_url("core.ssh.authorizedKeys"),
        console_password_url: option_url("core.hashedLinuxPassword"),
        login_page: lp.clone(),
        login_page_settings_url: option_url("tinyauth.users"),
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
        // The verdict above the map says it (no second warning).
        assert!(!ctx.map.ok);
        assert!(ctx.apps.iter().any(|a| a.tags.contains("bucket-open")));
    }

    /// The page renders, and reads the server logins under the names the
    /// template uses (the report's camelCase keys are only read).
    #[test]
    fn page_renders_logins_and_port_flow() {
        use rocket_dyn_templates::handlebars::Handlebars;
        let src = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/templates/access.html.hbs"
        ))
        .unwrap();
        let mut hb = Handlebars::new();
        hb.set_strict_mode(false);
        hb.register_template_string("access", src).unwrap();
        let mut r = report();
        r.ssh = serde_json::from_value(json!({
            "consolePassword": true, "passwordLogin": false, "netbirdSsh": true,
            "ports": [22], "users": ["admin"], "keys": []
        }))
        .unwrap();
        r.login_page.oidc_apps = vec!["vaultwarden".into()];
        for (port, iface, fw) in [
            (22, None, "allowed"),
            (9980, None, "docker"),
            (9982, Some("ve-streamproxy"), "interface"),
            (8080, None, "blocked"),
        ] {
            r.ports.push(ReportPort {
                protocol: "tcp".into(),
                port,
                interface: iface.map(str::to_string),
                described: true,
                label: format!("Port {port}"),
                login: "x".into(),
                firewall: fw.into(),
                ..Default::default()
            });
        }
        let ctx = build(&r, &DocumentMut::new());
        let html = hb.render("access", &ctx).unwrap();
        assert!(html.contains("Console password</dt><dd>Set"));
        assert!(html.contains("NetBird SSH</dt><dd>On"));
        assert!(html.contains("vaultwarden"));
        assert!(html.contains("Skips the firewall"));
        assert!(html.contains("Blocked by the firewall"));
        assert!(html.contains("/configuration/core/core?focus"));
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
