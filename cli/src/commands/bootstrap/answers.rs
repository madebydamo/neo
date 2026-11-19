//! The wizard's answers and how they land in settings.toml. Everything else in
//! the file (from an earlier run or the web UI step) stays as it is.

use toml_edit::{value, Array, DocumentMut, Item, Table};

use crate::utils::settings::{ensure_table, item_at, str_at};

/// How the internet reaches the new server.
#[derive(Debug, Clone, PartialEq)]
pub enum Access {
    /// Its own public address (ports 80/443 reach it).
    Public,
    /// Through the rathole tunnel of a streamproxy edge.
    Edge(Tunnel),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Tunnel {
    /// Address of the edge (rathole remoteAddr).
    pub remote_addr: String,
    /// Entry name on the edge (rathole name).
    pub name: String,
    pub token: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Answers {
    pub hostname: String,
    pub domain: String,
    pub email: String,
    pub authorized_keys: Vec<String>,
    /// `user:bcrypt` lines for the web login.
    pub web_users: Vec<String>,
    /// None: keep what is set. Some(""): no Linux password.
    pub linux_password_hash: Option<String>,
    pub disk: String,
    /// Extra disks (device → mount point), each its own erased pool. None keeps them.
    pub additional_disks: Option<Vec<(String, String)>>,
    /// Automatic snapshots of the Neo data; None keeps the setting.
    pub auto_snapshot: Option<bool>,
    pub access: Access,
    /// Neo flake the server follows (`neo-cli.server.neoInput`); None keeps it.
    pub server_neo_input: Option<String>,
}

pub const DEFAULT_NEO_INPUT: &str = "github:madebydamo/neo";

/// The table at `path`; new parent tables stay implicit (no empty `[services]`).
fn table<'a>(doc: &'a mut DocumentMut, path: &[&str]) -> &'a mut Table {
    let mut t = doc.as_table_mut();
    for (i, p) in path.iter().enumerate() {
        let new = !t.contains_key(p);
        t = ensure_table(t, p).expect("ensure_table always yields a table");
        if new && i + 1 < path.len() {
            t.set_implicit(true);
        }
    }
    t
}

fn string_array(items: &[String]) -> Item {
    let mut a = Array::new();
    for i in items {
        a.push(i.as_str());
    }
    value(a)
}

pub fn apply(doc: &mut DocumentMut, a: &Answers) {
    table(doc, &["core"]).insert("hostname", value(&a.hostname));
    if let Some(hash) = &a.linux_password_hash {
        table(doc, &["core"]).insert("hashedLinuxPassword", value(hash));
    }
    table(doc, &["core", "ssh"]).insert("authorizedKeys", string_array(&a.authorized_keys));

    let disko = table(doc, &["disko"]);
    disko.insert("enabled", value(true));
    disko.insert("mainDisk", value(&a.disk));
    if let Some(extra) = &a.additional_disks {
        if extra.is_empty() {
            disko.remove("additionalDisks");
        } else {
            let mut t = toml_edit::InlineTable::new();
            for (dev, mp) in extra {
                t.insert(dev, mp.as_str().into());
            }
            disko.insert("additionalDisks", value(t));
        }
    }
    if let Some(on) = a.auto_snapshot {
        if on {
            if let Some(t) = disko
                .get_mut("autoSnapshot")
                .and_then(Item::as_table_like_mut)
            {
                t.remove("enabled");
            }
        } else {
            table(doc, &["disko", "autoSnapshot"]).insert("enabled", value(false));
        }
    }

    let swag = table(doc, &["services", "swag"]);
    swag.insert("enabled", value(true));
    swag.insert("domain", value(&a.domain));
    swag.insert("email", value(&a.email));

    let tinyauth = table(doc, &["services", "tinyauth"]);
    tinyauth.insert("enabled", value(true));
    tinyauth.insert("users", string_array(&a.web_users));

    for svc in ["neo", "system-updater"] {
        table(doc, &["services", svc]).insert("enabled", value(true));
    }

    match &a.access {
        Access::Public => {
            if item_at(doc, &["services", "rathole"]).is_some() {
                table(doc, &["services", "rathole"]).insert("enabled", value(false));
            }
        }
        Access::Edge(t) => {
            let r = table(doc, &["services", "rathole"]);
            r.insert("enabled", value(true));
            r.insert("remoteAddr", value(&t.remote_addr));
            r.insert("name", value(&t.name));
            r.insert("token", value(&t.token));
        }
    }

    if let Some(input) = &a.server_neo_input {
        if input == DEFAULT_NEO_INPUT {
            if let Some(server) = doc
                .get_mut("neo-cli")
                .and_then(|c| c.get_mut("server"))
                .and_then(Item::as_table_like_mut)
            {
                server.remove("neoInput");
            }
        } else {
            table(doc, &["neo-cli", "server"]).insert("neoInput", value(input));
        }
    }
}

/// `disko.additionalDisks` as (device, mount point).
pub fn additional_disks(doc: &DocumentMut) -> Vec<(String, String)> {
    item_at(doc, &["disko", "additionalDisks"])
        .and_then(Item::as_table_like)
        .map(|t| {
            t.iter()
                .filter_map(|(k, v)| v.as_str().map(|m| (k.to_string(), m.to_string())))
                .collect()
        })
        .unwrap_or_default()
}

