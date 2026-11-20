//! Rescue portal of the backup store (docs/design/backup-store-rescue.md §3–4).
//!
//! SWAG sends a browser on the store's own address here with
//! `X-Neo-Site: backup-store` (nix/services/backup-store/swag.nix). On that
//! site [`PortalSite`] serves the portal at `/` and answers 404 for every route
//! the portal does not use. The same pages are at `/backup-store/portal` in the
//! normal web UI.
//!
//! Data: `report.json` of `neo backup-store report` (the web UI cannot read the
//! client homes). A rescue runs `neo backup-store rescue` as root in a
//! transient unit; the admin key reaches it through a 0600 file in
//! /run/neo-web, which the job moves into its own folder and shreds.

use std::fs;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;

use rocket::fairing::{Fairing, Info, Kind};
use rocket::form::{Form, FromForm};
use rocket::http::uri::Origin;
use rocket::http::Status;
use rocket::request::{FromRequest, Outcome};
use rocket::response::content::RawHtml;
use rocket::{get, post, routes, Data, Request};
use rocket_dyn_templates::Template;
use serde_json::{json, Value};

use super::locks::probe;
use super::ops::monitor::monitor_fragment;
use super::ops::store::{append_log, log_path};
use super::util::{self, alert_html, AlertKind};
use crate::commands::backup_store::{
    client_name_ok, now, rescue, ClientReport, KeyInfo, Report, StoreConfig, CURRENT_SECS,
    RECENT_CONNECTION_SECS, STALE_SECS,
};
use crate::utils::command::{get_timestamp, output_checked, sudo_command};
use crate::utils::locks::OpInfo;
use crate::utils::ops::{OpStatus, OperationKind, OperationLog};

/// Header SWAG sets on the store's address.
pub const SITE_HEADER: &str = "X-Neo-Site";
const SITE: &str = "backup-store";
const BASE: &str = "/backup-store/portal";
/// The admin key's way to the rescue job (neo-web's RuntimeDirectory, 0700).
const HANDOFF_DIR: &str = "/run/neo-web";
const NEO_FLAKE: &str = "github:madebydamo/neo";

pub fn routes() -> Vec<rocket::Route> {
    routes![portal, portal_client, portal_rescue, portal_not_found]
}

/// On the store's address: the portal and nothing else of the web UI.
pub struct PortalSite;

#[rocket::async_trait]
impl Fairing for PortalSite {
    fn info(&self) -> Info {
        Info {
            name: "backup-store portal site",
            kind: Kind::Request,
        }
    }

    async fn on_request(&self, req: &mut Request<'_>, _: &mut Data<'_>) {
        if req.headers().get_one(SITE_HEADER) != Some(SITE) {
            return;
        }
        if let Some(to) = site_path(req.uri().path().as_str()) {
            if let Ok(uri) = Origin::parse_owned(to) {
                req.set_uri(uri);
            }
        }
    }
}

/// Where a path on the store's address goes: None keeps it (query included).
fn site_path(path: &str) -> Option<String> {
    const ALLOWED: &[&str] = &["/static/", "/ws/op/rescue_", "/op/monitor/rescue_"];
    if path == "/" || path.is_empty() {
        return Some(BASE.to_string());
    }
    if path == BASE
        || path.starts_with(&format!("{BASE}/"))
        || ALLOWED.iter().any(|p| path.starts_with(p))
    {
        return None;
    }
    Some(format!("{BASE}/not-found"))
}

/// Whether the request came in on the store's own address.
pub struct OnStoreSite(bool);

#[rocket::async_trait]
impl<'r> FromRequest<'r> for OnStoreSite {
    type Error = ();
    async fn from_request(req: &'r Request<'_>) -> Outcome<Self, ()> {
        Outcome::Success(OnStoreSite(
            req.headers().get_one(SITE_HEADER) == Some(SITE),
        ))
    }
}

#[get("/backup-store/portal/not-found")]
fn portal_not_found() -> Status {
    Status::NotFound
}

/// What the page may show: on the store's address without a login, only the
/// general help (no client names, keys or sizes), and no rescue.
fn private_ok(site: &OnStoreSite, report: &Report) -> bool {
    !site.0 || report.store.login
}

