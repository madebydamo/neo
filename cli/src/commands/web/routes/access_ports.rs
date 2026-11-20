//! Open ports on the Access page as a flow: networks on the left, the host
//! firewall in the middle, the ports on the right (access.html.hbs).
//!
//! Each port is reached from one scope (every network, one VPN, this server
//! only, one address). The edge from its scope shows what the firewall does
//! with it (`firewall` in `neo.exposure.report`, nix/modules/core/exposure.nix).
//! The geometry is computed here, in percent of the diagram, so the page needs
//! no script: the SVG stretches with `preserveAspectRatio="none"`.
use serde::Serialize;

use super::access::{ReportPlaces, ReportPort};
use crate::commands::web::nix::sections::humanize;

/// One network the ports are reached from (left column).
#[derive(Serialize, Default, Debug)]
pub(super) struct FlowSource {
    id: String,
    label: String,
    detail: String,
    /// Icon in the template: globe, vpn, server, address.
    icon: &'static str,
    /// Networks inside "every network", as chips.
    networks: Vec<&'static str>,
    /// Vertical centre in percent of the diagram.
    y: f64,
}

/// One port (right column) and the edge to it.
#[derive(Serialize, Default, Debug)]
pub(super) struct FlowPort {
    port: u32,
    protocol: String,
    label: String,
    login: String,
    open: bool,
    described: bool,
    service: Option<String>,
    service_url: Option<String>,
    /// allowed | interface | docker | blocked | off
    firewall: String,
    firewall_label: &'static str,
    firewall_help: String,
    /// Colour of the edge and its firewall mark (Tailwind class).
    tone: &'static str,
    /// Badge colour of the firewall chip.
    chip: &'static str,
    /// Icon of the firewall mark: check, lock, cross, warn.
    mark: &'static str,
    /// The edge goes around the firewall (dashed).
    dashed: bool,
    /// Scope in words, for narrow screens (no diagram there).
    scope: String,
    y: f64,
    /// SVG path of the edge (viewBox 0 0 100 100).
    edge: String,
    /// Where the edge crosses the firewall, in percent.
    cross_y: f64,
    /// Quick-filter tags (access_filter.js), space-separated.
    pub(super) tags: String,
}

#[derive(Serialize, Default, Debug)]
pub(super) struct PortsFlow {
    sources: Vec<FlowSource>,
    pub(super) ports: Vec<FlowPort>,
    /// Number of rows, for the diagram height.
    rows: usize,
    /// Some port goes around the firewall (legend).
    any_docker: bool,
    any_blocked: bool,
    firewall_off: bool,
}

/// (sort order, id, label, detail, icon) of the scope a port is reached from.
fn scope_of(p: &ReportPort) -> (u8, String, String, String, &'static str) {
    let address = p.address.trim();
    if !address.is_empty() && address != "0.0.0.0" && address != "::" && address != "[::]" {
        return (
            4,
            format!("addr:{address}"),
            format!("Address {address}"),
            "Only traffic sent to this address".into(),
            "address",
        );
    }
    match p.interface.as_deref() {
        None => (
            0,
            "everywhere".into(),
            "Every network".into(),
            "Anyone who can reach this server".into(),
            "globe",
        ),
        Some("wt0") => (
            1,
            "netbird".into(),
            "NetBird".into(),
            "Your NetBird devices only".into(),
            "vpn",
        ),
        Some(i) if i.starts_with("tailscale") => (
            2,
            "tailscale".into(),
            "Tailscale".into(),
            "Your Tailscale devices only".into(),
            "vpn",
        ),
        Some(i) if i.starts_with("ve-") => (
            5,
            "server".into(),
            "This server".into(),
            "Programs on this server only".into(),
            "server",
        ),
        Some(i) => (
            3,
            format!("if:{i}"),
            format!("Network {i}"),
            "Only this network".into(),
            "vpn",
        ),
    }
}

