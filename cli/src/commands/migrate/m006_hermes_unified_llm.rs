use toml_edit::{DocumentMut, Item, Table};

/// Collapse split Hermes LLM keys into services.hermes.llm.{provider,apiKey,model}.
pub(super) fn migrate(doc: &mut DocumentMut) {
    let Some(services) = doc.get_mut("services").and_then(|s| s.as_table_mut()) else {
        return;
    };
    let Some(hermes) = services.get_mut("hermes").and_then(|s| s.as_table_mut()) else {
        return;
    };

    let model_provider = table_nonempty_str(hermes, "modelProvider");
    let default_model = table_nonempty_str(hermes, "defaultModel");
    let xai = table_nonempty_str(hermes, "xaiApiKey");
    let anthropic = table_nonempty_str(hermes, "anthropicApiKey");
    let openai = table_nonempty_str(hermes, "openaiApiKey");
    let openrouter = table_nonempty_str(hermes, "openrouterApiKey");

    let derived_provider = model_provider.clone().or_else(|| {
        if xai.is_some() {
            Some("xai".into())
        } else if anthropic.is_some() {
            Some("anthropic".into())
        } else if openai.is_some() {
            Some("openai".into())
        } else if openrouter.is_some() {
            Some("openrouter".into())
        } else {
            None
        }
    });

    let derived_key = match derived_provider.as_deref() {
        Some("xai") | Some("xai-oauth") => xai.clone(),
        Some("anthropic") => anthropic.clone(),
        Some("openai") | Some("openai-api") => openai.clone(),
        Some("openrouter") => openrouter.clone(),
        _ => xai
            .clone()
            .or(anthropic.clone())
            .or(openai.clone())
            .or(openrouter.clone()),
    };

    let skip_provider = llm_has_str(hermes, "provider");
    let skip_key = llm_has_str(hermes, "apiKey");
    let skip_model = llm_has_str(hermes, "model");

    if derived_provider.is_some() || derived_key.is_some() || default_model.is_some() {
        if hermes.get("llm").and_then(|i| i.as_table()).is_none() {
            hermes.insert("llm", Item::Table(Table::new()));
        }
        if let Some(llm) = hermes.get_mut("llm").and_then(|i| i.as_table_mut()) {
            if !skip_provider {
                if let Some(p) = derived_provider {
                    llm.insert("provider", toml_edit::value(p));
                }
            }
            if !skip_key {
                if let Some(k) = derived_key {
                    llm.insert("apiKey", toml_edit::value(k));
                }
            }
            if !skip_model {
                if let Some(m) = default_model {
                    llm.insert("model", toml_edit::value(m));
                }
            }
        }
    }

    for key in [
        "xaiApiKey",
        "anthropicApiKey",
        "openaiApiKey",
        "openrouterApiKey",
        "modelProvider",
        "defaultModel",
    ] {
        hermes.remove(key);
    }

    if let Some(llm) = hermes.get("llm").and_then(|i| i.as_table()) {
        if llm.is_empty() {
            hermes.remove("llm");
        }
    }
}

fn table_nonempty_str(table: &Table, key: &str) -> Option<String> {
    table
        .get(key)
        .and_then(|i| i.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

fn llm_has_str(hermes: &Table, key: &str) -> bool {
    hermes
        .get("llm")
        .and_then(|i| i.as_table())
        .and_then(|t| table_nonempty_str(t, key))
        .is_some()
}

#[cfg(test)]
mod tests {
    use super::super::apply_migrations;
    use toml_edit::DocumentMut;

    #[test]
    fn migration_006_unifies_hermes_llm_from_xai_key() {
        let raw = r#"
[services.hermes]
enabled = true
xaiApiKey = "secret-xai"
openaiApiKey = "secret-openai"
defaultModel = "grok-4"
"#;
        let mut doc: DocumentMut = raw.parse().unwrap();
        assert!(apply_migrations(&mut doc));

        let llm = doc
            .get("services")
            .and_then(|s| s.get("hermes"))
            .and_then(|h| h.get("llm"))
            .and_then(|t| t.as_table())
            .expect("services.hermes.llm");
        assert_eq!(llm.get("provider").and_then(|v| v.as_str()), Some("xai"));
        assert_eq!(
            llm.get("apiKey").and_then(|v| v.as_str()),
            Some("secret-xai")
        );
        assert_eq!(llm.get("model").and_then(|v| v.as_str()), Some("grok-4"));

        let hermes = doc
            .get("services")
            .and_then(|s| s.get("hermes"))
            .and_then(|t| t.as_table())
            .unwrap();
        for gone in [
            "xaiApiKey",
            "openaiApiKey",
            "anthropicApiKey",
            "openrouterApiKey",
            "modelProvider",
            "defaultModel",
        ] {
            assert!(!hermes.contains_key(gone), "{gone} must be removed");
        }
        assert!(
            !doc.to_string().contains("secret-openai"),
            "non-selected vendor keys must be dropped"
        );
    }

    #[test]
    fn migration_006_honors_explicit_model_provider() {
        let raw = r#"
[services.hermes]
modelProvider = "xai-oauth"
defaultModel = "grok-build-latest"
xaiApiKey = "unused-when-oauth"
"#;
        let mut doc: DocumentMut = raw.parse().unwrap();
        assert!(apply_migrations(&mut doc));
        let llm = doc
            .get("services")
            .and_then(|s| s.get("hermes"))
            .and_then(|h| h.get("llm"))
            .and_then(|t| t.as_table())
            .unwrap();
        assert_eq!(
            llm.get("provider").and_then(|v| v.as_str()),
            Some("xai-oauth")
        );
        assert_eq!(
            llm.get("apiKey").and_then(|v| v.as_str()),
            Some("unused-when-oauth")
        );
        assert_eq!(
            llm.get("model").and_then(|v| v.as_str()),
            Some("grok-build-latest")
        );
    }

    #[test]
    fn migration_006_preserves_existing_llm_and_drops_legacy_keys() {
        let raw = r#"
[services.hermes]
xaiApiKey = "old-key"
[services.hermes.llm]
provider = "anthropic"
apiKey = "keep-me"
model = "claude-sonnet-4"
"#;
        let mut doc: DocumentMut = raw.parse().unwrap();
        assert!(apply_migrations(&mut doc));
        let llm = doc
            .get("services")
            .and_then(|s| s.get("hermes"))
            .and_then(|h| h.get("llm"))
            .and_then(|t| t.as_table())
            .unwrap();
        assert_eq!(
            llm.get("provider").and_then(|v| v.as_str()),
            Some("anthropic")
        );
        assert_eq!(llm.get("apiKey").and_then(|v| v.as_str()), Some("keep-me"));
        let out = doc.to_string();
        assert!(!out.contains("old-key"));
        assert!(!out.contains("xaiApiKey"));
    }

    #[test]
    fn migration_006_is_idempotent() {
        let raw = r#"
[services.hermes]
xaiApiKey = "secret-xai"
"#;
        let mut doc: DocumentMut = raw.parse().unwrap();
        assert!(apply_migrations(&mut doc));
        assert!(
            !apply_migrations(&mut doc),
            "second run must report no further migrations"
        );
        let llm = doc
            .get("services")
            .and_then(|s| s.get("hermes"))
            .and_then(|h| h.get("llm"))
            .and_then(|t| t.as_table())
            .unwrap();
        assert_eq!(
            llm.get("apiKey").and_then(|v| v.as_str()),
            Some("secret-xai")
        );
    }
}