fn load() -> Result<Report, String> {
    match Report::load() {
        Ok(r) => Ok(r),
        Err(_) if StoreConfig::load().is_err() => {
            Err("The backup store is not on on this server.".into())
        }
        Err(_) => Err("The first report of the stored backups is not ready yet. It is made every 15 minutes; reload in a minute.".into()),
    }
}

#[get("/backup-store/portal")]
fn portal(site: OnStoreSite) -> Template {
    let report = match load() {
        Ok(r) => r,
        Err(msg) => return Template::render("backup_store/portal", json!({ "error": msg })),
    };
    let private = private_ok(&site, &report);
    let t = now();
    let clients: Vec<Value> = if private {
        report.clients.iter().map(|c| client_card(c, t)).collect()
    } else {
        Vec::new()
    };
    Template::render(
        "backup_store/portal",
        json!({
            "store": store_view(&report),
            "private": private,
            "clients": clients,
            "has_clients": !clients.is_empty(),
            "report_age": ago(t - report.generated),
        }),
    )
}

#[get("/backup-store/portal/client/<name>")]
fn portal_client(name: &str, site: OnStoreSite) -> Option<Template> {
    let report = load().ok()?;
    if !private_ok(&site, &report) {
        return None;
    }
    let c = report.client(name)?;
    let t = now();
    let can_rescue = c.configured && rescue_allowed(&site, &report);
    let folder = c.folders.first().cloned().unwrap_or_else(|| ".".into());
    let store = StoreConfig::load().ok();
    let url = store
        .as_ref()
        .map(|s| s.backup_url(&c.name, &folder))
        .unwrap_or_default();
    let recent = c
        .last_connection
        .filter(|l| t - l < RECENT_CONNECTION_SECS)
        .map(|l| ago(t - l));
    Some(Template::render(
        "backup_store/client",
        json!({
            "title": format!("Restore {}", c.name),
            "store": store_view(&report),
            "client": client_card(c, t),
            "zfs": c.format == "zfs",
            "encrypted": c.format == "encrypted",
            "backup_url": url,
            "folders": c.folders,
            "many_folders": c.folders.len() > 1,
            "git_remote": c.git_remote,
            "recent_connection": recent,
            "can_rescue": can_rescue,
            "rescue_off_reason": if !c.configured {
                "This server is no longer in the store's settings."
            } else if !rescue_allowed(&site, &report) {
                "Turn on the login of this page (Backup store → Require login) to restore from here."
            } else { "" },
            "cmd_computer": command_computer(c, &url),
            "cmd_nixos": command_nixos(c, &url),
        }),
    ))
}

/// Starting a rescue needs the portal's login on the store's address.
fn rescue_allowed(site: &OnStoreSite, report: &Report) -> bool {
    !site.0 || report.store.login
}

fn store_view(report: &Report) -> Value {
    let host = report
        .store
        .url
        .trim_start_matches("https://")
        .trim_end_matches('/')
        .to_string();
    json!({
        "url": report.store.url,
        "host": host,
        "internet": report.store.internet,
        "login": report.store.login,
    })
}

fn client_card(c: &ClientReport, t: i64) -> Value {
    let (state, label) = match c.last_backup {
        None if c.versions == Some(0) || c.folders.is_empty() => {
            ("none", "No backup yet".to_string())
        }
        None => ("none", "Last backup unknown".to_string()),
        Some(b) => {
            let age = t - b;
            let state = if age <= CURRENT_SECS {
                "ok"
            } else if age <= STALE_SECS {
                "late"
            } else {
                "stale"
            };
            (state, format!("Backed up {}", ago(age)))
        }
    };
    let format = match c.format.as_str() {
        "files" => "Plain files",
        "encrypted" => "Encrypted",
        "zfs" => "ZFS snapshots",
        _ => "Unknown format",
    };
    json!({
        "name": c.name,
        "configured": c.configured,
        "format": format,
        "state": state,
        "state_ok": state == "ok",
        "state_late": state == "late",
        "state_bad": state == "stale" || state == "none",
        "state_label": label,
        "estimated": c.estimated && c.last_backup.is_some(),
        "versions": c.versions.filter(|v| *v > 1),
        "size": c.size_bytes.map(size),
        "last_connection": c.last_connection.map(|l| ago(t - l)),
        "keys": c.keys,
        // Encrypted clients: what each key may do (recipients.rs).
        "roles_known": c.keys.iter().any(|k| !k.role.is_empty()),
        "unlock_keys": keys_with(c, "unlock"),
        "server_keys": keys_with(c, "server"),
        "access_keys": keys_with(c, "access"),
        "host": c.host,
    })
}

