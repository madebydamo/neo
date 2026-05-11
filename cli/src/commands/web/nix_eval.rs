use std::process::Command;

use super::structs::{OptionInfo, Service};

static EXTRACT_SERVICES: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/src/commands/web/nix/extract_services.nix"
));
static EXTRACT_OPTIONS: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/src/commands/web/nix/extract_service_options.nix"
));

pub fn extract_services(nix_cmd: &str, neo_input: &str) -> Vec<Service> {
    let expr = format!(
        r#"({}) {{ neoFlake = "{}"; }}"#,
        EXTRACT_SERVICES, neo_input
    );
    let output = Command::new(nix_cmd)
        .args(["eval", "--json", "--impure", "--expr", &expr])
        .output();
    let mut svcs: Vec<Service> = output
        .ok()
        .and_then(|o| {
            if o.status.success() {
                Some(o.stdout)
            } else {
                None
            }
        })
        .and_then(|stdout| serde_json::from_slice(&stdout).ok())
        .unwrap_or_default();
    svcs.sort_by_key(|s| s.name.clone());
    svcs
}

pub fn extract_service_options(nix_cmd: &str, neo_input: &str, service: &str) -> Vec<OptionInfo> {
    let expr = format!(
        r#"({}) {{ neoFlake = "{}"; service = "{}"; }}"#,
        EXTRACT_OPTIONS, neo_input, service
    );
    let output = Command::new(nix_cmd)
        .args(["eval", "--json", "--impure", "--expr", &expr])
        .output();
    println!(
        "{:?}",
        &output
            .as_ref()
            .ok()
            .map(|o| o.stdout.clone())
            .map(|o| String::from_utf8(o))
            .and_then(|o| o.ok())
            .map(|o| o.replace("\\n", "\\r"))
    );
    output
        .ok()
        .and_then(|o| {
            if o.status.success() {
                Some(o.stdout)
            } else {
                None
            }
        })
        .and_then(|stdout| serde_json::from_slice(&stdout).ok())
        .unwrap_or_default()
}
