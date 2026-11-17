use toml_edit::{DocumentMut, Item};

use crate::utils::profile::is_local_flake_ref;

use super::{insert_dotted, profile_has_key, remove_dotted};

/// Move shared neo-cli.neoInput / template onto the profiles.
/// A local checkout path stays on the local profile. The server profile keeps
/// the GitHub default instead of inheriting that path. A remote URL is copied
/// to both profiles.
pub(super) fn migrate(doc: &mut DocumentMut) {
    for key in ["neoInput", "template"] {
        let Some(raw) = doc
            .get("neo-cli")
            .and_then(|t| t.get(key))
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_string())
        else {
            continue;
        };
        if raw.is_empty() {
            remove_dotted(doc, &format!("neo-cli.{key}"));
            continue;
        }
        let item = Item::Value(toml_edit::Value::from(raw.as_str()));
        let profiles: &[&str] = if is_local_flake_ref(&raw) {
            &["local"]
        } else {
            &["local", "server"]
        };
        for profile in profiles {
            if profile_has_key(doc, profile, key) {
                continue;
            }
            insert_dotted(doc, &format!("neo-cli.{profile}.{key}"), item.clone());
        }
        remove_dotted(doc, &format!("neo-cli.{key}"));
    }
}

#[cfg(test)]
mod tests {
    use super::super::apply_migrations;
    use toml_edit::DocumentMut;

    #[test]
    fn migration_007_keeps_a_local_checkout_off_the_server_profile() {
        let raw = r#"
[neo-cli]
neoInput = "git+file:/home/damo/Documents/projects/homeserver/neo"
template = "/home/damo/Documents/projects/homeserver/neo#homeserver"

[neo-cli.local]
configPath = "/home/damo/Documents/projects/homeserver/neo/build"
"#;
        let mut doc: DocumentMut = raw.parse().unwrap();
        assert!(apply_migrations(&mut doc));

        let cli = doc.get("neo-cli").and_then(|t| t.as_table()).unwrap();
        assert!(cli.get("neoInput").is_none());
        assert!(cli.get("template").is_none());
        assert_eq!(
            cli.get("local")
                .and_then(|t| t.get("neoInput"))
                .and_then(|v| v.as_str()),
            Some("git+file:/home/damo/Documents/projects/homeserver/neo")
        );
        assert_eq!(
            cli.get("local")
                .and_then(|t| t.get("template"))
                .and_then(|v| v.as_str()),
            Some("/home/damo/Documents/projects/homeserver/neo#homeserver")
        );
        let server = cli.get("server").and_then(|t| t.as_table());
        assert!(
            server
                .map(|t| !t.contains_key("neoInput") && !t.contains_key("template"))
                .unwrap_or(true),
            "server profile must keep the GitHub default"
        );
        assert!(
            !apply_migrations(&mut doc),
            "second run must report no further migrations"
        );
    }
}
