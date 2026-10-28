use serde::{Deserialize, Serialize};

fn default_true() -> bool {
    true
}

fn default_generate_label() -> String {
    "Generate".to_string()
}

fn default_apply_set() -> String {
    "set".to_string()
}

fn default_extract_identity() -> String {
    "identity".to_string()
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct HelperInput {
    pub name: String,
    #[serde(rename = "type")]
    pub input_type: String,
    #[serde(default)]
    pub label: String,
    #[serde(default = "default_true")]
    pub required: bool,
    #[serde(default)]
    pub placeholder: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub values: Option<Vec<String>>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct OptionHelper {
    #[serde(default)]
    pub id: String,
    pub kind: String,
    #[serde(default = "default_generate_label")]
    pub label: String,
    #[serde(default)]
    pub description: String,
    #[serde(default = "default_apply_set")]
    pub apply: String,
    /// Absolute script path from extract. Trusted only after server-side schema resolve.
    #[serde(default)]
    pub script: String,
    #[serde(default)]
    pub inputs: Vec<HelperInput>,
}

/// attrsOf keys derived from another option (e.g. usernames from users list).
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct OptionUiKeysFrom {
    pub option: String,
    /// "identity" | "beforeColon"
    #[serde(default = "default_extract_identity")]
    pub extract: String,
}

/// One mode for exclusiveListPair (open / allow / block, etc.).
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct OptionUiMode {
    pub id: String,
    pub label: String,
    /// Submodule list field names active in this mode (empty = open / no lists).
    #[serde(default)]
    pub active: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "listLabel")]
    pub list_label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "hintEmpty")]
    pub hint_empty: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "hintFilled"
    )]
    pub hint_filled: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub badge: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct OptionUiSave {
    #[serde(default, rename = "pruneEmptyEntries")]
    pub prune_empty_entries: bool,
    #[serde(default, rename = "omitIfEmpty")]
    pub omit_if_empty: bool,
}

/// Collapsible settings section (`lib.neo.ui.groups.*` / `mkGroup`).
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct OptionUiGroup {
    pub id: String,
    #[serde(default)]
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default)]
    pub rank: i64,
}

/// Declarative UI presentation (widgets, choices, keysFrom, save). See nix/lib/ui.nix.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct OptionUi {
    /// Settings section; inherited from the nearest ancestor at extract time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<OptionUiGroup>,
    /// Human label; falls back to the humanized last path segment.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Show this option's value as a chip on its collapsed group header.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub summary: bool,
    /// Display names for enum / choice values.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "choiceLabels"
    )]
    pub choice_labels: Option<std::collections::BTreeMap<String, String>>,
    /// Sibling bool option (relative name) that must be true for this field to show.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "visibleWhen"
    )]
    pub visible_when: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub widget: Option<String>,
    /// Named choice provider or resolved list name (type.values holds the actual choices).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub choices: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "keysFrom")]
    pub keys_from: Option<OptionUiKeysFrom>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modes: Option<Vec<OptionUiMode>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub save: Option<OptionUiSave>,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "emptyHint")]
    pub empty_hint: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "entryLabel"
    )]
    pub entry_label: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "choiceEmptyHint"
    )]
    pub choice_empty_hint: Option<String>,
    /// Provider rows for providerAuth (id, label, hasApiKey, hasOauth, …).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub catalog: Option<Vec<serde_json::Value>>,
    /// OAuth helper script + optional runAs / env for providerAuth.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub oauth: Option<OptionUiOauth>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct OptionUiOauth {
    #[serde(default)]
    pub script: String,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "runAs")]
    pub run_as: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub env: Option<std::collections::BTreeMap<String, String>>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct OptionType {
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub elem: Option<Box<OptionType>>,
    /// Submodule field schemas (for attrsOf/listOf of submodule, or nested submodule types).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fields: Option<Vec<OptionSchema>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<i64>,
    /// Enum members or resolved `ui.choices`. Nix enums are not always strings
    /// (`types.enum [0 1 2]` on disko `folders.<name>.survive`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub values: Option<Vec<serde_json::Value>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,
    /// `values` paired with display labels (filled in Rust from ui.choiceLabels).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub choices: Option<Vec<ChoiceItem>>,
}