fn keys_with<'a>(c: &'a ClientReport, role: &str) -> Vec<&'a KeyInfo> {
    c.keys.iter().filter(|k| k.role == role).collect()
}

fn command_computer(c: &ClientReport, url: &str) -> String {
    if c.format == "zfs" {
        let repo = c.git_remote.as_deref().unwrap_or("<your config repo>");
        return format!(
            "nix run {NEO_FLAKE} -- restore {} \\\n  --host root@<new IP> --key ~/.ssh/id_ed25519",
            shell_arg(repo)
        );
    }
    format!("nix run {NEO_FLAKE} -- restore \\\n  --from-backup {url} \\\n  --host root@<new IP> --key ~/.ssh/id_ed25519")
}

fn command_nixos(c: &ClientReport, url: &str) -> String {
    let nix = "nix --extra-experimental-features 'nix-command flakes'";
    if c.format == "zfs" {
        let repo = c.git_remote.as_deref().unwrap_or("<your config repo>");
        return format!(
            "{nix} run {NEO_FLAKE} -- restore {} \\\n  --key /root/admin_key --shred-key",
            shell_arg(repo)
        );
    }
    format!("{nix} run {NEO_FLAKE} -- restore \\\n  --from-backup {url} \\\n  --key /root/admin_key --shred-key")
}

/// Quote for the shown command when the value needs it. A placeholder such as
/// `<your config repo>` stays as it is: the reader replaces it.
fn shell_arg(s: &str) -> String {
    let plain = s
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || "@%+=:,./-_~".contains(c));
    if plain || (s.starts_with('<') && s.ends_with('>')) {
        s.to_string()
    } else {
        format!("'{}'", s.replace('\'', r"'\''"))
    }
}

/// "5 minutes ago", "3 hours ago", "2 days ago".
pub fn ago(secs: i64) -> String {
    let secs = secs.max(0);
    let (n, unit) = if secs < 120 {
        return "just now".into();
    } else if secs < 7200 {
        (secs / 60, "minute")
    } else if secs < 2 * 86400 {
        (secs / 3600, "hour")
    } else {
        (secs / 86400, "day")
    };
    format!("{n} {unit}{} ago", if n == 1 { "" } else { "s" })
}

/// Bytes as people read them (powers of 1000).
pub fn size(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "kB", "MB", "GB", "TB", "PB"];
    let mut v = bytes as f64;
    let mut u = 0;
    while v >= 1000.0 && u < UNITS.len() - 1 {
        v /= 1000.0;
        u += 1;
    }
    if u == 0 || v >= 100.0 {
        format!("{:.0} {}", v, UNITS[u])
    } else {
        format!("{:.1} {}", v, UNITS[u])
    }
}

#[derive(FromForm)]
struct RescueForm {
    host: String,
    disk: Option<String>,
    folder: Option<String>,
    key: String,
    passphrase: Option<String>,
    /// "check" or "restore".
    action: String,
    /// The client's name, typed to confirm a restore.
    confirm: Option<String>,
}

#[post("/backup-store/portal/client/<name>/rescue", data = "<form>")]
fn portal_rescue(name: &str, site: OnStoreSite, form: Form<RescueForm>) -> RawHtml<String> {
    let form = form.into_inner();
    match start_rescue(name, &site, &form) {
        Ok(html) => RawHtml(html),
        Err(msg) => RawHtml(alert_html(AlertKind::Error, &msg)),
    }
}

