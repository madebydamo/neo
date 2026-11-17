use toml_edit::DocumentMut;

use super::{insert_dotted, move_dotted, remove_dotted};

/// Move the single tinyauth Entra block into the multi-provider `oauth` attrset:
/// `entra.*` → `oauth.entra.*` (+ `type = "entra"`), `entra.autoRedirect` →
/// `oauthAutoRedirect = "entra"`, and `entraGroups` → `oauthGroups`.
pub(super) fn migrate(doc: &mut DocumentMut) {
    move_dotted(
        doc,
        "services.tinyauth.entraGroups",
        "services.tinyauth.oauthGroups",
    );
    let Some(mut entra) = remove_dotted(doc, "services.tinyauth.entra") else {
        return;
    };
    let Some(table) = entra.as_table_like_mut() else {
        return;
    };
    let auto_redirect = table
        .remove("autoRedirect")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    // `enabled` used to default to false; providers now default to enabled.
    if table.get("enabled").is_none() {
        table.insert("enabled", toml_edit::value(false));
    }
    table.insert("type", toml_edit::value("entra"));
    if auto_redirect {
        insert_dotted(
            doc,
            "services.tinyauth.oauthAutoRedirect",
            toml_edit::value("entra"),
        );
    }
    insert_dotted(doc, "services.tinyauth.oauth.entra", entra);
}

#[cfg(test)]
mod tests {
    use super::super::apply_migrations;
    use toml_edit::DocumentMut;

    #[test]
    fn migration_010_moves_entra_into_oauth_providers() {
        let raw = r#"
[services.tinyauth]
enabled = true
entraGroups = ["firma"]

[services.tinyauth.entra]
enabled = true
autoRedirect = true
clientId = "cid"
tenantId = "tid"
"#;
        let mut doc: DocumentMut = raw.parse().unwrap();
        assert!(apply_migrations(&mut doc));
        let ta = doc["services"]["tinyauth"].as_table().expect("tinyauth");
        assert!(ta.get("entra").is_none());
        assert!(ta.get("entraGroups").is_none());
        assert_eq!(ta["oauthGroups"].as_array().map(|a| a.len()), Some(1));
        assert_eq!(ta["oauthAutoRedirect"].as_str(), Some("entra"));
        let p = &ta["oauth"]["entra"];
        assert_eq!(p["type"].as_str(), Some("entra"));
        assert_eq!(p["enabled"].as_bool(), Some(true));
        assert_eq!(p["clientId"].as_str(), Some("cid"));
        assert!(p.get("autoRedirect").is_none());
    }

    #[test]
    fn migration_010_keeps_disabled_entra_disabled() {
        let raw = r#"
[services.tinyauth.entra]
clientId = "cid"
"#;
        let mut doc: DocumentMut = raw.parse().unwrap();
        assert!(apply_migrations(&mut doc));
        let p = &doc["services"]["tinyauth"]["oauth"]["entra"];
        assert_eq!(p["enabled"].as_bool(), Some(false));
    }
}
