use super::errors::{offers_flake_update, offers_store_repair, plan_for, NixError};
use super::registry::{
    EXTRACT_NEO_THEME, EXTRACT_PLUGIN_INVENTORY, EXTRACT_PROXIED_SERVICES, EXTRACT_SERVICES,
    EXTRACT_SERVICE_OPTIONS,
};
use super::repl::NixEvaluator;
use crate::commands::web::plugins::{attach_service_plugin_badges, plugin_badges, plugin_filters};
use crate::commands::web::types::{
    EvalErrorUi, ExtractedServiceGroups, IndexContext, NavigatorContext, OptionPaneContext,
    OptionSchema, RuntimeUnit, ServiceMeta,
};
use crate::commands::web::util::{escape_nix_string, service_name_ok};

struct FormattedNixFailure {
    message: String,
    kind_id: String,
    can_store_repair: bool,
    can_flake_update: bool,
}

/// Map an anyhow/nix failure into a stable UI string with kind + summary.
fn format_nix_failure(context: &str, err: &anyhow::Error) -> FormattedNixFailure {
    let nix_err = NixError::classify(&format!("{err:#}"));
    let plan = plan_for(nix_err.kind);
    let hint = if plan.help.is_empty() {
        String::new()
    } else {
        format!(" {}", plan.help)
    };
    FormattedNixFailure {
        message: format!("{context}: {}.{hint}", nix_err.display_message()),
        kind_id: nix_err.kind.id().to_string(),
        can_store_repair: offers_store_repair(nix_err.kind),
        can_flake_update: offers_flake_update(nix_err.kind),
    }
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
    #[serde(default, rename = "pluginUrls")]
    plugin_urls: Vec<String>,
    #[serde(default, rename = "pluginInventory")]
    plugin_inventory: Vec<crate::commands::web::types::PluginInventoryEntry>,
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

fn enrich_type(t: &mut crate::commands::web::types::OptionType) {
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
            Ok(mut extracted) => {
                let inventory = extracted.plugin_inventory.clone();
                for group in &mut extracted.groups {
                    attach_service_plugin_badges(&mut group.services, &inventory);
                }
                IndexContext {
                    groups: extracted.groups,
                    categories: extracted.categories,
                    plugin_filters: plugin_filters(&inventory),
                    ..Default::default()
                }
            }
            Err(e) => {
                eprintln!("web: nix extract_services failed: {e:#}");
                let f = format_nix_failure("Nix error while extracting service list", &e);
                IndexContext {
                    eval_error: EvalErrorUi::from_failure(
                        f.message,
                        f.kind_id,
                        f.can_store_repair,
                        f.can_flake_update,
                    ),
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
            eval_error: EvalErrorUi::message(reason),
            units: vec![],
            containers: std::collections::HashMap::new(),
            appdata: None,
            appdata_root: None,
            plugins: vec![],
            plugin_inventory_json: "[]".to_string(),
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
                let f = format_nix_failure(&ctx, &e);
                return OptionPaneContext {
                    service: label.to_string(),
                    meta: None,
                    options: vec![],
                    options_json: "[]".to_string(),
                    save_endpoint: target.save_endpoint(),
                    is_core: target.is_core(),
                    eval_error: EvalErrorUi::from_failure(
                        f.message,
                        f.kind_id,
                        f.can_store_repair,
                        f.can_flake_update,
                    ),
                    units: vec![],
                    containers: std::collections::HashMap::new(),
                    appdata: None,
                    appdata_root: None,
                    plugins: vec![],
                    plugin_inventory_json: "[]".to_string(),
                };
            }
        };
        let mut opts = raw.options;
        enrich_options(&mut opts);
        if let PaneTarget::CoreSection(section) = &target {
            rename_scalar_core_option(section, &mut opts);
        }
        let options_json = serde_json::to_string(&opts).unwrap_or_else(|_| "[]".to_string());
        let units = map_units(raw.units, &raw.containers);
        let inv_urls: Vec<String> = raw.plugin_inventory.iter().map(|p| p.url.clone()).collect();
        let plugins = plugin_badges(&raw.plugin_urls, &inv_urls);
        let plugin_inventory_json =
            serde_json::to_string(&raw.plugin_inventory).unwrap_or_else(|_| "[]".to_string());
        OptionPaneContext {
            service: label.to_string(),
            meta: raw.meta,
            options: opts,
            options_json,
            save_endpoint: target.save_endpoint(),
            is_core: target.is_core(),
            eval_error: raw.error.map(EvalErrorUi::message).unwrap_or_default(),
            units,
            containers: raw.containers,
            appdata: raw.appdata.filter(|p| !p.is_empty()),
            appdata_root: raw.appdata_root.filter(|p| !p.is_empty()),
            plugins,
            plugin_inventory_json,
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
                let f = format_nix_failure("Nix error while building navigator", &e);
                NavigatorContext {
                    domain: None,
                    services: vec![],
                    theme: String::new(),
                    eval_error: EvalErrorUi::from_failure(
                        f.message,
                        f.kind_id,
                        f.can_store_repair,
                        f.can_flake_update,
                    ),
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

    /// Service name → plugin flake URLs that declare it (from the current flake).
    pub async fn extract_plugin_owners(
        &mut self,
    ) -> anyhow::Result<std::collections::HashMap<String, Vec<String>>> {
        #[derive(serde::Deserialize)]
        struct RawOwners {
            #[serde(default)]
            owners: std::collections::HashMap<String, Vec<String>>,
        }
        let inner = format!("{} {{ neoFlake = f; }}", EXTRACT_PLUGIN_INVENTORY.load_name);
        let raw: RawOwners = self.query_json(&inner).await?;
        Ok(raw.owners)
    }
}