fn start_rescue(name: &str, site: &OnStoreSite, form: &RescueForm) -> Result<String, String> {
    let report = load()?;
    if !rescue_allowed(site, &report) {
        return Err("Restoring from here needs the login of this page.".into());
    }
    let store = StoreConfig::load().map_err(|e| format!("{e:#}"))?;
    if !client_name_ok(name) || !store.clients.contains_key(name) {
        return Err(format!("{name} is not a client of this store."));
    }
    let check = match form.action.as_str() {
        "check" => true,
        "restore" => false,
        _ => return Err("Unknown action.".into()),
    };
    if !check && form.confirm.as_deref().map(str::trim) != Some(name) {
        return Err(format!(
            "Type {name} to confirm: the new machine's disk is erased."
        ));
    }
    let host = form.host.trim();
    if !host_ok(host) {
        return Err("Give the new machine as root@IP or root@IP:port.".into());
    }
    let disk = opt(&form.disk);
    if disk.is_some_and(|d| !d.starts_with("/dev/") || !plain(d)) {
        return Err("The disk is a path like /dev/disk/by-id/… or /dev/vda.".into());
    }
    let folder = opt(&form.folder);
    if folder.is_some_and(|f| !plain(f) || f.split('/').any(|p| p == "..") || f.starts_with('-')) {
        return Err("Unknown backup folder.".into());
    }
    let key = normalize_key(&form.key).ok_or(
        "Paste the private key: the text from -----BEGIN OPENSSH PRIVATE KEY----- to the END line.",
    )?;
    let passphrase = opt(&form.passphrase);

    probe(
        &rescue::lock_spec(name),
        OpInfo::new("rescue", format!("Rescue of {name}")),
    )
    .map_err(|b| b.0)?;

    let ts = get_timestamp();
    let op = OperationLog::new(OperationKind::Rescue, &ts);
    let suffix = op.suffix().to_string();
    let key_file = handoff_file(&suffix, "key", key.as_bytes())?;
    let pass_file = match passphrase {
        Some(p) => Some(
            handoff_file(&suffix, "pass", p.as_bytes()).inspect_err(|_| {
                shred(&key_file);
            })?,
        ),
        None => None,
    };
    op.init_for_web_trigger(&ts);

    let mut args: Vec<String> = vec![
        "backup-store".into(),
        "rescue".into(),
        name.into(),
        "--host".into(),
        host.into(),
        "--key-file".into(),
        key_file.display().to_string(),
        "--op".into(),
        suffix.clone(),
    ];
    if let Some(p) = &pass_file {
        args.extend(["--passphrase-file".into(), p.display().to_string()]);
    }
    if let Some(d) = disk {
        args.extend(["--disk".into(), d.into()]);
    }
    if let Some(f) = folder {
        args.extend(["--folder".into(), f.into()]);
    }
    if check {
        args.push("--check".into());
    }
    if let Err(e) = spawn_rescue(name, &suffix, &args) {
        shred(&key_file);
        if let Some(p) = &pass_file {
            shred(p);
        }
        let msg = format!("{e:#}");
        append_log(&log_path(op.id()), &msg);
        op.write_state(OpStatus::Failed, "start", Some(&msg), None);
    }
    let note = if check {
        "The check boots the new machine into the NixOS installer and changes nothing on its disk. You may close this page."
    } else {
        "The new machine's disk is erased, then the server is restored and reboots. You may close this page: the restore keeps running."
    };
    Ok(monitor_fragment(op.id(), Some(note)))
}

/// The job as root in its own unit, with a private RuntimeDirectory for the keys.
fn spawn_rescue(name: &str, suffix: &str, args: &[String]) -> anyhow::Result<()> {
    let unit = format!("neo-rescue-{name}-{suffix}.service");
    let mut cmd = sudo_command();
    cmd.args([
        "systemd-run",
        "--collect",
        "--no-ask-password",
        "--no-block",
        "--unit",
        &unit,
        "--service-type=oneshot",
        "-p",
        &format!("RuntimeDirectory=neo-rescue-{suffix}"),
        "-p",
        "RuntimeDirectoryMode=0700",
        "-p",
        &format!("Description=Neo rescue of {name}"),
        "-E",
        "HOME=/root",
        "-E",
        &format!("NIX_BINARY_PATH={}", util::nix_bin()),
        "-E",
        "PATH=/run/current-system/sw/bin:/usr/bin:/bin",
        &util::neo_bin(),
    ]);
    cmd.args(args);
    output_checked(&mut cmd, "systemd-run")?;
    Ok(())
}

