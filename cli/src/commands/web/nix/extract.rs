use super::errors::NixError;
use super::registry::{
    EXTRACT_NEO_THEME, EXTRACT_PROXIED_SERVICES, EXTRACT_SERVICES, EXTRACT_SERVICE_OPTIONS,
};
use super::repl::NixEvaluator;
use crate::commands::web::structs::{
    ExtractedServiceGroups, IndexContext, NavigatorContext, OptionPaneContext, OptionSchema,
    RuntimeUnit, ServiceMeta,
};
use crate::commands::web::util::{escape_nix_string, service_name_ok};

/// Map an anyhow/nix failure into a stable UI string with kind + summary.
fn format_nix_failure(context: &str, err: &anyhow::Error) -> String {
    let nix_err = NixError::classify(&format!("{err:#}"));
    format!(
        "{context}: {}.{}",
        nix_err.display_message(),
        match nix_err.kind {
            super::errors::NixErrorKind::MissingStorePath => {
                " Try repairing the store (nix-store --verify --repair) or refreshing flake inputs if the lock came from another machine."
            }
            super::errors::NixErrorKind::Timeout => {
                " The evaluator aborted without a result; check neo-web logs and config flake health."
            }
            super::errors::NixErrorKind::NetworkFetchFailed => {
                " Check network connectivity and that flake input URLs are reachable."
            }
            _ => "",
        }
    )
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

#[derive(serde::Deserialize)]
struct RawPane {
    meta: Option<ServiceMeta>,
    options: Vec<OptionSchema>,
    #[serde(default)]
    units: Vec<String>,
    #[serde(default)]
    containers: std::collections::HashMap<String, String>,
    #[serde(default)]
    appdata: Option<String>,
    #[serde(default, rename = "appdataRoot")]
    appdata_root: Option<String>,
    #[serde(default)]
    error: Option<String>,
}

enum PaneTarget {
    Service(String),
    CoreSection(String),
}

impl PaneTarget {
    fn label(&self) -> &str {
        match self {
            PaneTarget::Service(s) | PaneTarget::CoreSection(s) => s,
        }
    }

    fn nix_arg(&self) -> (&'static str, &str) {
        match self {
            PaneTarget::Service(s) => ("service", s.as_str()),
            PaneTarget::CoreSection(s) => ("section", s.as_str()),
        }
    }

    fn is_core(&self) -> bool {
        matches!(self, PaneTarget::CoreSection(_))
    }

    fn save_endpoint(&self) -> String {
        match self {
            PaneTarget::Service(s) => format!("/save/{s}"),
            PaneTarget::CoreSection(s) => format!("/save-core/{s}"),
        }
    }
}

fn map_units(
    units: Vec<String>,
    containers: &std::collections::HashMap<String, String>,
) -> Vec<RuntimeUnit> {
    units
        .into_iter()
        .map(|name| {
            let bare = if name.starts_with("docker-") {
                &name[7..]
            } else {
                name.as_str()
            };
            let is_container = containers.contains_key(bare) || name.starts_with("docker-");
            RuntimeUnit { name, is_container }
        })
        .collect()
}

fn enrich_type(t: &mut crate::commands::web::structs::OptionType) {
    if let Some(fields) = t.fields.as_mut() {
        enrich_options(fields);
    }
    if let Some(elem) = t.elem.as_mut() {
        enrich_type(elem);
    }
}

fn enrich_options(opts: &mut [OptionSchema]) {
    for o in opts.iter_mut() {
        o.defaultDisplay = value_to_display(&o.default);
        o.currentDisplay = o.current.as_ref().map(value_to_display).unwrap_or_default();
        enrich_type(&mut o.r#type);
    }
}

fn rename_scalar_core_option(section: &str, opts: &mut [OptionSchema]) {
    let scalar_sections = [
        "timeZone",
        "uid",
        "gid",
        "hostname",
        "hashedLinuxPassword",
        "plugins",
    ];
    if scalar_sections.contains(&section) && opts.len() == 1 && opts[0].name.is_empty() {
        opts[0].name = section.to_string();
    }
}

impl NixEvaluator {
    pub async fn extract_services(&mut self) -> IndexContext {
        let inner = format!("{} {{ neoFlake = f; }}", EXTRACT_SERVICES.load_name);
        match self.query_json::<ExtractedServiceGroups>(&inner).await {
            Ok(extracted) => IndexContext {
                groups: extracted.groups,
                categories: extracted.categories,
                ..Default::default()
            },
            Err(e) => {
                eprintln!("web: nix extract_services failed: {e:#}");
                IndexContext {
                    error: Some(format_nix_failure(
                        "Nix error while extracting service list",
                        &e,
                    )),
                    ..Default::default()
                }
            }
        }
    }

    fn invalid_pane(target: &PaneTarget, reason: &str) -> OptionPaneContext {
        OptionPaneContext {
            service: target.label().to_string(),
            meta: None,
            options: vec![],
            options_json: "[]".to_string(),
            save_endpoint: target.save_endpoint(),
            is_core: target.is_core(),
            error: Some(reason.to_string()),
            units: vec![],
            containers: std::collections::HashMap::new(),
            appdata: None,
            appdata_root: None,
        }
    }

    async fn extract_pane(&mut self, target: PaneTarget) -> OptionPaneContext {
        // Same charset as service names (alnum, `-`, `_`); rejects empty, path
        // separators, and the literal fallback "service".
        if !service_name_ok(target.label()) {
            return Self::invalid_pane(
                &target,
                &format!(
                    "Invalid {} name for nix extract",
                    if target.is_core() {
                        "section"
                    } else {
                        "service"
                    }
                ),
            );
        }

        let (arg_name, arg_val) = target.nix_arg();
        let label = target.label();
        let escaped = escape_nix_string(arg_val);
        let inner = format!(
            r#"{} {{ neoFlake = f; {} = "{}"; }}"#,
            EXTRACT_SERVICE_OPTIONS.load_name, arg_name, escaped
        );
        let raw: RawPane = match self.query_json(&inner).await {
            Ok(v) => v,
            Err(e) => {
                eprintln!("web: nix extract_pane({label}) failed: {e:#}");
                let ctx = if target.is_core() {
                    format!("Nix error for core section ({label})")
                } else {
                    format!("Nix error for service options ({label})")
                };
                RawPane {
                    meta: None,
                    options: vec![],
                    units: vec![],
                    containers: std::collections::HashMap::new(),
                    appdata: None,
                    appdata_root: None,
                    error: Some(format_nix_failure(&ctx, &e)),
                }
            }
        };
        let mut opts = raw.options;
        enrich_options(&mut opts);
        if let PaneTarget::CoreSection(section) = &target {
            rename_scalar_core_option(section, &mut opts);
        }
        let options_json = serde_json::to_string(&opts).unwrap_or_else(|_| "[]".to_string());
        let units = map_units(raw.units, &raw.containers);
        OptionPaneContext {
            service: label.to_string(),
            meta: raw.meta,
            options: opts,
            options_json,
            save_endpoint: target.save_endpoint(),
            is_core: target.is_core(),
            error: raw.error,
            units,
            containers: raw.containers,
            appdata: raw.appdata.filter(|p| !p.is_empty()),
            appdata_root: raw.appdata_root.filter(|p| !p.is_empty()),
        }
    }

    pub async fn extract_service_options(&mut self, service: &str) -> OptionPaneContext {
        self.extract_pane(PaneTarget::Service(service.to_string()))
            .await
    }

    pub async fn extract_neo_section(&mut self, section: &str) -> OptionPaneContext {
        self.extract_pane(PaneTarget::CoreSection(section.to_string()))
            .await
    }

    pub async fn extract_proxied_services(&mut self) -> NavigatorContext {
        let inner = format!("{} {{ neoFlake = f; }}", EXTRACT_PROXIED_SERVICES.load_name);
        let mut ctx: NavigatorContext = match self.query_json(&inner).await {
            Ok(v) => v,
            Err(e) => {
                eprintln!("web: nix extract_proxied_services failed: {e:#}");
                NavigatorContext {
                    domain: None,
                    services: vec![],
                    theme: String::new(),
                    error: Some(format_nix_failure(
                        "Nix error while building navigator",
                        &e,
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
