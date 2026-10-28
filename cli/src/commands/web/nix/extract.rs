use super::errors::eval_error_ui;
use super::registry::{
    NixExtractor, EXTRACT_NEO_THEME, EXTRACT_PLUGIN_INVENTORY, EXTRACT_PROXIED_SERVICES,
    EXTRACT_SECRET_PATHS, EXTRACT_SERVICES, EXTRACT_SERVICE_OPTIONS,
};
use super::repl::NixEvaluator;
use crate::commands::web::plugins::{attach_service_plugin_badges, plugin_badges, plugin_filters};
use crate::commands::web::types::{
    EvalErrorUi, ExtractedServiceGroups, IndexContext, NavigatorContext, OptionPaneContext,
    OptionSchema, RuntimeUnit, ServiceMeta,
};
use crate::commands::web::util::{escape_nix_string, service_name_ok};

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
    timers: Vec<String>,
    #[serde(default, rename = "setupUnits")]
    setup_units: Vec<String>,
    #[serde(default, rename = "groupUnit")]
    group_unit: Option<String>,
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
    timers: &[String],
    setup: &[String],
    containers: &std::collections::HashMap<String, String>,
) -> Vec<RuntimeUnit> {
    units
        .into_iter()
        .map(|name| {
            let is_container = name.starts_with("docker-") || containers.contains_key(&name);
            let is_timer = timers.contains(&name);
            let is_setup = setup.contains(&name);
            RuntimeUnit {
                name,
                is_container,
                is_timer,
                is_setup,
            }
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
    /// Run an extractor that only takes the bound flake (`<load_name> { neoFlake = f; }`).
    async fn query_extractor<T: serde::de::DeserializeOwned>(
        &mut self,
        extractor: &NixExtractor,
    ) -> anyhow::Result<T> {
        self.query_json(&format!("{} {{ neoFlake = f; }}", extractor.load_name))
            .await
    }

    pub async fn extract_services(&mut self) -> IndexContext {
        match self
            .query_extractor::<ExtractedServiceGroups>(&EXTRACT_SERVICES)
            .await
        {
            Ok(mut extracted) => {
                let inventory = extracted.plugin_inventory.clone();
                for group in &mut extracted.groups {
                    attach_service_plugin_badges(&mut group.services, &inventory);
                    for service in &mut group.services {
                        self.icons.link(&mut service.icon);
                    }
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
                IndexContext {
                    eval_error: eval_error_ui("Nix error while extracting service list", &e),
                    ..Default::default()
                }
            }
        }
    }

    /// Empty pane that only carries an error banner.
    fn error_pane(target: &PaneTarget, eval_error: EvalErrorUi) -> OptionPaneContext {
        OptionPaneContext {
            service: target.label().to_string(),
            meta: None,
            options: vec![],
            sections: vec![],
            options_json: "[]".to_string(),
            save_endpoint: target.save_endpoint(),
            is_core: target.is_core(),
            eval_error,
            units: vec![],
            group_unit: None,
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
            let what = if target.is_core() {
                "section"
            } else {
                "service"
            };
            return Self::error_pane(
                &target,
                EvalErrorUi::message(format!("Invalid {what} name for nix extract")),
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
                return Self::error_pane(&target, eval_error_ui(&ctx, &e));
            }
        };
        let mut opts = raw.options;
        enrich_options(&mut opts);
        if let PaneTarget::CoreSection(section) = &target {
            rename_scalar_core_option(section, &mut opts);
        }
        super::sections::label_options(&mut opts);
        let mut sections = super::sections::build_sections(&opts);
        for section in &mut sections {
            self.icons.link(&mut section.icon);
            section.icon_tint = section.icon.as_deref().is_some_and(super::sections::is_svg);
        }
        let options_json = serde_json::to_string(&opts).unwrap_or_else(|_| "[]".to_string());
        let units = map_units(raw.units, &raw.timers, &raw.setup_units, &raw.containers);
        let mut meta = raw.meta;
        if let Some(meta) = meta.as_mut() {
            self.icons.link(&mut meta.icon);
        }
        let inv_urls: Vec<String> = raw.plugin_inventory.iter().map(|p| p.url.clone()).collect();
        let plugins = plugin_badges(&raw.plugin_urls, &inv_urls);
        let plugin_inventory_json =
            serde_json::to_string(&raw.plugin_inventory).unwrap_or_else(|_| "[]".to_string());
        OptionPaneContext {
            service: label.to_string(),
            meta,
            options: opts,
            sections,
            options_json,
            save_endpoint: target.save_endpoint(),
            is_core: target.is_core(),
            eval_error: raw.error.map(EvalErrorUi::message).unwrap_or_default(),
            units,
            group_unit: raw.group_unit.filter(|u| !u.is_empty()),
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
        let mut ctx: NavigatorContext = match self.query_extractor(&EXTRACT_PROXIED_SERVICES).await
        {
            Ok(v) => v,
            Err(e) => {
                eprintln!("web: nix extract_proxied_services failed: {e:#}");
                NavigatorContext {
                    eval_error: eval_error_ui("Nix error while building navigator", &e),
                    ..Default::default()
                }
            }
        };
        crate::commands::web::navigator::finish(&mut ctx, &self.icons);
        ctx
    }

    /// Warm the evaluator (navigator + theme). Returns the navigator eval error, if any.
    pub async fn warm_up(&mut self) -> Option<String> {
        let nav = self.extract_proxied_services().await;
        let _ = self.extract_neo_theme().await;
        nav.eval_error.error
    }

    pub async fn extract_neo_theme(&mut self) -> String {
        match self.query_extractor(&EXTRACT_NEO_THEME).await {
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
        let raw: RawOwners = self.query_extractor(&EXTRACT_PLUGIN_INVENTORY).await?;
        Ok(raw.owners)
    }

    /// Settings paths of every secret option ("*" = any attrsOf key).
    pub async fn extract_secret_paths(&mut self) -> anyhow::Result<Vec<Vec<String>>> {
        #[derive(serde::Deserialize)]
        struct RawPaths {
            #[serde(default)]
            paths: Vec<Vec<String>>,
        }
        let raw: RawPaths = self.query_extractor(&EXTRACT_SECRET_PATHS).await?;
        Ok(raw.paths)
    }
}
