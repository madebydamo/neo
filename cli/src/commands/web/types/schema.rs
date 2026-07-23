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
}
