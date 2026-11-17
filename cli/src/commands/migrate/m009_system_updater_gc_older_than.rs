use toml_edit::DocumentMut;

use super::{insert_dotted, remove_dotted};

/// Fix system-updater GC option typo: OlderThen → OlderThan.
/// Existing corrected key wins; the misspelled key is always dropped.
pub(super) fn migrate(doc: &mut DocumentMut) {
    let Some(val) = remove_dotted(doc, "services.system-updater.garbageCollectOlderThen") else {
        return;
    };
    let taken = doc
        .get("services")
        .and_then(|t| t.get("system-updater"))
        .and_then(|t| t.as_table_like())
        .is_some_and(|t| t.contains_key("garbageCollectOlderThan"));
    if !taken {
        insert_dotted(doc, "services.system-updater.garbageCollectOlderThan", val);
    }
}

#[cfg(test)]
mod tests {
    use super::super::apply_migrations;
    use toml_edit::DocumentMut;

    #[test]
    fn migration_009_renames_gc_older_then_to_than() {
        let raw = r#"
[services.system-updater]
enabled = true
garbageCollectOlderThen = "30d"
"#;
        let mut doc: DocumentMut = raw.parse().unwrap();
        assert!(apply_migrations(&mut doc));
        let updater = doc
            .get("services")
            .and_then(|s| s.get("system-updater"))
            .and_then(|t| t.as_table())
            .expect("services.system-updater");
        assert_eq!(
            updater
                .get("garbageCollectOlderThan")
                .and_then(|v| v.as_str()),
            Some("30d")
        );
        assert!(updater.get("garbageCollectOlderThen").is_none());
        assert!(
            !apply_migrations(&mut doc),
            "second run must report no further migrations"
        );
    }

    #[test]
    fn migration_009_keeps_existing_than_and_drops_then() {
        let raw = r#"
[services.system-updater]
garbageCollectOlderThan = "60d"
garbageCollectOlderThen = "7d"
"#;
        let mut doc: DocumentMut = raw.parse().unwrap();
        assert!(apply_migrations(&mut doc));
        let updater = doc
            .get("services")
            .and_then(|s| s.get("system-updater"))
            .and_then(|t| t.as_table())
            .expect("services.system-updater");
        assert_eq!(
            updater
                .get("garbageCollectOlderThan")
                .and_then(|v| v.as_str()),
            Some("60d")
        );
        assert!(updater.get("garbageCollectOlderThen").is_none());
    }
}
