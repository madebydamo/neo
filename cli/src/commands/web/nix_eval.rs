pub use super::evaluator::NixEvaluator;

use super::nix_extractors::{
    EXTRACT_NEO_THEME, EXTRACT_PROXIED_SERVICES, EXTRACT_SERVICES, EXTRACT_SERVICE_OPTIONS,
};
use super::structs::{
    IndexContext, NavigatorContext, OptionPaneContext, OptionSchema, Service, ServiceMeta,
};

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

impl NixEvaluator {
    pub async fn extract_services(&mut self) -> IndexContext {
        let inner = format!("{} {{ neoFlake = f; }}", EXTRACT_SERVICES.load_name);
        match self.query_json(&inner).await {
            Ok(svcs) => IndexContext {
                services: svcs,
                ..Default::default()
            },
            Err(e) => {
                eprintln!("web: nix extract_services failed: {e}");
                IndexContext {
                    services: vec![],
                    error: Some(format!(
                        "Nix evaluator error or timeout (>{}s) while extracting service list: {}. \
                         This often means a long first-time eval, or a problem in the config flake (e.g. flake.lock has 'path' locked to a /nix/store/*-source that no longer exists after GC or because the lock was generated on a dev machine with local git+file paths). \
                         The server-side timeout is now 10min to allow completion; the UI will show this message instead of spinning.",
                        600, e
                    )),
                    ..Default::default()
                }
            }
        }
    }

    pub async fn extract_service_options(&mut self, service: &str) -> OptionPaneContext {
        let inner = format!(
            r#"{} {{ neoFlake = f; service = "{}"; }}"#,
            EXTRACT_SERVICE_OPTIONS.load_name, service
        );
        #[derive(serde::Deserialize)]
        struct RawPane {
            meta: Option<ServiceMeta>,
            options: Vec<OptionSchema>,
            #[serde(default)]
            units: Vec<String>,
            #[serde(default)]
            containers: std::collections::HashMap<String, String>,
            #[serde(default)]
            error: Option<String>,
        }
        let raw: RawPane = match self.query_json(&inner).await {
            Ok(v) => v,
            Err(e) => {
                eprintln!("web: nix extract_service_options({service}) failed: {e}");
                RawPane {
                    meta: None,
                    options: vec![],
                    units: vec![],
                    containers: std::collections::HashMap::new(),
                    error: Some(format!("Nix eval error/timeout for service options ({}): {}. See neo-web logs. Long timeout (10min) active.", service, e)),
                }
            }
        };
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
            save_endpoint: format!("/save/{service}"),
            is_core: false,
            error: raw.error,
            units: raw.units,
            containers: raw.containers,
        }
    }

    pub async fn extract_neo_section(&mut self, section: &str) -> OptionPaneContext {
        let inner = format!(
            r#"{} {{ neoFlake = f; section = "{}"; }}"#,
            EXTRACT_SERVICE_OPTIONS.load_name, section
        );
        #[derive(serde::Deserialize)]
        struct RawPane {
            meta: Option<ServiceMeta>,
            options: Vec<OptionSchema>,
            #[serde(default)]
            units: Vec<String>,
            #[serde(default)]
            containers: std::collections::HashMap<String, String>,
            #[serde(default)]
            error: Option<String>,
        }
        let raw: RawPane = match self.query_json(&inner).await {
            Ok(v) => v,
            Err(e) => {
                eprintln!("web: nix extract_neo_section({section}) failed: {e}");
                RawPane {
                    meta: None,
                    options: vec![],
                    units: vec![],
                    containers: std::collections::HashMap::new(),
                    error: Some(format!(
                        "Nix eval error/timeout for core section ({}): {}. See logs.",
                        section, e
                    )),
                }
            }
        };
        let mut opts = raw.options;
        for o in &mut opts {
            o.defaultDisplay = value_to_display(&o.default);
            o.currentDisplay = o.current.as_ref().map(value_to_display).unwrap_or_default();
        }

        // For scalar core options the walk produces name="", rename to the section
        // so the form labels/saves it correctly (under [core] in toml).
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
            save_endpoint: format!("/save-core/{section}"),
            is_core: true,
            error: raw.error,
            units: raw.units,
            containers: raw.containers,
        }
    }

    pub async fn extract_proxied_services(&mut self) -> NavigatorContext {
        let inner = format!("{} {{ neoFlake = f; }}", EXTRACT_PROXIED_SERVICES.load_name);
        let mut ctx: NavigatorContext = match self.query_json(&inner).await {
            Ok(v) => v,
            Err(e) => {
                eprintln!("web: nix extract_proxied_services failed: {e}");
                NavigatorContext {
                    domain: None,
                    services: vec![],
                    theme: String::new(),
                    error: Some(format!(
                        "Nix evaluator error or timeout (>{}s) while building navigator: {e}. \
                         Common cause: flake.lock (in your config dir) references a now-missing /nix/store/*-source/flake.nix (from dev machine git+file or post-GC). \
                         Long server timeout (10min) in effect; UI will display this instead of spinning.",
                        600
                    )),
                    ..Default::default()
                }
            }
        };
        let dom = ctx.domain.clone();
        for s in &mut ctx.services {
            s.domain = dom.clone();
            if s.initials.is_empty() {
                s.initials = s.name.chars().take(2).collect::<String>().to_uppercase();
            }
        }
        ctx
    }

    pub async fn extract_neo_theme(&mut self) -> String {
        let inner = format!("{} {{ neoFlake = f; }}", EXTRACT_NEO_THEME.load_name);
        match self.query_json(&inner).await {
            Ok(v) => v,
            Err(e) => {
                eprintln!("web: nix extract_neo_theme failed: {e}");
                "lofi".to_string()
            }
        }
    }
}
