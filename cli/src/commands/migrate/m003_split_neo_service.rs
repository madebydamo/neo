use toml_edit::{DocumentMut, Item};

use super::{insert_dotted, profile_has_key, remove_dotted};

/// Delete retired [neo-service] and [nixos] tables.
///
/// Keys that still exist are moved to core.plugins, services.system-updater, and neo-cli.
/// Anything else in those tables (enabled, bootstrapEnabled, …) is dropped.
pub(super) fn migrate(doc: &mut DocumentMut) {
    for key in ["neo-service", "nixos"] {
        if let Some(item) = remove_dotted(doc, key) {
            absorb_retired_service_table(doc, &item);
        }
    }
}

fn absorb_retired_service_table(doc: &mut DocumentMut, svc_item: &Item) {
    let Some(svc) = svc_item.as_table() else {
        return;
    };

    if let Some(plugins) = svc.get("plugins") {
        insert_dotted(doc, "core.plugins", plugins.clone());
    }

    // system-updater.enabled from autoUpdateEnabled, else bootstrapEnabled
    if let Some(v) = svc.get("autoUpdateEnabled") {
        insert_dotted(doc, "services.system-updater.enabled", v.clone());
    } else if let Some(v) = svc.get("bootstrapEnabled") {
        insert_dotted(doc, "services.system-updater.enabled", v.clone());
    }
    if let Some(v) = svc.get("autoUpdateTimer") {
        insert_dotted(doc, "services.system-updater.schedule", v.clone());
    }
    // Legacy neo-service key kept the misspelling; write the corrected option name.
    if let Some(v) = svc.get("garbageCollectOlderThen") {
        insert_dotted(
            doc,
            "services.system-updater.garbageCollectOlderThan",
            v.clone(),
        );
    }

    // Server-side config path → neo-cli.server (004 also handles legacy top-level configPath).
    if let Some(v) = svc.get("configPath") {
        let already = profile_has_key(doc, "server", "configPath");
        if !already {
            insert_dotted(doc, "neo-cli.server.configPath", v.clone());
        }
    }

    // Shared CLI keys: fill neo-cli only when the key is not already set.
    const CLI_KEYS: &[&str] = &[
        "repoUrl",
        "neoInput",
        "template",
        "bootstrapMethod",
        "gitUserName",
        "gitUserEmail",
        "defaultBranch",
        "rebuildBranchFormat",
    ];
    for key in CLI_KEYS {
        let already = doc
            .get("neo-cli")
            .and_then(|t| t.as_table())
            .map(|t| t.contains_key(key))
            .unwrap_or(false);
        if already {
            continue;
        }
        if let Some(v) = svc.get(key) {
            insert_dotted(doc, &format!("neo-cli.{key}"), v.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::apply_migrations;
    use toml_edit::DocumentMut;

    #[test]
    fn migration_003_drops_retired_service_sections() {
        let raw = r#"
[neo-service]
enabled = true
bootstrapEnabled = true
autoUpdateEnabled = false
autoUpdateTimer = "daily"
garbageCollectOlderThen = "7d"
plugins = ["github:example/plugin"]
neoInput = "github:example/neo"
configPath = "/var/neo/DATA/AppData/configuration"

[nixos]
enabled = true
repoUrl = "https://example.test/cfg.git"
"#;
        let mut doc: DocumentMut = raw.parse().unwrap();
        assert!(apply_migrations(&mut doc));

        assert!(doc.get("neo-service").is_none());
        assert!(doc.get("nixos").is_none());

        let updater = doc
            .get("services")
            .and_then(|s| s.get("system-updater"))
            .and_then(|t| t.as_table())
            .expect("services.system-updater");
        assert_eq!(
            updater.get("enabled").and_then(|v| v.as_bool()),
            Some(false)
        );
        assert_eq!(
            updater.get("schedule").and_then(|v| v.as_str()),
            Some("daily")
        );
        assert_eq!(
            updater
                .get("garbageCollectOlderThan")
                .and_then(|v| v.as_str()),
            Some("7d")
        );
        assert!(updater.get("garbageCollectOlderThen").is_none());

        // Carried over by 003, then turned into tables by 014.
        let plugins = crate::utils::plugins::plugins_from_doc(&doc);
        assert_eq!(
            plugins.iter().map(|p| p.url.as_str()).collect::<Vec<_>>(),
            vec!["github:example/plugin"]
        );

        let cli = doc.get("neo-cli").and_then(|t| t.as_table()).unwrap();
        assert!(
            cli.get("neoInput").is_none(),
            "shared neoInput moves onto the profiles"
        );
        assert_eq!(
            cli.get("local")
                .and_then(|t| t.get("neoInput"))
                .and_then(|v| v.as_str()),
            Some("github:example/neo")
        );
        assert_eq!(
            cli.get("server")
                .and_then(|t| t.get("neoInput"))
                .and_then(|v| v.as_str()),
            Some("github:example/neo")
        );
        assert_eq!(
            cli.get("repoUrl").and_then(|v| v.as_str()),
            Some("https://example.test/cfg.git")
        );
        assert_eq!(
            cli.get("server")
                .and_then(|t| t.get("configPath"))
                .and_then(|v| v.as_str()),
            Some("/var/neo/DATA/AppData/configuration")
        );

        let out = doc.to_string();
        assert!(!out.contains("bootstrapEnabled"));
        assert!(!out.contains("autoUpdateEnabled"));
        assert!(
            !apply_migrations(&mut doc),
            "second run must report no further migrations"
        );
    }
}