/// A string list setting (empty when unset).
pub fn strings(doc: &DocumentMut, path: &[&str]) -> Vec<String> {
    item_at(doc, path)
        .and_then(Item::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// A non-empty string setting.
pub fn string(doc: &DocumentMut, path: &[&str]) -> Option<String> {
    str_at(doc, path)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// Hostname rules NixOS accepts (networking.hostName).
pub fn check_hostname(s: &str) -> Result<(), String> {
    let ok = !s.is_empty()
        && s.len() <= 63
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        && !s.starts_with('-')
        && !s.ends_with('-');
    if ok {
        Ok(())
    } else {
        Err("use lowercase letters, digits and dashes (max 63)".into())
    }
}

pub fn check_domain(s: &str) -> Result<(), String> {
    let ok = s.contains('.')
        && !s.starts_with('.')
        && !s.ends_with('.')
        && !s.contains("://")
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'.');
    if ok {
        Ok(())
    } else {
        Err("a domain like home.example.com (no https://)".into())
    }
}

pub fn check_email(s: &str) -> Result<(), String> {
    match s.split_once('@') {
        Some((u, d)) if !u.is_empty() && d.contains('.') => Ok(()),
        _ => Err("an email address for Let's Encrypt".into()),
    }
}

pub fn check_username(s: &str) -> Result<(), String> {
    if !s.is_empty()
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.')
    {
        Ok(())
    } else {
        Err("letters, digits, '.', '-' and '_'".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn answers(access: Access) -> Answers {
        Answers {
            hostname: "home".into(),
            domain: "home.example.com".into(),
            email: "me@example.com".into(),
            authorized_keys: vec!["ssh-ed25519 AAAA me".into()],
            web_users: vec!["me:$2b$10$x".into()],
            linux_password_hash: None,
            disk: "/dev/disk/by-id/ata-x".into(),
            additional_disks: None,
            auto_snapshot: None,
            access,
            server_neo_input: None,
        }
    }

    #[test]
    fn writes_the_basics_and_keeps_the_rest() {
        let mut doc: DocumentMut =
            "[core]\nhashedLinuxPassword = \"$6$old\"\n[services.immich]\nenabled = true\n"
                .parse()
                .unwrap();
        apply(&mut doc, &answers(Access::Public));
        assert_eq!(str_at(&doc, &["core", "hostname"]), Some("home"));
        assert_eq!(
            str_at(&doc, &["core", "hashedLinuxPassword"]),
            Some("$6$old")
        );
        assert_eq!(
            str_at(&doc, &["services", "swag", "domain"]),
            Some("home.example.com")
        );
        assert_eq!(
            str_at(&doc, &["disko", "mainDisk"]),
            Some("/dev/disk/by-id/ata-x")
        );
        assert_eq!(
            strings(&doc, &["services", "tinyauth", "users"]),
            ["me:$2b$10$x"]
        );
        assert_eq!(doc["services"]["immich"]["enabled"].as_bool(), Some(true));
        assert_eq!(doc["services"]["neo"]["enabled"].as_bool(), Some(true));
        assert!(doc["services"].get("rathole").is_none());
    }

    #[test]
    fn edge_sets_the_tunnel_and_public_turns_it_off() {
        let mut doc = DocumentMut::new();
        let tunnel = Tunnel {
            remote_addr: "edge.example.com".into(),
            name: "home".into(),
            token: "tok".into(),
        };
        apply(&mut doc, &answers(Access::Edge(tunnel)));
        assert_eq!(doc["services"]["rathole"]["enabled"].as_bool(), Some(true));
        assert_eq!(str_at(&doc, &["services", "rathole", "token"]), Some("tok"));
        apply(&mut doc, &answers(Access::Public));
        assert_eq!(doc["services"]["rathole"]["enabled"].as_bool(), Some(false));
    }

    #[test]
    fn server_neo_input_default_is_not_written() {
        let mut doc = DocumentMut::new();
        let mut a = answers(Access::Public);
        a.server_neo_input = Some("github:madebydamo/neo/dev".into());
        apply(&mut doc, &a);
        assert_eq!(
            str_at(&doc, &["neo-cli", "server", "neoInput"]),
            Some("github:madebydamo/neo/dev")
        );
        a.server_neo_input = Some(DEFAULT_NEO_INPUT.into());
        apply(&mut doc, &a);
        assert!(str_at(&doc, &["neo-cli", "server", "neoInput"]).is_none());
    }

    #[test]
    fn disko_extras() {
        let mut doc = DocumentMut::new();
        let mut a = answers(Access::Public);
        a.additional_disks = Some(vec![(
            "/dev/disk/by-id/b".into(),
            "/var/neo/DATA/Media".into(),
        )]);
        a.auto_snapshot = Some(false);
        apply(&mut doc, &a);
        assert_eq!(
            additional_disks(&doc),
            [(
                "/dev/disk/by-id/b".to_string(),
                "/var/neo/DATA/Media".to_string()
            )]
        );
        assert_eq!(
            doc["disko"]["autoSnapshot"]["enabled"].as_bool(),
            Some(false)
        );
        a.additional_disks = Some(vec![]);
        a.auto_snapshot = Some(true);
        apply(&mut doc, &a);
        assert!(additional_disks(&doc).is_empty());
        assert!(doc["disko"]["autoSnapshot"].get("enabled").is_none());
    }

    #[test]
    fn checks() {
        assert!(check_hostname("home-1").is_ok());
        assert!(check_hostname("Home").is_err());
        assert!(check_domain("home.example.com").is_ok());
        assert!(check_domain("https://x.com").is_err());
        assert!(check_email("a@b.ch").is_ok());
        assert!(check_email("ab.ch").is_err());
        assert!(check_username("damo").is_ok());
        assert!(check_username("da mo").is_err());
    }
}
