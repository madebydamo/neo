//! What pages ask the pool for: query keys, cached results and the typed
//! accessors that turn them into page contexts.
use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{anyhow, Result};
use serde_json::Value;

use super::super::errors::eval_error_ui;
use super::super::extract::{error_pane_for, OptionIndex};
use super::super::overlay;
use super::super::repl::NixEvaluator;
use super::EvalPool;
use crate::commands::web::guides::GuidesExtract;
use crate::commands::web::routes::privacy::PrivacyExtract;
use crate::commands::web::types::{
    EvalErrorUi, EvalMark, IndexContext, NavigatorContext, OptionPaneContext,
};

/// One eval-backed query. `id()` is what pages and the WS push name it by.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum QueryKey {
    Services,
    Theme,
    Navigator,
    Guides,
    Privacy,
    OptionIndex,
    SecretPaths,
    PluginOwners,
    Pane { core: bool, name: String },
}

impl QueryKey {
    pub fn id(&self) -> String {
        match self {
            Self::Services => "services".into(),
            Self::Theme => "theme".into(),
            Self::Navigator => "navigator".into(),
            Self::Guides => "guides".into(),
            Self::Privacy => "privacy".into(),
            Self::OptionIndex => "option-index".into(),
            Self::SecretPaths => "secret-paths".into(),
            Self::PluginOwners => "plugin-owners".into(),
            Self::Pane { core: true, name } => format!("pane:core:{name}"),
            Self::Pane { core: false, name } => format!("pane:service:{name}"),
        }
    }

    /// Forms and what saves read (secret paths, plugin owners) must match the
    /// files they write: never served from an older structure. Read-only pages
    /// (guides, privacy map, theme, option index) may be.
    pub(super) fn waits_for_structure(&self) -> bool {
        self.structural() || matches!(self, Self::Pane { .. } | Self::SecretPaths)
    }

    /// Lists of services and plugins: computed before a structural change
    /// swaps in, since the pages listing services wait for them.
    pub(super) fn structural(&self) -> bool {
        match self {
            Self::Services | Self::Navigator | Self::PluginOwners => true,
            Self::Pane { core, name } => *core && name == "plugins",
            _ => false,
        }
    }

    fn label(&self) -> &'static str {
        match self {
            Self::Services => "Nix error while extracting service list",
            Self::Navigator => "Nix error while building navigator",
            Self::Guides => "Nix error while extracting guides",
            Self::Privacy => "Nix error while building the privacy map",
            Self::Pane { core: true, .. } => "Nix error for core section",
            Self::Pane { core: false, .. } => "Nix error for service options",
            _ => "Nix error",
        }
    }
}

/// A cached query result.
#[derive(Clone)]
pub(super) enum Cached {
    Services(Arc<IndexContext>),
    Theme(Arc<String>),
    Navigator(Arc<NavigatorContext>),
    Pane(Arc<OptionPaneContext>),
    Guides(Arc<GuidesExtract>),
    Privacy(Arc<PrivacyExtract>),
    OptionIndex(Arc<OptionIndex>),
    SecretPaths(Arc<Vec<Vec<String>>>),
    PluginOwners(Arc<HashMap<String, Vec<String>>>),
}

impl Cached {
    /// Stable serialization for comparing two evaluations.
    pub(super) fn json(&self) -> String {
        let v = match self {
            Self::Services(x) => serde_json::to_value(&**x),
            Self::Theme(x) => serde_json::to_value(&**x),
            Self::Navigator(x) => serde_json::to_value(&**x),
            Self::Pane(x) => serde_json::to_value(&**x),
            Self::Guides(x) => serde_json::to_value(&**x),
            Self::Privacy(x) => serde_json::to_value(&**x),
            Self::OptionIndex(x) => serde_json::to_value(&**x),
            Self::SecretPaths(x) => serde_json::to_value(&**x),
            Self::PluginOwners(x) => serde_json::to_value(&**x),
        };
        // serde_json::Value objects are sorted maps: HashMap order cannot differ.
        v.map(|v| v.to_string()).unwrap_or_default()
    }

    /// With `new` settings laid over a result evaluated from `old`.
    pub(super) fn overlaid(&self, old: &Value, new: &Value) -> Cached {
        if old.is_null() || new.is_null() {
            return self.clone();
        }
        match self {
            Self::Services(x) => {
                let mut c = (**x).clone();
                if overlay::services(&mut c, old, new) {
                    return Self::Services(Arc::new(c));
                }
            }
            Self::Pane(x) => {
                let mut c = (**x).clone();
                if overlay::pane(&mut c, old, new) {
                    return Self::Pane(Arc::new(c));
                }
            }
            _ => {}
        }
        self.clone()
    }
}

pub(super) async fn run_query(ev: &mut NixEvaluator, key: &QueryKey) -> Result<Cached> {
    Ok(match key {
        QueryKey::Services => Cached::Services(Arc::new(ev.extract_services().await?)),
        QueryKey::Theme => Cached::Theme(Arc::new(ev.extract_neo_theme().await?)),
        QueryKey::Navigator => Cached::Navigator(Arc::new(ev.extract_proxied_services().await?)),
        QueryKey::Guides => Cached::Guides(Arc::new(ev.extract_guides().await?)),
        QueryKey::Privacy => Cached::Privacy(Arc::new(ev.extract_privacy().await?)),
        QueryKey::OptionIndex => Cached::OptionIndex(Arc::new(ev.extract_option_index().await?)),
        QueryKey::SecretPaths => Cached::SecretPaths(Arc::new(ev.extract_secret_paths().await?)),
        QueryKey::PluginOwners => Cached::PluginOwners(Arc::new(ev.extract_plugin_owners().await?)),
        QueryKey::Pane { core: true, name } => {
            Cached::Pane(Arc::new(ev.extract_neo_section(name).await?))
        }
        QueryKey::Pane { core: false, name } => {
            Cached::Pane(Arc::new(ev.extract_service_options(name).await?))
        }
    })
}

