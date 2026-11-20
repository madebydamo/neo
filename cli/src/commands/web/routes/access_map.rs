//! The access map at the top of the Access page (access.html.hbs): how a
//! request gets into this server, as a diagram.
//!
//!   where from  →  door (ports through the firewall)  →  guard  →  inside
//!
//! Web apps share one door (the web ports), so they are grouped by how exposed
//! they are ("public websites", "internet, behind the login page", …); the
//! other doors (SSH, DNS, VPNs), one per service, carry their guard themselves.
//! The lines are drawn by static/access_map.js from `edges`; on a phone the
//! origins are tabs and every node lists where it is reached from (`origins`).
//!
//! With the rathole relay the server has no public address: the internet
//! reaches the web door through the relay and nothing else.
use serde::Serialize;

use super::access::{ReportPlaces, ReportPort, ReportSsh};
use crate::commands::web::nix::sections::humanize;

/// What the map needs of one app (computed in access.rs from settings + report).
pub(super) struct MapApp {
    pub name: String,
    pub title: String,
    pub icon: Option<String>,
    /// Places it opens from (`local`, `tailscale`, `netbird`, `web`).
    pub places: Vec<String>,
    pub login_on: bool,
    pub sso_on: bool,
    pub own_login: bool,
    /// No login on purpose (a public website).
    pub public_by_design: bool,
    pub open_paths: usize,
}

#[derive(Serialize, Debug, Default)]
pub(super) struct Verdict {
    /// error | warning | info | ok
    tone: &'static str,
    text: String,
    /// Quick filter a click turns on.
    filter: Option<String>,
}

#[derive(Serialize, Debug, Default)]
pub(super) struct Origin {
    id: String,
    /// Place id (`web`, `local`, …) for the phone tabs.
    place: &'static str,
    label: &'static str,
    detail: &'static str,
    icon: &'static str,
    inactive: bool,
}

#[derive(Serialize, Debug, Default)]
pub(super) struct Chip {
    text: String,
    /// ok | info | warning | error | muted
    tone: &'static str,
    /// Details, one line per port (tooltip).
    help: String,
}

#[derive(Serialize, Debug, Default)]
pub(super) struct Door {
    id: String,
    title: String,
    ports: String,
    /// The web door leads to the app groups; the others end here.
    web: bool,
    firewall: Chip,
    /// What stops someone at this door (not for the web door).
    guard: Option<Chip>,
    /// Place ids that reach it, space-separated (phone tabs).
    origins: String,
    /// Where from, in words.
    from: String,
}

#[derive(Serialize, Debug, Default)]
pub(super) struct GroupApp {
    name: String,
    title: String,
    icon: Option<String>,
}

#[derive(Serialize, Debug, Default)]
pub(super) struct Group {
    id: String,
    title: &'static str,
    detail: &'static str,
    /// error | warning | info | ok | muted
    tone: &'static str,
    /// Guard icon: none | public | own | login
    guard: &'static str,
    origins: String,
    from: String,
    apps: Vec<GroupApp>,
    /// "1 app" / "3 apps".
    count: String,
    filter: String,
}

#[derive(Serialize, Debug, Default, PartialEq)]
pub(super) struct Edge {
    from: String,
    to: String,
    /// ok | info | warning | error | muted
    tone: &'static str,
    dashed: bool,
}

#[derive(Serialize, Debug, Default)]
pub(super) struct AccessMap {
    verdict: Vec<Verdict>,
    /// Nothing in the verdict is a problem.
    pub(super) ok: bool,
    /// Worst tone in the verdict: error | warning | ok.
    level: &'static str,
    /// The internet reaches only the web door, through the relay.
    relay: bool,
    origins: Vec<Origin>,
    doors: Vec<Door>,
    groups: Vec<Group>,
    edges: Vec<Edge>,
}

