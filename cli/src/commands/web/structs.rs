use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone)]
pub struct Service {
    pub name: String,
    pub enabled: bool,
}

#[derive(Serialize)]
pub struct IndexContext {
    pub services: Vec<Service>,
}

#[derive(Clone, Debug)]
pub struct AppConfig {
    pub nix_cmd: String,
    pub neo_input: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct OptionInfo {
    pub name: String,
    pub r#type: String,
    pub default: String,
    pub description: String,
}

#[derive(Serialize)]
pub struct OptionContext {
    pub service: String,
    pub options: Vec<OptionInfo>,
}
