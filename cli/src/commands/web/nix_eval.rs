use std::process::Command;

use super::structs::{NavigatorContext, OptionPaneContext, OptionSchema, Service, ServiceMeta};

static EXTRACT_SERVICES: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/src/commands/web/nix/extract_services.nix"
));
static EXTRACT_OPTIONS: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/src/commands/web/nix/extract_service_options.nix"
));
static EXTRACT_PROXIED: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/src/commands/web/nix/extract_proxied_services.nix"
));
static EXTRACT_NEO_THEME: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/src/commands/web/nix/extract_neo_theme.nix"
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
    svcs.sort_by(|a, b| match (a.rank, b.rank) {
        (Some(ra), Some(rb)) => ra.cmp(&rb),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => a.name.cmp(&b.name),
    });
    svcs
}

fn value_to_display(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::Null => "null".to_string(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Array(_) | serde_json::Value::Object(_) => {
            serde_json::to_string_pretty(v).unwrap_or_else(|_| v.to_string())
        }
    }
}

pub fn extract_service_options(nix_cmd: &str, neo_input: &str, service: &str) -> OptionPaneContext {
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

    #[derive(serde::Deserialize)]
    struct RawPane {
        meta: Option<ServiceMeta>,
        options: Vec<OptionSchema>,
    }

    let raw: RawPane = output
        .ok()
        .and_then(|o| {
            if o.status.success() {
                Some(o.stdout)
            } else {
                None
            }
        })
        .and_then(|stdout| serde_json::from_slice(&stdout).ok())
        .unwrap_or(RawPane {
            meta: None,
            options: vec![],
        });

    let mut opts = raw.options;
    for o in &mut opts {
        o.defaultDisplay = value_to_display(&o.default);
        o.currentDisplay = o.current.as_ref().map(value_to_display).unwrap_or_default();
    }

    let options_json = serde_json::to_string(&opts).unwrap_or_else(|_| "[]".to_string());
    OptionPaneContext {
        service: service.to_string(),
        meta: raw.meta,
        options: opts,
        options_json,
        save_endpoint: format!("/save/{}", service),
        is_core: false,
    }
}

pub fn extract_neo_section(nix_cmd: &str, neo_input: &str, section: &str) -> OptionPaneContext {
    let expr = format!(
        r#"({}) {{ neoFlake = "{}"; section = "{}"; }}"#,
        EXTRACT_OPTIONS, neo_input, section
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

    #[derive(serde::Deserialize)]
    struct RawPane {
        meta: Option<ServiceMeta>,
        options: Vec<OptionSchema>,
    }

    let raw: RawPane = output
        .ok()
        .and_then(|o| {
            if o.status.success() {
                Some(o.stdout)
            } else {
                None
            }
        })
        .and_then(|stdout| serde_json::from_slice(&stdout).ok())
        .unwrap_or(RawPane {
            meta: None,
            options: vec![],
        });

    let mut opts = raw.options;
    for o in &mut opts {
        o.defaultDisplay = value_to_display(&o.default);
        o.currentDisplay = o.current.as_ref().map(value_to_display).unwrap_or_default();
    }

    // For scalar core options (timeZone, uid, gid, hostname, hashedLinuxPassword) the walk produces name="", rename to the section
    // so the form labels/saves it correctly (under [core] in toml). This only applies for the legacy individual /core/xxx cards (now removed in favor of aggregate /core/core).
    let scalar_sections = ["timeZone", "uid", "gid", "hostname", "hashedLinuxPassword"];
    if scalar_sections.contains(&section) && opts.len() == 1 && opts[0].name == "" {
        opts[0].name = section.to_string();
    }

    let options_json = serde_json::to_string(&opts).unwrap_or_else(|_| "[]".to_string());
    OptionPaneContext {
        service: section.to_string(),
        meta: raw.meta,
        options: opts,
        options_json,
        save_endpoint: format!("/save-core/{}", section),
        is_core: true,
    }
}

pub fn extract_proxied_services(nix_cmd: &str, neo_input: &str) -> NavigatorContext {
    let expr = format!(r#"({}) {{ neoFlake = "{}"; }}"#, EXTRACT_PROXIED, neo_input);
    let output = Command::new(nix_cmd)
        .args(["eval", "--json", "--impure", "--expr", &expr])
        .output();
    let mut ctx: NavigatorContext = output
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
    ctx.services.sort_by(|a, b| match (a.rank, b.rank) {
        (Some(ra), Some(rb)) => ra.cmp(&rb),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => a.name.cmp(&b.name),
    });
    let dom = ctx.domain.clone();
    for s in &mut ctx.services {
        s.domain = dom.clone();
        if s.initials.is_empty() {
            s.initials = s.name.chars().take(2).collect::<String>().to_uppercase();
        }
    }
    ctx
}

pub fn extract_neo_theme(nix_cmd: &str, neo_input: &str) -> String {
    let expr = format!(
        r#"({}) {{ neoFlake = "{}"; }}"#,
        EXTRACT_NEO_THEME, neo_input
    );
    let output = Command::new(nix_cmd)
        .args(["eval", "--json", "--impure", "--expr", &expr])
        .output();
    output
        .ok()
        .and_then(|o| {
            if o.status.success() {
                Some(o.stdout)
            } else {
                None
            }
        })
        .and_then(|stdout| serde_json::from_slice::<String>(&stdout).ok())
        .unwrap_or_else(|| "lofi".to_string())
}
