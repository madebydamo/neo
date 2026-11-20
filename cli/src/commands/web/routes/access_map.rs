//! The access map at the top of the Access page (access.html.hbs): how a
//! request gets into this server, as a diagram.
//!
//!   where from  →  door (ports through the firewall)  →  guard  →  inside
//!
//! Web apps share one door (the web ports), so they are grouped by how exposed
//! they are ("anyone on the internet", "internet, behind the login page", …);
//! the other doors (SSH, DNS, VPNs) carry their guard themselves. The lines
//! between the nodes are drawn by static/access_map.js from `edges`.
//! The verdict above the map says in words what is open.
use serde::Serialize;

use super::access::{ReportPlaces, ReportPort, ReportSsh};

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
    pub open_paths: usize,
}

#[derive(Serialize, Debug, Default)]
pub(super) struct Verdict {
    /// error | warning | ok
    tone: &'static str,
    text: String,
    /// Quick filter a click turns on.
    filter: Option<String>,
}

#[derive(Serialize, Debug, Default)]
pub(super) struct Origin {
    id: String,
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
}

#[derive(Serialize, Debug, Default)]
pub(super) struct Door {
    id: String,
    label: String,
    ports: String,
    /// The web door leads to the app groups; the others end here.
    web: bool,
    firewall: Vec<Chip>,
    /// What stops someone at this door (not for the web door).
    guard: Option<Chip>,
    /// Who or what is behind it ("Server shell: admin, homeserver").
    inside: Option<String>,
    /// Where from, in words (narrow screens draw no lines).
    from: String,
    /// Filter for the ports list.
    filter: &'static str,
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
    /// error | warning | ok | muted
    tone: &'static str,
    /// Guard icon: none | own | login
    guard: &'static str,
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
    match (active.is_empty(), internet, guarded, app.own_login) {
        (true, ..) => "nowhere",
        (_, true, false, false) => "open",
        (_, true, false, true) => "own",
        (_, true, true, _) => "guarded",
        (_, false, false, false) => "private-open",
        _ => "private",
    }
}

fn place_active(id: &str, places: &ReportPlaces) -> bool {
    match id {
        "tailscale" => places.tailscale,
        "netbird" => places.netbird,
        _ => true,
    }
}