// ---- typed queries ----------------------------------------------------------

impl EvalPool {
    pub async fn services(&self) -> IndexContext {
        let (r, mark) = self.get(&QueryKey::Services).await;
        let mut ctx = match r {
            Ok(Cached::Services(x)) => (*x).clone(),
            other => IndexContext {
                eval_error: self.error_ui(&QueryKey::Services, other.err()),
                ..Default::default()
            },
        };
        ctx.eval = mark;
        ctx
    }

    pub async fn theme(&self) -> String {
        match self.get(&QueryKey::Theme).await.0 {
            Ok(Cached::Theme(t)) => (*t).clone(),
            _ => "light".to_string(),
        }
    }

    pub async fn navigator(&self) -> NavigatorContext {
        match self.get(&QueryKey::Navigator).await.0 {
            Ok(Cached::Navigator(n)) => (*n).clone(),
            other => NavigatorContext {
                eval_error: self.error_ui(&QueryKey::Navigator, other.err()),
                ..Default::default()
            },
        }
    }

    /// A service (or core section) pane, settings saved since laid over it.
    /// Secrets are still placeholders on encrypted configs (panes.rs reveals them).
    pub async fn pane(&self, is_core: bool, name: &str) -> OptionPaneContext {
        let key = QueryKey::Pane {
            core: is_core,
            name: name.to_string(),
        };
        let (r, mark) = self.get(&key).await;
        let mut pane = match r {
            Ok(Cached::Pane(p)) => (*p).clone(),
            other => {
                let mut p = error_pane_for(name, is_core);
                let what = format!("{} ({name})", key.label());
                p.eval_error = match other {
                    Err(e) => eval_error_ui(&what, &e),
                    Ok(_) => EvalErrorUi::message(what),
                };
                p
            }
        };
        pane.eval = mark;
        pane
    }

    pub async fn guides(&self) -> (Result<GuidesExtract>, EvalMark) {
        let (r, mark) = self.get(&QueryKey::Guides).await;
        let r = r.and_then(|c| match c {
            Cached::Guides(g) => Ok((*g).clone()),
            _ => Err(anyhow!("unexpected cache entry")),
        });
        (r, mark)
    }

    pub async fn privacy(&self) -> (Result<PrivacyExtract>, EvalMark) {
        let (r, mark) = self.get(&QueryKey::Privacy).await;
        let r = r.and_then(|c| match c {
            Cached::Privacy(p) => Ok((*p).clone()),
            _ => Err(anyhow!("unexpected cache entry")),
        });
        (r, mark)
    }

    pub async fn option_index(&self) -> Result<OptionIndex> {
        match self.get(&QueryKey::OptionIndex).await.0? {
            Cached::OptionIndex(i) => Ok((*i).clone()),
            _ => Err(anyhow!("unexpected cache entry")),
        }
    }

    pub async fn secret_paths(&self) -> Result<Vec<Vec<String>>> {
        match self.get(&QueryKey::SecretPaths).await.0? {
            Cached::SecretPaths(p) => Ok((*p).clone()),
            _ => Err(anyhow!("unexpected cache entry")),
        }
    }

    pub async fn plugin_owners(&self) -> Result<HashMap<String, Vec<String>>> {
        match self.get(&QueryKey::PluginOwners).await.0? {
            Cached::PluginOwners(o) => Ok((*o).clone()),
            _ => Err(anyhow!("unexpected cache entry")),
        }
    }

    fn error_ui(&self, key: &QueryKey, err: Option<anyhow::Error>) -> EvalErrorUi {
        let err = err.unwrap_or_else(|| anyhow!("unexpected cache entry"));
        eprintln!("web: {} failed: {err:#}", key.id());
        eval_error_ui(key.label(), &err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_ids_and_structure() {
        assert_eq!(
            QueryKey::Pane {
                core: false,
                name: "a".into()
            }
            .id(),
            "pane:service:a"
        );
        assert_eq!(
            QueryKey::Pane {
                core: true,
                name: "ssh".into()
            }
            .id(),
            "pane:core:ssh"
        );
        assert!(QueryKey::Services.structural());
        assert!(QueryKey::Pane {
            core: true,
            name: "plugins".into()
        }
        .structural());
        assert!(!QueryKey::Pane {
            core: false,
            name: "plugins".into()
        }
        .structural());
        assert!(!QueryKey::Theme.structural());
        // Forms and the secret list a save routes by never come from an older structure.
        assert!(QueryKey::Pane {
            core: false,
            name: "a".into()
        }
        .waits_for_structure());
        assert!(QueryKey::SecretPaths.waits_for_structure());
        assert!(QueryKey::PluginOwners.waits_for_structure());
        assert!(!QueryKey::Guides.waits_for_structure());
        assert!(!QueryKey::Privacy.waits_for_structure());
    }
}