/// Exposure group of an app; also its quick-filter tag `bucket-<group>`.
pub(super) fn bucket_of(app: &MapApp, places: &ReportPlaces) -> &'static str {
    let active: Vec<&String> = app
        .places
        .iter()
        .filter(|p| place_active(p, places))
        .collect();
    let internet = active.iter().any(|p| *p == "web");
    let guarded = app.login_on || app.sso_on;
    if active.is_empty() {
        "nowhere"
    } else if internet && guarded {
        "guarded"
    } else if internet && app.own_login {
        "own"
    } else if internet && app.public_by_design {
        "public"
    } else if internet {
        "open"
    } else if guarded || app.own_login {
        "private"
    } else {
        "private-open"
    }
}

fn place_active(id: &str, places: &ReportPlaces) -> bool {
    match id {
        "tailscale" => places.tailscale,
        "netbird" => places.netbird,
        _ => true,
    }
}

/// (id, title, detail, tone, guard icon)
const GROUPS: [(&str, &str, &str, &str, &str); 7] = [
    (
        "open",
        "Open, but should not be",
        "Its login was turned off: anyone on the internet gets in",
        "error",
        "none",
    ),
    (
        "public",
        "Public websites",
        "Open to everyone, on purpose",
        "info",
        "public",
    ),
    (
        "own",
        "Internet, the app's own login",
        "Visitors log in to the app itself",
        "info",
        "own",
    ),
    (
        "guarded",
        "Internet, behind the login page",
        "Visitors log in on the login page first",
        "ok",
        "login",
    ),
    (
        "private-open",
        "Home and VPN, no login",
        "Anyone on these networks gets in",
        "muted",
        "none",
    ),
    (
        "private",
        "Home and VPN only",
        "Not on the internet, with a login",
        "ok",
        "login",
    ),
    (
        "nowhere",
        "Not reachable",
        "Switched off for every place",
        "muted",
        "none",
    ),
];

/// (place id, label, icon)
const ORIGINS: [(&str, &str, &str); 4] = [
    ("web", "Internet", "globe"),
    ("local", "Home network", "home"),
    ("tailscale", "Tailscale", "vpn"),
    ("netbird", "NetBird", "vpn"),
];

fn origin_label(id: &str) -> &'static str {
    ORIGINS.iter().find(|o| o.0 == id).map_or("", |o| o.1)
}

fn wildcard(address: &str) -> bool {
    matches!(address.trim(), "" | "0.0.0.0" | "::" | "[::]")
}

/// Place ids that reach a port from outside. `public`: the server has a
/// public address (no relay), so a port open to every network faces the
/// internet too.
fn port_origins(p: &ReportPort, places: &ReportPlaces, public: bool) -> Vec<&'static str> {
    if p.firewall == "blocked" {
        return Vec::new();
    }
    if !wildcard(&p.address) {
        // Docker binds it to one address of this server: the home network's.
        return vec!["local"];
    }
    match p.interface.as_deref() {
        None => ORIGINS
            .iter()
            .map(|o| o.0)
            .filter(|id| place_active(id, places) && (*id != "web" || public))
            .collect(),
        Some("wt0") => vec!["netbird"],
        Some(i) if i.starts_with("tailscale") => vec!["tailscale"],
        _ => Vec::new(),
    }
}

fn is_web_port(p: &ReportPort) -> bool {
    matches!(
        p.id.as_deref(),
        Some(
            "swag-http"
                | "swag-https"
                | "streamproxy-http"
                | "streamproxy-https"
                | "streamproxy-swag"
        )
    ) || p.service.as_deref() == Some("swag")
}

fn join_ports(ps: &[&ReportPort]) -> String {
    let mut v: Vec<String> = Vec::new();
    for p in ps {
        let s = p.port.to_string();
        if !v.contains(&s) {
            v.push(s);
        }
    }
    v.join(" · ")
}

fn state_words(state: &str) -> &'static str {
    match state {
        "allowed" => "firewall lets it in",
        "interface" => "firewall: one network only",
        "docker" => "skips the firewall (Docker)",
        "blocked" => "blocked by the firewall",
        "off" => "no firewall",
        _ => "unknown",
    }
}