/// `root@host` or `root@host:port`, nothing a shell or ssh would read as more.
fn host_ok(h: &str) -> bool {
    let Some((user, rest)) = h.split_once('@') else {
        return false;
    };
    user == "root"
        && !rest.is_empty()
        && !rest.starts_with('-')
        && rest
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || ".-:[]".contains(c))
}

fn plain(s: &str) -> bool {
    s.chars()
        .all(|c| c.is_ascii_alphanumeric() || "/._-:".contains(c))
}

fn opt(s: &Option<String>) -> Option<&str> {
    s.as_deref().map(str::trim).filter(|s| !s.is_empty())
}

/// An OpenSSH private key with Unix line ends and a final newline.
fn normalize_key(text: &str) -> Option<String> {
    let k = text.replace("\r\n", "\n").replace('\r', "\n");
    let k = k.trim();
    (k.starts_with("-----BEGIN ") && k.contains("PRIVATE KEY-----") && k.ends_with("-----"))
        .then(|| format!("{k}\n"))
}

/// A 0600 file in neo-web's private runtime folder; the job takes it over.
fn handoff_file(suffix: &str, what: &str, content: &[u8]) -> Result<PathBuf, String> {
    let path = PathBuf::from(HANDOFF_DIR).join(format!("rescue-{suffix}.{what}"));
    let mut f = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)
        .map_err(|e| format!("cannot hand the key to the job ({}): {e}", path.display()))?;
    f.write_all(content)
        .map_err(|e| format!("cannot hand the key to the job: {e}"))?;
    Ok(path)
}