fn firewall_text(state: &str, scope: &str) -> (&'static str, String) {
    match state {
        "allowed" => (
            "Firewall lets it in",
            "A firewall rule opens this port to every network.".into(),
        ),
        "interface" => (
            "Firewall: this network only",
            format!("The firewall opens this port only for {scope}. It drops the same port from every other network."),
        ),
        "docker" => (
            "Skips the firewall",
            "No firewall rule opens this port, but Docker publishes it. Docker's rules come before the firewall, so every network reaches it anyway.".into(),
        ),
        "blocked" => (
            "Blocked by the firewall",
            "Something listens on this port, but the firewall drops traffic from outside.".into(),
        ),
        "off" => (
            "Firewall is off",
            "The firewall is switched off, so nothing filters this port.".into(),
        ),
        _ => ("Unknown", "Neo does not know what the firewall does with this port.".into()),
    }
}

/// (tone, chip, mark) of a firewall state.
fn firewall_style(state: &str) -> (&'static str, &'static str, &'static str) {
    match state {
        "allowed" => ("text-success", "badge-success", "check"),
        "interface" => ("text-info", "badge-info", "lock"),
        "blocked" => ("text-base-content/40", "badge-ghost", "cross"),
        _ => ("text-warning", "badge-warning", "warn"),
    }
}

/// Quick-filter tags of a port (same names as the apps' tags in access.rs).
fn port_tags(p: &ReportPort, scope_id: &str) -> String {
    let internet = scope_id == "everywhere" && p.firewall != "blocked";
    let no_login = p.open || !p.described;
    let mut t = Vec::new();
    if internet {
        t.push("internet");
    }
    if no_login {
        t.push("nologin");
    }
    if internet && no_login {
        t.push("internet-nologin");
    }
    if p.firewall == "docker" || p.firewall == "off" {
        t.push("skipsfw");
    }
    if !p.described || p.firewall == "off" {
        t.push("risk");
    }
    t.join(" ")
}

fn centre(row: usize, rows: usize) -> f64 {
    ((row as f64 + 0.5) * 100.0 / rows as f64 * 100.0).round() / 100.0
}