/// One badge for what the firewall does with a door's ports (worst first).
fn firewall_chip(ps: &[&ReportPort]) -> Chip {
    let has = |s: &str| ps.iter().any(|p| p.firewall == s);
    let vpn_only = ps.iter().all(|p| {
        p.firewall == "interface"
            && p.interface
                .as_deref()
                .is_some_and(|i| i == "wt0" || i.starts_with("tailscale"))
    });
    let (text, tone) = if has("off") {
        ("No firewall", "error")
    } else if has("docker") {
        ("Skips the firewall", "warning")
    } else if has("allowed") {
        ("Firewall open", "ok")
    } else if vpn_only {
        ("VPN only", "info")
    } else if has("interface") {
        ("This server only", "muted")
    } else if has("blocked") {
        ("Blocked", "muted")
    } else {
        ("Unknown", "muted")
    };
    let mut help = ps
        .iter()
        .map(|p| {
            format!(
                "{} {}: {}",
                p.port,
                p.protocol.to_uppercase(),
                state_words(&p.firewall)
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    if has("docker") {
        help.push_str(
            "\nDocker publishes these ports itself, before the firewall sees the traffic.",
        );
    }
    Chip {
        text: text.into(),
        tone,
        help,
    }
}

/// What stops someone at a door that is not the web door.
fn guard_chip(ps: &[&ReportPort]) -> Chip {
    let help = ps
        .iter()
        .map(|p| {
            format!(
                "{} {} ({}): {}",
                p.port,
                p.protocol.to_uppercase(),
                p.label,
                if p.described {
                    p.login.as_str()
                } else {
                    "unknown"
                }
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let guarded: Vec<&&ReportPort> = ps.iter().filter(|p| p.described && !p.open).collect();
    let (text, tone) = if ps.iter().any(|p| !p.described) {
        (
            "Unknown: Neo does not know what uses it".to_string(),
            "error",
        )
    } else if guarded.is_empty() {
        ("Answers anyone, on purpose".to_string(), "info")
    } else if guarded.len() < ps.len() {
        (
            format!("{} · some ports answer anyone", guarded[0].login),
            "ok",
        )
    } else {
        (guarded[0].login.clone(), "ok")
    };
    Chip { text, tone, help }
}

fn plural(n: usize, one: &str, many: &str) -> String {
    if n == 1 {
        format!("1 {one}")
    } else {
        format!("{n} {many}")
    }
}

fn words(origins: &[&str]) -> String {
    if origins.is_empty() {
        "Only from inside this server".into()
    } else {
        origins
            .iter()
            .map(|o| origin_label(o))
            .collect::<Vec<_>>()
            .join(", ")
    }
}

/// A door before layout: id, title, ports, web door, guard, risky for the internet.
type DoorSpec<'a> = (
    String,
    String,
    Vec<&'a ReportPort>,
    bool,
    Option<Chip>,
    bool,
);

pub(super) fn build(
    apps: &[MapApp],
    ports: &[ReportPort],
    places: &ReportPlaces,
    ssh: &ReportSsh,
) -> AccessMap {
    let public = !places.relay;
    let mut map = AccessMap {
        relay: places.relay,
        origins: ORIGINS
            .iter()
            .map(|(id, label, icon)| Origin {
                id: format!("o-{id}"),
                place: id,
                label,
                detail: match *id {
                    "web" if places.relay => "Through your relay, to the web door only",
                    "web" => "Anyone in the world",
                    "local" => "Devices at home",
                    "tailscale" => "Your Tailscale devices",
                    _ => "Your NetBird devices",
                },
                icon,
                inactive: !place_active(id, places),
            })
            .collect(),
        ..Default::default()
    };

    // --- App groups (behind the web door) ---
    for (id, title, detail, tone, guard) in GROUPS {
        let members: Vec<&MapApp> = apps.iter().filter(|a| bucket_of(a, places) == id).collect();
        if members.is_empty() {
            continue;
        }
        let from: Vec<&str> = ORIGINS
            .iter()
            .map(|o| o.0)
            .filter(|oid| {
                place_active(oid, places)
                    && members.iter().any(|a| a.places.iter().any(|p| p == oid))
            })
            .collect();
        map.groups.push(Group {
            id: format!("g-{id}"),
            title,
            detail,
            tone,
            guard,
            origins: from.join(" "),
            from: if from.is_empty() {
                "nowhere".into()
            } else {
                words(&from)
            },
            apps: members
                .iter()
                .map(|a| GroupApp {
                    name: a.name.clone(),
                    title: a.title.clone(),
                    icon: a.icon.clone(),
                })
                .collect(),
            count: plural(members.len(), "app", "apps"),
            filter: format!("bucket-{id}"),
        });
    }
    let count_of = |id: &str| {
        let gid = format!("g-{id}");
        map.groups
            .iter()
            .find(|g| g.id == gid)
            .map_or(0, |g| g.apps.len())
    };
    let open_apps = count_of("open");
    let public_apps = count_of("public");

    // --- Doors: web, SSH, then one per service ---
    let web: Vec<&ReportPort> = ports.iter().filter(|p| is_web_port(p)).collect();
    let ssh_ports: Vec<&ReportPort> = ports
        .iter()
        .filter(|p| p.id.as_deref() == Some("ssh"))
        .collect();
    let mut others: Vec<(String, Vec<&ReportPort>)> = Vec::new();
    for p in ports
        .iter()
        .filter(|p| !is_web_port(p) && p.id.as_deref() != Some("ssh"))
    {
        let key = p.service.clone().unwrap_or_else(|| p.label.clone());
        match others.iter_mut().find(|(k, _)| *k == key) {
            Some((_, v)) => v.push(p),
            None => others.push((key, vec![p])),
        }
    }

    let mut doors: Vec<DoorSpec> = Vec::new();
    if !web.is_empty() {
        doors.push(("d-web".into(), "Web".into(), web, true, None, open_apps > 0));
    }
    if !ssh_ports.is_empty() {
        let guard = if ssh.password_login {
            Chip {
                text: "Password: no SSH key set".into(),
                tone: "error",
                help: String::new(),
            }
        } else {
            Chip {
                text: format!("Only {}", plural(ssh.keys.len(), "SSH key", "SSH keys")),
                tone: "ok",
                help: format!("Server shell for {}", ssh.users.join(", ")),
            }
        };
        doors.push((
            "d-ssh".into(),
            "SSH".into(),
            ssh_ports.clone(),
            false,
            Some(guard),
            ssh.password_login,
        ));
    }
    for (i, (key, ps)) in others.into_iter().enumerate() {
        let title = if ps.iter().all(|p| p.label == ps[0].label) && ps[0].described {
            ps[0].label.clone()
        } else if ps[0].service.is_some() {
            humanize(&key)
        } else {
            "Unknown port".into()
        };
        let guard = guard_chip(&ps);
        let risky = guard.tone == "error";
        doors.push((format!("d-{i}"), title, ps, false, Some(guard), risky));
    }

    for (id, title, ps, is_web, guard, risky) in doors {
        let mut from: Vec<&'static str> = Vec::new();
        for p in &ps {
            for o in port_origins(p, places, public) {
                if !from.contains(&o) {
                    from.push(o);
                }
            }
        }
        // The relay brings the internet to the web door, whatever the firewall.
        if is_web && !public && !from.contains(&"web") {
            from.insert(0, "web");
        }
        for o in &from {
            let skips = ps.iter().any(|p| {
                (p.firewall == "docker" || p.firewall == "off")
                    && port_origins(p, places, public).contains(o)
            });
            let (tone, dashed) = if *o == "web" && risky {
                ("error", true)
            } else if *o == "web" {
                ("ok", false)
            } else if skips {
                ("warning", true)
            } else {
                ("muted", false)
            };
            map.edges.push(Edge {
                from: format!("o-{o}"),
                to: id.clone(),
                tone,
                dashed,
            });
        }
        if is_web {
            for g in &map.groups {
                map.edges.push(Edge {
                    from: id.clone(),
                    to: g.id.clone(),
                    tone: g.tone,
                    dashed: g.tone == "error",
                });
            }
        }
        map.doors.push(Door {
            id,
            title,
            ports: join_ports(&ps),
            web: is_web,
            firewall: firewall_chip(&ps),
            guard,
            origins: from.join(" "),
            from: words(&from),
        });
    }

    // --- Verdict ---
    let mut v = Vec::new();
    if ports.iter().any(|p| p.firewall == "off") {
        v.push(Verdict {
            tone: "error",
            text: "The firewall is off: every port answers every network.".into(),
            filter: None,
        });
    }
    if open_apps > 0 {
        v.push(Verdict {
            tone: "error",
            text: format!(
                "{} on the internet without any login, although {} should have one.",
                plural(open_apps, "app is", "apps are"),
                if open_apps == 1 { "it" } else { "they" }
            ),
            filter: Some("bucket-open".into()),
        });
    }
    let unknown: Vec<String> = ports
        .iter()
        .filter(|p| !p.described)
        .map(|p| p.port.to_string())
        .collect();
    if !unknown.is_empty() {
        v.push(Verdict {
            tone: "error",
            text: format!(
                "Port {} is open, but Neo does not know what uses it.",
                unknown.join(", ")
            ),
            filter: Some("risk".into()),
        });
    }
    if ssh.password_login && !ssh_ports.is_empty() {
        v.push(Verdict {
            tone: "error",
            text: "SSH accepts passwords, because no SSH key is set.".into(),
            filter: None,
        });
    }
    let paths = apps
        .iter()
        .filter(|a| bucket_of(a, places) == "guarded" && a.login_on && a.open_paths > 0)
        .count();
    if paths > 0 {
        v.push(Verdict {
            tone: "warning",
            text: format!(
                "{} on the internet {} addresses that skip the login page (share links, phone apps).",
                plural(paths, "app", "apps"),
                if paths == 1 { "has" } else { "have" }
            ),
            filter: Some("openpaths".into()),
        });
    }
    let problems = v.iter().any(|x| x.tone == "error" || x.tone == "warning");
    map.level = if v.iter().any(|x| x.tone == "error") {
        "error"
    } else if problems {
        "warning"
    } else {
        "ok"
    };
    map.ok = !problems;
    if map.ok {
        v.insert(
            0,
            Verdict {
                tone: "ok",
                text: "Nothing opens to the internet without a login, except what you publish on purpose."
                    .into(),
                filter: None,
            },
        );
    }
    if public_apps > 0 {
        v.push(Verdict {
            tone: "info",
            text: format!(
                "{} open to everyone on purpose.",
                plural(public_apps, "public website is", "public websites are")
            ),
            filter: Some("bucket-public".into()),
        });
    }
    if places.relay {
        v.push(Verdict {
            tone: "info",
            text:
                "The internet reaches this server only through your relay, and only its web door."
                    .into(),
            filter: None,
        });
    }
    map.verdict = v;
    map
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(name: &str, places: &[&str], login: bool, own: bool) -> MapApp {
        MapApp {
            name: name.into(),
            title: name.into(),
            icon: None,
            places: places.iter().map(|s| s.to_string()).collect(),
            login_on: login,
            sso_on: false,
            own_login: own,
            public_by_design: false,
            open_paths: 0,
        }
    }

    fn port(
        id: &str,
        service: Option<&str>,
        port: u32,
        iface: Option<&str>,
        firewall: &str,
    ) -> ReportPort {
        ReportPort {
            id: Some(id.into()),
            protocol: "tcp".into(),
            port,
            interface: iface.map(str::to_string),
            described: true,
            label: id.into(),
            service: service.map(str::to_string),
            login: "x".into(),
            firewall: firewall.into(),
            ..Default::default()
        }
    }

    #[test]
    fn apps_are_grouped_by_exposure() {
        let p = ReportPlaces::default();
        assert_eq!(
            bucket_of(&app("a", &["web", "local"], false, false), &p),
            "open"
        );
        let mut site = app("a", &["web"], false, false);
        site.public_by_design = true;
        assert_eq!(bucket_of(&site, &p), "public");
        assert_eq!(bucket_of(&app("a", &["web"], false, true), &p), "own");
        assert_eq!(bucket_of(&app("a", &["web"], true, false), &p), "guarded");
        assert_eq!(
            bucket_of(&app("a", &["local"], false, false), &p),
            "private-open"
        );
        assert_eq!(bucket_of(&app("a", &["local"], true, false), &p), "private");
        // Tailscale is not set up here: the app opens from nowhere.
        assert_eq!(
            bucket_of(&app("a", &["tailscale"], true, false), &p),
            "nowhere"
        );
    }

    #[test]
    fn behind_a_relay_the_internet_reaches_only_the_web_door() {
        let places = ReportPlaces {
            relay: true,
            netbird: true,
            ..Default::default()
        };
        let mut dns = port("pihole-dns-tcp", Some("pihole"), 53, None, "allowed");
        dns.address = "192.168.1.5".into();
        dns.open = true;
        let ports = vec![
            port("ssh", None, 22, None, "allowed"),
            port("swag-https", Some("swag"), 443, None, "docker"),
            dns,
            port("pihole-dns-udp", Some("pihole"), 53, None, "allowed"),
            port("netbird-wg", Some("netbird"), 51820, None, "allowed"),
            port(
                "netbird-dns",
                Some("netbird"),
                5353,
                Some("wt0"),
                "interface",
            ),
        ];
        let ssh = ReportSsh {
            keys: vec!["k".into()],
            ..Default::default()
        };
        let m = build(
            &[app("immich", &["web", "local"], true, false)],
            &ports,
            &places,
            &ssh,
        );
        let titles: Vec<&str> = m.doors.iter().map(|d| d.title.as_str()).collect();
        assert_eq!(titles, ["Web", "SSH", "Pihole", "Netbird"]);
        let web_edges: Vec<&str> = m
            .edges
            .iter()
            .filter(|e| e.from == "o-web")
            .map(|e| e.to.as_str())
            .collect();
        assert_eq!(web_edges, ["d-web"]);
        // DNS bound to the home address: the home network reaches it.
        assert!(m.edges.iter().any(|e| e.from == "o-local" && e.to == "d-0"));
        assert_eq!(m.doors[2].origins, "local netbird");
        assert_eq!(
            m.doors[2].guard.as_ref().unwrap().text,
            "x · some ports answer anyone"
        );
        assert_eq!(m.doors[3].firewall.text, "Firewall open");
        assert_eq!(m.doors[0].firewall.text, "Skips the firewall");
        assert!(m.ok);
    }

    #[test]
    fn a_login_turned_off_is_the_only_red() {
        let ssh = ReportSsh {
            keys: vec!["k".into()],
            ..Default::default()
        };
        let mut site = app("shop", &["web"], false, false);
        site.public_by_design = true;
        let apps = vec![
            site,
            app("pastebin", &["web"], false, false),
            app("filebrowser", &["web"], false, true),
        ];
        let m = build(
            &apps,
            &[port("swag-https", Some("swag"), 443, None, "allowed")],
            &ReportPlaces::default(),
            &ssh,
        );
        assert!(!m.ok);
        assert_eq!(m.level, "error");
        assert_eq!(
            m.verdict[0].text,
            "1 app is on the internet without any login, although it should have one."
        );
        assert!(m
            .verdict
            .iter()
            .any(|v| v.text == "1 public website is open to everyone on purpose."));
        assert!(m.edges.contains(&Edge {
            from: "o-web".into(),
            to: "d-web".into(),
            tone: "error",
            dashed: true,
        }));
        let groups: Vec<&str> = m.groups.iter().map(|g| g.id.as_str()).collect();
        assert_eq!(groups, ["g-open", "g-public", "g-own"]);
    }
}