fn shred(path: &PathBuf) {
    if let Ok(meta) = fs::metadata(path) {
        let _ = fs::write(path, vec![0u8; meta.len() as usize]);
    }
    let _ = fs::remove_file(path);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_store_site_only_reaches_the_portal() {
        assert_eq!(site_path("/").as_deref(), Some(BASE));
        for keep in [
            BASE,
            "/backup-store/portal/client/brugg",
            "/static/neo-ui.css",
            "/ws/op/rescue_20261119-120000",
            "/op/monitor/rescue_20261119-120000",
        ] {
            assert_eq!(site_path(keep), None, "{keep}");
        }
        for deny in [
            "/configuration",
            "/backup-store/portalx",
            "/ws/op/activation_1",
            "/ws/status",
            "/secrets/card",
            "/mcp",
        ] {
            assert_eq!(
                site_path(deny).as_deref(),
                Some("/backup-store/portal/not-found"),
                "{deny}"
            );
        }
    }

    #[test]
    fn hosts_and_keys() {
        assert!(host_ok("root@203.0.113.7"));
        assert!(host_ok("root@203.0.113.7:2222"));
        assert!(host_ok("root@[2001:db8::1]"));
        assert!(host_ok("root@vps.example.com"));
        for bad in [
            "203.0.113.7",
            "admin@x",
            "root@",
            "root@-oProxyCommand=x",
            "root@a b",
            "root@a;b",
        ] {
            assert!(!host_ok(bad), "{bad}");
        }
        let k = normalize_key("\r\n-----BEGIN OPENSSH PRIVATE KEY-----\r\nabc\r\n-----END OPENSSH PRIVATE KEY-----\r\n  ").unwrap();
        assert_eq!(
            k,
            "-----BEGIN OPENSSH PRIVATE KEY-----\nabc\n-----END OPENSSH PRIVATE KEY-----\n"
        );
        assert!(normalize_key("ssh-ed25519 AAAA me").is_none());
    }

    #[test]
    fn people_read_ages_and_sizes() {
        assert_eq!(ago(30), "just now");
        assert_eq!(ago(300), "5 minutes ago");
        assert_eq!(ago(5 * 3600), "5 hours ago");
        assert_eq!(ago(3 * 86400), "3 days ago");
        assert_eq!(size(512), "512 B");
        assert_eq!(size(182_400_000_000), "182 GB");
        assert_eq!(size(1_500_000), "1.5 MB");
    }

    #[test]
    fn commands_for_each_format() {
        let mut c = ClientReport {
            name: "brugg".into(),
            format: "encrypted".into(),
            ..Default::default()
        };
        let url = "https://brugg@backup.example.com/backups";
        assert!(command_computer(&c, url)
            .contains("--from-backup https://brugg@backup.example.com/backups"));
        assert!(command_nixos(&c, url).contains("--key /root/admin_key"));
        c.format = "zfs".into();
        c.git_remote = Some("git@github.com:me/conf.git".into());
        let z = command_computer(&c, url);
        assert!(z.contains("restore git@github.com:me/conf.git") && !z.contains("--from-backup"));
    }

    fn render(name: &str, ctx: &Value) -> String {
        use rocket_dyn_templates::handlebars::Handlebars;
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/templates/");
        let mut hb = Handlebars::new();
        hb.set_strict_mode(false);
        for (t, f) in [
            ("partials/head_meta", "partials/head_meta.html.hbs"),
            (
                "partials/vendor_scripts",
                "partials/vendor_scripts.html.hbs",
            ),
            (name, &format!("{name}.html.hbs")),
        ] {
            hb.register_template_string(t, std::fs::read_to_string(format!("{dir}{f}")).unwrap())
                .unwrap();
        }
        hb.render(name, ctx).unwrap()
    }

    fn report() -> ClientReport {
        ClientReport {
            name: "brugg".into(),
            format: "encrypted".into(),
            configured: true,
            keys: vec![crate::commands::backup_store::KeyInfo {
                label: "damian@laptop".into(),
                fingerprint: "SHA256:abc".into(),
                role: String::new(),
            }],
            last_backup: Some(1000),
            versions: Some(14),
            size_bytes: Some(182_000_000_000),
            folders: vec!["backups".into()],
            ..Default::default()
        }
    }

    #[test]
    fn portal_lists_the_clients() {
        let html = render(
            "backup_store/portal",
            &json!({
                "store": {"host": "backup.example.com", "internet": true, "login": true},
                "private": true,
                "has_clients": true,
                "clients": [client_card(&report(), 1000 + 5 * 3600)],
                "report_age": "just now",
            }),
        );
        assert!(html.contains("Backed up 5 hours ago"));
        assert!(html.contains("badge-success"));
        assert!(html.contains("14 versions · 182 GB"));
        assert!(html.contains(r#"href="/backup-store/portal/client/brugg""#));
        assert!(html.contains("damian@laptop"));
        let public = render(
            "backup_store/portal",
            &json!({"store": {"host": "b"}, "private": false}),
        );
        assert!(!public.contains("brugg") && public.contains("Require login"));
    }

    #[test]
    fn client_page_has_three_ways() {
        let c = report();
        let url = "https://brugg@backup.example.com/backups";
        let ctx = |can: bool, internet: bool| {
            json!({
                "title": "Restore brugg",
                "store": {"host": "backup.example.com", "internet": internet, "login": true},
                "client": client_card(&c, 1000 + 3600),
                "encrypted": true,
                "backup_url": url,
                "folders": c.folders,
                "recent_connection": "5 minutes ago",
                "can_rescue": can,
                "rescue_off_reason": "Turn on the login",
                "cmd_computer": command_computer(&c, url),
                "cmd_nixos": command_nixos(&c, url),
            })
        };
        let html = render("backup_store/client", &ctx(true, true));
        for want in [
            "<title>Restore brugg</title>",
            "From this server",
            "From your computer",
            "From a NixOS machine",
            r#"hx-post="/backup-store/portal/client/brugg/rescue""#,
            "SHA256:abc",
            "connected 5 minutes ago",
            "System → SSH on brugg",
            "open to the internet",
            "--from-backup https://brugg@backup.example.com/backups",
        ] {
            assert!(html.contains(want), "{want}");
        }
        let html = render("backup_store/client", &ctx(false, false));
        assert!(!html.contains("/rescue\""));
        assert!(!html.contains(r#"hx-post="#));
        assert!(html.contains("Turn on the login"));
        assert!(html.contains("not open to the internet"));
    }
}
