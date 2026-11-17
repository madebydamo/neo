use toml_edit::{DocumentMut, Item};

use super::insert_dotted;

/// The backup's default SSH key became the host key (escrowed, so it survives a
/// restore). Configured backups keep the homeserver key their remote trusts.
pub(super) fn migrate(doc: &mut DocumentMut) {
    let configured = doc
        .get("services")
        .and_then(|t| t.get("backup"))
        .and_then(|t| t.as_table_like())
        .is_some_and(|t| t.contains_key("host") && !t.contains_key("sshKey"));
    if configured {
        insert_dotted(
            doc,
            "services.backup.sshKey",
            Item::Value("/home/homeserver/.ssh/id_ed25519".into()),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::super::apply_migrations;
    use toml_edit::DocumentMut;

    #[test]
    fn migration_013_pins_the_old_backup_key() {
        let mut doc: DocumentMut = "[services.backup]\nenabled = true\nhost = \"b.example\"\n"
            .parse()
            .unwrap();
        assert!(apply_migrations(&mut doc));
        let key = doc["services"]["backup"]["destinations"]["main"]["sshKey"].as_str();
        assert_eq!(key, Some("/home/homeserver/.ssh/id_ed25519"));

        let mut kept: DocumentMut = "[services.backup]\nhost = \"b\"\nsshKey = \"/k\"\n"
            .parse()
            .unwrap();
        apply_migrations(&mut kept);
        assert_eq!(
            kept["services"]["backup"]["destinations"]["main"]["sshKey"].as_str(),
            Some("/k")
        );

        let mut fresh = DocumentMut::new();
        apply_migrations(&mut fresh);
        assert!(fresh
            .get("services")
            .and_then(|s| s.get("backup"))
            .is_none());
    }
}
