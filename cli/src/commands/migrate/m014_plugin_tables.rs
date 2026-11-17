use toml_edit::DocumentMut;

/// `core.plugins` went from a list of URLs to `[core.plugins.<id>]` tables
/// (docs/design/plugin-instances.md). Ids are the ones the list form already
/// gets in Nix, so the flake inputs `plugin-<id>` stay the same.
pub(super) fn migrate(doc: &mut DocumentMut) {
    if crate::utils::plugins::plugins_are_tables(doc) {
        return;
    }
    let entries = crate::utils::plugins::plugins_from_doc(doc);
    crate::utils::plugins::write_plugins(doc, &entries);
}

#[cfg(test)]
mod tests {
    use super::super::apply_migrations;
    use toml_edit::DocumentMut;

    #[test]
    fn migration_014_turns_the_plugin_list_into_tables() {
        let raw = r#"
[core]
hostname = "x"
plugins = ["github:madebydamo/highsea.neo", "git+file:/home/me/portrait"]
"#;
        let mut doc: DocumentMut = raw.parse().unwrap();
        assert!(apply_migrations(&mut doc));
        let out = doc.to_string();
        assert!(
            out.contains("[core.plugins.highsea-neo]\nurl = \"github:madebydamo/highsea.neo\""),
            "{out}"
        );
        assert!(
            out.contains("[core.plugins.portrait]\nurl = \"git+file:/home/me/portrait\""),
            "{out}"
        );
        assert!(!apply_migrations(&mut doc));
    }
}