const GROUPS: [(&str, &str, &str, &str, &str); 6] = [
    (
        "open",
        "Anyone on the internet",
        "No login at all",
        "error",
        "none",
    ),
    (
        "own",
        "Internet, app login only",
        "Only the app's own login screen stands in the way",
        "warning",
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
        "warning",
        "none",
    ),
    (
        "private",
        "Home and VPN only",
        "Not on the internet, and a login on top",
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

const ORIGINS: [(&str, &str, &str, &str); 4] = [
    ("web", "Internet", "Anyone in the world", "globe"),
    ("local", "Home network", "Devices at home", "home"),
    ("tailscale", "Tailscale", "Your Tailscale devices", "vpn"),
    ("netbird", "NetBird", "Your NetBird devices", "vpn"),
];

fn origin_label(id: &str) -> &'static str {
    ORIGINS.iter().find(|o| o.0 == id).map_or("", |o| o.1)
}

/// Origins (place ids) that reach a port from outside.
fn port_origins(p: &ReportPort, places: &ReportPlaces) -> Vec<&'static str> {
    if p.firewall == "blocked" {
        return Vec::new();
    }
    let a = p.address.trim();
    if !a.is_empty() && a != "0.0.0.0" && a != "::" && a != "[::]" {
        return Vec::new();
    }
    match p.interface.as_deref() {
        None => ORIGINS
            .iter()
            .map(|o| o.0)
            .filter(|id| place_active(id, places))
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

fn firewall_chips(ps: &[&ReportPort]) -> Vec<Chip> {
    let of = |state: &str| -> Vec<&ReportPort> {
        ps.iter().copied().filter(|p| p.firewall == state).collect()
    };
    let mut chips = Vec::new();
    let allowed = of("allowed");
    if !allowed.is_empty() {
        chips.push(Chip {
            text: format!("Firewall lets in {}", join_ports(&allowed)),
            tone: "ok",
        });
    }
    let only: Vec<&ReportPort> = of("interface");
    let (vpn, inside): (Vec<&ReportPort>, Vec<&ReportPort>) = only.into_iter().partition(|p| {
        p.interface
            .as_deref()
            .is_some_and(|i| i == "wt0" || i.starts_with("tailscale"))
    });
    if !vpn.is_empty() {
        chips.push(Chip {
            text: format!("VPN only: {}", join_ports(&vpn)),
            tone: "info",
        });
    }
    if !inside.is_empty() {
        chips.push(Chip {
            text: format!("Inside this server only: {}", join_ports(&inside)),
            tone: "muted",
        });
    }
    let docker = of("docker");
    if !docker.is_empty() {
        chips.push(Chip {
            text: format!("Skips the firewall: {}", join_ports(&docker)),
            tone: "warning",
        });
    }
    let off = of("off");
    if !off.is_empty() {
        chips.push(Chip {
            text: format!("No firewall: {}", join_ports(&off)),
            tone: "error",
        });
    }
    let blocked = of("blocked");
    if !blocked.is_empty() {
        chips.push(Chip {
            text: format!("Blocked: {}", join_ports(&blocked)),
            tone: "muted",
        });
    }
    chips
}

fn plural(n: usize, one: &str, many: &str) -> String {
    if n == 1 {
        format!("1 {one}")
    } else {
        format!("{n} {many}")
    }
}

pub(super) fn build(
    apps: &[MapApp],
    ports: &[ReportPort],
    places: &ReportPlaces,
    ssh: &ReportSsh,
) -> AccessMap {
    let mut map = AccessMap {
        origins: ORIGINS
            .iter()
            .map(|(id, label, detail, icon)| Origin {
                id: format!("o-{id}"),
                label,
                detail,
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
        let mut from: Vec<&str> = Vec::new();
        for (oid, ..) in ORIGINS {
            if members.iter().any(|a| a.places.iter().any(|p| p == oid))
                && place_active(oid, places)
            {
                from.push(origin_label(oid));
            }
        }
        map.groups.push(Group {
            id: format!("g-{id}"),
            title,
            detail,
            tone,
            guard,
            from: if from.is_empty() {
                "Nowhere".into()
            } else {
                from.join(", ")
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
    let open_apps = map
        .groups
        .iter()
        .find(|g| g.id == "g-open")
        .map_or(0, |g| g.apps.len());

    // --- Doors ---
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
        let key = format!("{}|{}", p.service.as_deref().unwrap_or(""), p.label);
        match others.iter_mut().find(|(k, _)| *k == key) {
            Some((_, v)) => v.push(p),
            None => others.push((key, vec![p])),
        }
    }

    let add_door = |map: &mut AccessMap,
                    id: String,
                    label: String,
                    ps: &[&ReportPort],
                    web: bool,
                    guard: Option<Chip>,
                    inside: Option<String>,
                    filter: &'static str,
                    risky: bool| {
        let mut from: Vec<&'static str> = Vec::new();
        for p in ps {
            for o in port_origins(p, places) {
                if !from.contains(&o) {
                    from.push(o);
                }
            }
        }
        for o in &from {
            let skips = ps.iter().any(|p| {
                (p.firewall == "docker" || p.firewall == "off")
                    && port_origins(p, places).contains(o)
            });
            let tone = if *o == "web" && risky {
                "error"
            } else if skips {
                "warning"
            } else if *o == "web" {
                "ok"
            } else {
                "muted"
            };
            map.edges.push(Edge {
                from: format!("o-{o}"),
                to: id.clone(),
                tone,
                dashed: skips,
            });
        }
        let from_text = if from.is_empty() {
            "Only from inside this server".to_string()
        } else {
            from.iter()
                .map(|o| origin_label(o))
                .collect::<Vec<_>>()
                .join(", ")
        };
        map.doors.push(Door {
            id,
            label,
            ports: join_ports(ps),
            web,
            firewall: firewall_chips(ps),
            guard,
            inside,
            from: from_text,
            filter,
        });
    };

    if !web.is_empty() {
        add_door(
            &mut map,
            "d-web".into(),
            "Web".into(),
            &web,
            true,
            None,
            Some("Your apps".into()),
            "internet",
            open_apps > 0,
        );
        for g in &map.groups {
            map.edges.push(Edge {
                from: "d-web".into(),
                to: g.id.clone(),
                tone: g.tone,
                dashed: false,
            });
        }
    }
    if !ssh_ports.is_empty() {
        let guard = if ssh.password_login {
            Chip {
                text: "Password: no SSH key set".into(),
                tone: "error",
            }
        } else {
            Chip {
                text: format!("Only {}", plural(ssh.keys.len(), "SSH key", "SSH keys")),
                tone: "ok",
            }
        };
        add_door(
            &mut map,
            "d-ssh".into(),
            "SSH".into(),
            &ssh_ports,
            false,
            Some(guard),
            Some(format!("Server shell: {}", ssh.users.join(", "))),
            "",
            ssh.password_login,
        );
    }
    for (i, (_, ps)) in others.iter().enumerate() {
        let p = ps[0];
        let guard = if !p.described {
            Chip {
                text: "Unknown: Neo does not know what uses it".into(),
                tone: "error",
            }
        } else if p.open {
            Chip {
                text: format!("Answers anyone, on purpose ({})", p.login),
                tone: "info",
            }
        } else {
            Chip {
                text: p.login.clone(),
                tone: "ok",
            }
        };
        add_door(
            &mut map,
            format!("d-{i}"),
            if p.described {
                p.label.clone()
            } else {
                "Unknown port".into()
            },
            ps,
            false,
            Some(guard),
            p.service
                .as_ref()
                .map(|s| crate::commands::web::nix::sections::humanize(s)),
            "",
            !p.described,
        );
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
                "{} open to anyone on the internet, without any login.",
                plural(open_apps, "app is", "apps are")
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
    if let Some(g) = map.groups.iter().find(|g| g.id == "g-own") {
        v.push(Verdict {
            tone: "warning",
            text: format!(
                "{} on the internet only behind the app's own login.",
                plural(g.apps.len(), "app is", "apps are")
            ),
            filter: Some("bucket-own".into()),
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
                "{} on the internet {} addresses that skip the login page.",
                plural(paths, "app", "apps"),
                if paths == 1 { "has" } else { "have" }
            ),
            filter: Some("openpaths".into()),
        });
    }
    let skip: Vec<&ReportPort> = ports.iter().filter(|p| p.firewall == "docker").collect();
    if !skip.is_empty() {
        v.push(Verdict {
            tone: "warning",
            text: format!(
                "Port {} {} the firewall: Docker publishes {} to every network.",
                join_ports(&skip).replace(" · ", ", "),
                if skip.len() == 1 { "skips" } else { "skip" },
                if skip.len() == 1 { "it" } else { "them" }
            ),
            filter: Some("skipsfw".into()),
        });
    }
    map.ok = v.is_empty();
    map.level = if v.iter().any(|x| x.tone == "error") {
        "error"
    } else if v.is_empty() {
        "ok"
    } else {
        "warning"
    };
    if map.ok {
        v.push(Verdict {
            tone: "ok",
            text: "Nothing opens to the internet without a login.".into(),
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
            open_paths: 0,
        }
    }

    fn port(id: &str, port: u32, iface: Option<&str>, firewall: &str) -> ReportPort {
        ReportPort {
            id: Some(id.into()),
            protocol: "tcp".into(),
            port,
            interface: iface.map(str::to_string),
            described: true,
            label: id.into(),
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
    fn map_shows_doors_edges_and_verdict() {
        let places = ReportPlaces {
            netbird: true,
            ..Default::default()
        };
        let ports = vec![
            port("ssh", 22, None, "allowed"),
            port("streamproxy-https", 443, None, "allowed"),
            port("swag-http", 9980, None, "docker"),
            port("netbird-dns", 5353, Some("wt0"), "interface"),
        ];
        let ssh = ReportSsh {
            keys: vec!["k".into()],
            users: vec!["admin".into()],
            ..Default::default()
        };
        let apps = vec![
            app("pastebin", &["web"], false, false),
            app("immich", &["web"], true, false),
        ];
        let m = build(&apps, &ports, &places, &ssh);
        let doors: Vec<&str> = m.doors.iter().map(|d| d.label.as_str()).collect();
        assert_eq!(doors, ["Web", "SSH", "netbird-dns"]);
        assert_eq!(m.doors[0].ports, "443 · 9980");
        // The internet reaches the web door, which leads to an open app.
        assert!(m.edges.contains(&Edge {
            from: "o-web".into(),
            to: "d-web".into(),
            tone: "error",
            dashed: true,
        }));
        assert!(m
            .edges
            .iter()
            .any(|e| e.from == "o-netbird" && e.to == "d-0"));
        assert!(!m.edges.iter().any(|e| e.from == "o-web" && e.to == "d-0"));
        assert_eq!(m.groups.len(), 2);
        assert!(!m.ok);
        assert_eq!(
            m.verdict[0].text,
            "1 app is open to anyone on the internet, without any login."
        );
        assert!(m
            .verdict
            .iter()
            .any(|v| v.text.starts_with("Port 9980 skips the firewall")));
    }

    #[test]
    fn a_clean_server_says_so() {
        let ssh = ReportSsh {
            keys: vec!["k".into()],
            ..Default::default()
        };
        let m = build(
            &[app("immich", &["web"], true, false)],
            &[port("ssh", 22, None, "allowed")],
            &ReportPlaces::default(),
            &ssh,
        );
        assert!(m.ok);
        assert_eq!(m.verdict[0].tone, "ok");
    }
}
