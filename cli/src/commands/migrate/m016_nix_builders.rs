use toml_edit::{Array, DocumentMut, Item, Table, Value};

use super::{insert_dotted, remove_dotted};

/// Remote Nix builders went from one `core.nix.remoteBuild` table to
/// `core.nix.builders.<name>` (multi-machine with priority weights):
/// - If `remoteBuild` has a `host`, move it to `core.nix.builders.main`
///   (preserving `enabled`, defaulting to `false` if omitted since the old
///   default was `false` while the new per-builder default is `true`).
/// - Convert `system = "..."` to `systems = ["..."]`.
/// - If `remoteBuild` had no `host`, drop the table cleanly.
pub(super) fn migrate(doc: &mut DocumentMut) {
    let Some(old) = remove_dotted(doc, "core.nix.remoteBuild") else {
        return;
    };
    let Some(mut tbl) = old.into_table().ok() else {
        return;
    };

    let has_host = tbl
        .get("host")
        .and_then(|v| v.as_str())
        .map(|s| !s.trim().is_empty())
        .unwrap_or(false);
    if !has_host {
        return;
    }

    if !tbl.contains_key("enabled") {
        tbl.insert("enabled", Item::Value(Value::from(false)));
    }

    if let Some(sys) = tbl.remove("system") {
        if let Some(s) = sys.as_str() {
            let mut arr = Array::new();
            arr.push(s);
            tbl.insert("systems", Item::Value(Value::Array(arr)));
        }
    }

    let mut target = Table::new();
    for (k, v) in tbl.iter() {
        target.insert(k, v.clone());
    }
    insert_dotted(doc, "core.nix.builders.main", Item::Table(target));
}

#[cfg(test)]
mod tests {
    use super::super::apply_migrations;
    use toml_edit::DocumentMut;

    #[test]
    fn migration_016_moves_enabled_remote_build_into_builders_main() {
        let mut doc: DocumentMut = r#"
[core.nix.remoteBuild]
enabled = true
host = "buildbox"
user = "damo"
maxJobs = 16
speedFactor = 4
system = "aarch64-linux"
extraOptions = ["-p", "2222"]
"#
        .parse()
        .unwrap();
        apply_migrations(&mut doc);
        let nix = &doc["core"]["nix"];
        assert!(nix.get("remoteBuild").is_none());
        let main = &nix["builders"]["main"];
        assert_eq!(main["enabled"].as_bool(), Some(true));
        assert_eq!(main["host"].as_str(), Some("buildbox"));
        assert_eq!(main["user"].as_str(), Some("damo"));
        assert_eq!(main["maxJobs"].as_integer(), Some(16));
        assert_eq!(main["speedFactor"].as_integer(), Some(4));
        assert!(main.get("system").is_none());
        let systems: Vec<&str> = main["systems"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|v| v.as_str())
            .collect();
        assert_eq!(systems, vec!["aarch64-linux"]);
        assert_eq!(main["extraOptions"].as_array().map(|a| a.len()), Some(2));
    }

    #[test]
    fn migration_016_keeps_disabled_when_enabled_was_omitted() {
        let mut doc: DocumentMut = r#"
[core.nix.remoteBuild]
host = "buildbox"
user = "damo"
"#
        .parse()
        .unwrap();
        apply_migrations(&mut doc);
        let main = &doc["core"]["nix"]["builders"]["main"];
        assert_eq!(main["enabled"].as_bool(), Some(false));
        assert_eq!(main["host"].as_str(), Some("buildbox"));
    }

    #[test]
    fn migration_016_drops_empty_or_hostless_remote_build() {
        let mut doc: DocumentMut = r#"
[core.nix.remoteBuild]
enabled = false
"#
        .parse()
        .unwrap();
        apply_migrations(&mut doc);
        let nix = &doc["core"]["nix"];
        assert!(nix.get("remoteBuild").is_none());
        assert!(nix.get("builders").is_none());
    }
}
