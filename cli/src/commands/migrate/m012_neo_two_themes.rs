use toml_edit::DocumentMut;

/// The web UI has only `light` and `dark` now: `lofi` became `light`,
/// `halloween` (dark with orange) becomes `dark`.
pub(super) fn migrate(doc: &mut DocumentMut) {
    let Some(neo) = doc
        .get_mut("services")
        .and_then(|t| t.get_mut("neo"))
        .and_then(|t| t.as_table_like_mut())
    else {
        return;
    };
    let new = match neo.get("theme").and_then(|v| v.as_str()) {
        Some("lofi") => "light",
        Some("halloween") => "dark",
        _ => return,
    };
    neo.insert("theme", toml_edit::value(new));
}

#[cfg(test)]
mod tests {
    use super::super::apply_migrations;
    use toml_edit::DocumentMut;

    #[test]
    fn migration_012_maps_old_themes() {
        let mut doc: DocumentMut = "[services.neo]\ntheme = \"halloween\"\n".parse().unwrap();
        assert!(apply_migrations(&mut doc));
        assert_eq!(doc["services"]["neo"]["theme"].as_str(), Some("dark"));
        let mut doc: DocumentMut = "[services.neo]\ntheme = \"lofi\"\n".parse().unwrap();
        apply_migrations(&mut doc);
        assert_eq!(doc["services"]["neo"]["theme"].as_str(), Some("light"));
    }
}
