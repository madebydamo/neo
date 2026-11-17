use toml_edit::DocumentMut;

use super::remove_dotted;

/// `neo-cli.bootstrapMethod` is gone: config repos always come from the template.
pub(super) fn migrate(doc: &mut DocumentMut) {
    for path in [
        "neo-cli.bootstrapMethod",
        "neo-cli.local.bootstrapMethod",
        "neo-cli.server.bootstrapMethod",
    ] {
        remove_dotted(doc, path);
    }
}

#[cfg(test)]
mod tests {
    use super::super::apply_migrations;
    use toml_edit::DocumentMut;

    #[test]
    fn migration_011_drops_bootstrap_method_everywhere() {
        let raw = r#"
[neo-cli]
bootstrapMethod = "clone"
repoUrl = "https://example.test/cfg.git"

[neo-cli.server]
bootstrapMethod = "template"

[neo-service]
bootstrapMethod = "clone"
"#;
        let mut doc: DocumentMut = raw.parse().unwrap();
        assert!(apply_migrations(&mut doc));
        let cli = doc.get("neo-cli").and_then(|t| t.as_table()).unwrap();
        assert!(cli.get("bootstrapMethod").is_none());
        assert!(cli
            .get("server")
            .and_then(|t| t.get("bootstrapMethod"))
            .is_none());
        assert_eq!(
            cli.get("repoUrl").and_then(|v| v.as_str()),
            Some("https://example.test/cfg.git")
        );
    }
}