pub(super) fn build(ports: &[ReportPort], places: &ReportPlaces) -> PortsFlow {
    let mut rows: Vec<(u8, String, &ReportPort)> = ports
        .iter()
        .map(|p| {
            let (order, id, ..) = scope_of(p);
            (order, id, p)
        })
        .collect();
    rows.sort_by(|a, b| {
        (a.0, &a.1, a.2.port, &a.2.protocol).cmp(&(b.0, &b.1, b.2.port, &b.2.protocol))
    });
    let n = rows.len();

    let mut sources: Vec<FlowSource> = Vec::new();
    let mut members: Vec<Vec<usize>> = Vec::new();
    for (i, (_, id, p)) in rows.iter().enumerate() {
        match sources.iter().position(|s| &s.id == id) {
            Some(k) => members[k].push(i),
            None => {
                let (_, id, label, detail, icon) = scope_of(p);
                let networks = if id == "everywhere" {
                    let mut v = vec!["Internet", "Home network"];
                    if places.netbird {
                        v.push("NetBird");
                    }
                    if places.tailscale {
                        v.push("Tailscale");
                    }
                    v
                } else {
                    Vec::new()
                };
                sources.push(FlowSource {
                    id,
                    label,
                    detail,
                    icon,
                    networks,
                    y: 0.0,
                });
                members.push(vec![i]);
            }
        }
    }
    for (s, m) in sources.iter_mut().zip(&members) {
        let sum: f64 = m.iter().map(|&i| centre(i, n)).sum();
        s.y = (sum / m.len() as f64 * 100.0).round() / 100.0;
    }

    let flow_ports: Vec<FlowPort> = rows
        .iter()
        .enumerate()
        .map(|(i, (_, id, p))| {
            let src = sources.iter().find(|s| &s.id == id);
            let ys = src.map_or(0.0, |s| s.y);
            let scope = src.map_or(String::new(), |s| s.label.clone());
            let yp = centre(i, n);
            let (edge, cross_y) = if p.firewall == "blocked" {
                // Ends at the firewall.
                (format!("M0 {ys} C25 {ys} 25 {yp} 50 {yp}"), yp)
            } else {
                (
                    format!("M0 {ys} C50 {ys} 50 {yp} 100 {yp}"),
                    ((ys + yp) / 2.0 * 100.0).round() / 100.0,
                )
            };
            let (firewall_label, firewall_help) = firewall_text(&p.firewall, &scope);
            let (tone, chip, mark) = firewall_style(&p.firewall);
            FlowPort {
                port: p.port,
                protocol: p.protocol.to_uppercase(),
                label: p.label.clone(),
                login: if p.described {
                    p.login.clone()
                } else {
                    "Unknown".into()
                },
                open: p.open,
                described: p.described,
                service_url: p
                    .service
                    .as_ref()
                    .map(|s| format!("/configuration/option/{s}")),
                service: p.service.as_ref().map(|s| humanize(s)),
                firewall: p.firewall.clone(),
                firewall_label,
                firewall_help,
                tone,
                chip,
                mark,
                dashed: p.firewall == "docker" || p.firewall == "off",
                tags: port_tags(p, id),
                scope,
                y: yp,
                edge,
                cross_y,
            }
        })
        .collect();

    PortsFlow {
        any_docker: flow_ports.iter().any(|p| p.firewall == "docker"),
        any_blocked: flow_ports.iter().any(|p| p.firewall == "blocked"),
        firewall_off: flow_ports.iter().any(|p| p.firewall == "off"),
        rows: n,
        sources,
        ports: flow_ports,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn port(port: u32, iface: Option<&str>, firewall: &str) -> ReportPort {
        ReportPort {
            protocol: "tcp".into(),
            port,
            interface: iface.map(str::to_string),
            described: true,
            label: format!("Port {port}"),
            login: "x".into(),
            firewall: firewall.into(),
            ..Default::default()
        }
    }

    #[test]
    fn groups_ports_by_scope_and_centres_sources() {
        let ports = vec![
            port(9982, Some("ve-streamproxy"), "interface"),
            port(443, None, "allowed"),
            port(9980, None, "docker"),
            port(5353, Some("wt0"), "interface"),
        ];
        let places = ReportPlaces {
            netbird: true,
            ..Default::default()
        };
        let f = build(&ports, &places);
        let order: Vec<u32> = f.ports.iter().map(|p| p.port).collect();
        assert_eq!(order, [443, 9980, 5353, 9982]);
        let ids: Vec<&str> = f.sources.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, ["everywhere", "netbird", "server"]);
        // Every network owns rows 0 and 1 of 4: centres 12.5 and 37.5.
        assert_eq!(f.sources[0].y, 25.0);
        assert_eq!(
            f.sources[0].networks,
            ["Internet", "Home network", "NetBird"]
        );
        assert_eq!(f.ports[1].edge, "M0 25 C50 25 50 37.5 100 37.5");
        assert_eq!(f.ports[1].firewall_label, "Skips the firewall");
        assert_eq!(f.ports[1].tags, "internet skipsfw");
        assert_eq!(f.ports[2].tags, "");
        assert!(f.any_docker);
        assert_eq!(f.ports[2].scope, "NetBird");
    }

    #[test]
    fn blocked_edge_stops_at_the_firewall() {
        let f = build(&[port(8080, None, "blocked")], &ReportPlaces::default());
        assert_eq!(f.ports[0].edge, "M0 50 C25 50 25 50 50 50");
        assert!(f.any_blocked);
    }

    #[test]
    fn bound_address_is_its_own_scope() {
        let mut p = port(9982, None, "docker");
        p.address = "10.233.0.1".into();
        let f = build(&[p], &ReportPlaces::default());
        assert_eq!(f.sources[0].label, "Address 10.233.0.1");
    }
}