/// One enum / multi-select choice: raw value (as saved) + display label.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct ChoiceItem {
    pub value: serde_json::Value,
    pub label: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct OptionSchema {
    pub name: String,
    #[serde(rename = "type")]
    pub r#type: OptionType,
    pub typeLabel: String,
    #[serde(default)]
    pub default: serde_json::Value,
    #[serde(default)]
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub example: Option<serde_json::Value>,
    #[serde(default)]
    pub internal: bool,
    #[serde(default, rename = "readOnly")]
    pub read_only: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current: Option<serde_json::Value>,
    #[serde(default)]
    pub defaultDisplay: String,
    #[serde(default)]
    pub currentDisplay: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rank: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub helper: Option<OptionHelper>,
    /// lib.neo.mkSecretOption: the form masks the value.
    #[serde(default)]
    pub secret: bool,
    /// Declarative presentation metadata from option.ui (widgets, keysFrom, …).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ui: Option<OptionUi>,
    /// Display label (ui.label or humanized last path segment). Filled in Rust.
    #[serde(default)]
    pub label: String,
    /// Humanized parent path ("Containers") when the label alone lacks context.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub crumb: String,
    /// Absolute option name of ui.visibleWhen (resolved against this option's parent).
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        rename = "visibleWhen"
    )]
    pub visible_when: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enum_values_keep_integer_members() {
        let t: OptionType = serde_json::from_value(serde_json::json!({
            "kind": "enum",
            "values": [0, 1, 2]
        }))
        .expect("integer enum members");
        assert_eq!(
            t.values,
            Some(vec![
                serde_json::json!(0),
                serde_json::json!(1),
                serde_json::json!(2)
            ])
        );
    }

    #[test]
    fn enum_values_keep_string_members() {
        let t: OptionType = serde_json::from_value(serde_json::json!({
            "kind": "enum",
            "values": ["auto", "bulk"]
        }))
        .expect("string enum members");
        assert_eq!(
            t.values,
            Some(vec![serde_json::json!("auto"), serde_json::json!("bulk")])
        );
    }

    /// The disko pane is one widget-on-parent submodule. Nested
    /// `folders.*.survive` is `types.enum [0 1 2]`; rejecting that integer
    /// used to fail the whole section with "invalid type: integer `0`".
    #[test]
    fn disko_schema_deserializes_survive_int_enum() {
        let schema: OptionSchema = serde_json::from_value(serde_json::json!({
            "name": "",
            "type": {
                "kind": "submodule",
                "fields": [{
                    "name": "folders",
                    "type": {
                        "kind": "attrsOf",
                        "elem": {
                            "kind": "submodule",
                            "fields": [{
                                "name": "survive",
                                "type": { "kind": "enum", "values": [0, 1, 2] },
                                "typeLabel": "enum",
                                "default": 0,
                                "description": "How many disks may die before this folder is lost."
                            }]
                        }
                    },
                    "typeLabel": "attrs of submodule",
                    "default": {},
                    "description": ""
                }]
            },
            "typeLabel": "submodule",
            "default": {},
            "description": "Disko configuration",
            "ui": { "widget": "storageLayout" }
        }))
        .expect("disko schema");

        let survive = &schema.r#type.fields.as_ref().unwrap()[0]
            .r#type
            .elem
            .as_ref()
            .unwrap()
            .fields
            .as_ref()
            .unwrap()[0];
        assert_eq!(survive.name, "survive");
        assert_eq!(
            survive.r#type.values.as_ref().unwrap()[0],
            serde_json::json!(0)
        );
    }
}
