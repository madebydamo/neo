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

/// Declarative UI presentation (widgets, choices, keysFrom, save). See nix/lib/ui.nix.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct OptionUi {
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub values: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,
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
    /// Declarative presentation metadata from option.ui (widgets, keysFrom, …).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ui: Option<OptionUi>,
}
