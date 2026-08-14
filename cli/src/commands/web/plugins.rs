//! Plugin identity, ownership, and settings cleanup when a plugin URL is removed.
//!
//! Labels are the last path segment of a flake URL and are **not** unique.
//! Ownership and uninstall always key on the full URL.

use std::collections::{HashMap, HashSet};

use toml_edit::DocumentMut;

use super::types::{PluginFilter, PluginInventoryEntry, Service, ServicePlugin};

/// Canonical flake URL so lock originals and settings.toml entries compare equal.
/// `file:///path` and `git+file:///path` become `git+file:/path`.
pub fn normalize_plugin_url(url: &str) -> String {
    let mut s = url.trim();
    if let Some(i) = s.find('#') {
        s = &s[..i];
    }
    let (base, query) = match s.find('?') {
        Some(i) => (&s[..i], &s[i..]),
        None => (s, ""),
    };
    let file_rest = base
        .strip_prefix("git+file://")
        .or_else(|| base.strip_prefix("git+file:"))
        .or_else(|| base.strip_prefix("file://"))
        .or_else(|| base.strip_prefix("file:"));
    if let Some(rest) = file_rest {
        let path = rest.trim_start_matches('/');
        return format!("git+file:/{path}{query}");
    }
    format!("{base}{query}")
}

/// Display label from a flake URL: last path segment, query/fragment stripped.
/// Labels may overlap; never use this as an identity.
pub fn plugin_label(url: &str) -> String {
    let path = url_path(url);
    path.rsplit('/')
        .find(|s| !s.is_empty())
        .unwrap_or(url.trim())
        .to_string()
}

/// Services whose settings tables should be dropped when the plugin list changes.
///
/// A service is removed only when **every** plugin URL that declares it is in
/// the removed set. Core services (no plugin owners) stay. A service declared
/// by two plugins stays if either plugin remains.
pub fn services_to_remove(
    owners: &HashMap<String, Vec<String>>,
    old_plugins: &[String],
    new_plugins: &[String],
) -> Vec<String> {
    let new_set: HashSet<String> = new_plugins.iter().map(|u| normalize_plugin_url(u)).collect();
    let removed: HashSet<String> = old_plugins
        .iter()
        .map(|u| normalize_plugin_url(u))
        .filter(|u| !new_set.contains(u))
        .collect();
    if removed.is_empty() {
        return Vec::new();
    }
    let mut gone: Vec<String> = owners
        .iter()
        .filter_map(|(service, urls)| {
            if urls.is_empty() {
                return None;
            }
            if urls
                .iter()
                .all(|u| removed.contains(&normalize_plugin_url(u)))
            {
                Some(service.clone())
            } else {
                None
            }
        })
        .collect();
    gone.sort();
    gone
}

/// Read `core.plugins` (list of flake URL strings) from settings.toml.
pub fn plugins_from_doc(doc: &DocumentMut) -> Vec<String> {
    doc.get("core")
        .and_then(|c| c.get("plugins"))
        .and_then(|p| p.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default()
}

/// Drop settings tables for services that only the removed plugin URLs declared.
/// Returns the service names that were stripped.
#[cfg(test)]
pub fn apply_plugin_removal(
    doc: &mut DocumentMut,
    new_plugins: &[String],
    owners: &HashMap<String, Vec<String>>,
) -> Vec<String> {
    let old = plugins_from_doc(doc);
    let gone = services_to_remove(owners, &old, new_plugins);
    strip_service_tables(doc, &gone);
    gone
}

/// Delete `[services.<name>]` tables. Returns true if anything was removed.
pub fn strip_service_tables(doc: &mut DocumentMut, services: &[String]) -> bool {
    if services.is_empty() {
        return false;
    }
    let Some(table) = doc.get_mut("services").and_then(|s| s.as_table_mut()) else {
        return false;
    };
    let mut changed = false;
    for name in services {
        if table.remove(name).is_some() {
            changed = true;
        }
    }
    changed
}

/// Human filter/badge text. Duplicate labels get a short URL suffix.
pub fn display_label(url: &str, label: &str, label_counts: &HashMap<String, usize>) -> String {
    if label_counts.get(label).copied().unwrap_or(0) <= 1 {
        return label.to_string();
    }
    format!("{label} · {}", url_path(url))
}

pub fn count_labels<'a, I>(labels: I) -> HashMap<String, usize>
where
    I: IntoIterator<Item = &'a str>,
{
    let mut counts = HashMap::new();
    for label in labels {
        *counts.entry(label.to_string()).or_insert(0) += 1;
    }
    counts
}

/// Badge/filter records for a set of plugin URLs. Labels may overlap; `display` disambiguates.
pub fn plugin_badges(urls: &[String], all_plugin_urls: &[String]) -> Vec<ServicePlugin> {
    let labels: Vec<String> = all_plugin_urls.iter().map(|u| plugin_label(u)).collect();
    let counts = count_labels(labels.iter().map(String::as_str));
    urls.iter()
        .map(|url| {
            let label = plugin_label(url);
            ServicePlugin {
                url: url.clone(),
                label: label.clone(),
                display: display_label(url, &label, &counts),
            }
        })
        .collect()
}

pub fn plugin_filters(inventory: &[PluginInventoryEntry]) -> Vec<PluginFilter> {
    let urls: Vec<String> = inventory.iter().map(|p| p.url.clone()).collect();
    plugin_badges(&urls, &urls)
        .into_iter()
        .map(|b| PluginFilter {
            url: b.url,
            label: b.label,
            display: b.display,
        })
        .collect()
}

pub fn attach_service_plugin_badges(services: &mut [Service], inventory: &[PluginInventoryEntry]) {
    let all: Vec<String> = inventory.iter().map(|p| p.url.clone()).collect();
    for svc in services {
        svc.plugins = plugin_badges(&svc.plugin_urls, &all);
    }
}

fn url_path(url: &str) -> String {
    let mut s = url.trim();
    if let Some(i) = s.find(['?', '#']) {
        s = &s[..i];
    }
    s = s.trim_end_matches('/');
    const PREFIXES: &[&str] = &[
        "git+file:",
        "git+https://",
        "git+http://",
        "https://",
        "http://",
        "path:",
        "github:",
        "gitlab:",
        "sourcehut:",
    ];
    for p in PREFIXES {
        if let Some(rest) = s.strip_prefix(p) {
            s = rest;
            break;
        }
    }
    s.trim_start_matches('/').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_plugin_url_unifies_file_forms() {
        assert_eq!(
            normalize_plugin_url("file:///home/me/high_sea"),
            "git+file:/home/me/high_sea"
        );
        assert_eq!(
            normalize_plugin_url("git+file:///home/me/high_sea"),
            "git+file:/home/me/high_sea"
        );
        assert_eq!(
            normalize_plugin_url("git+file:/home/me/high_sea"),
            "git+file:/home/me/high_sea"
        );
        assert_eq!(
            normalize_plugin_url("github:me/portrait"),
            "github:me/portrait"
        );
    }

    #[test]
    fn remove_matches_file_url_aliases() {
        let mut owners = HashMap::new();
        owners.insert(
            "jellyfin".into(),
            vec!["file:///p/high_sea".into()],
        );
        let old = vec!["git+file:/p/high_sea".into()];
        let new: Vec<String> = vec![];
        assert_eq!(services_to_remove(&owners, &old, &new), vec!["jellyfin"]);
    }

    #[test]
    fn plugin_label_uses_last_path_segment() {
        assert_eq!(plugin_label("github:madebydamo/highsea.neo"), "highsea.neo");
        assert_eq!(
            plugin_label("git+file:/home/damo/Documents/projects/homeserver/high_sea"),
            "high_sea"
        );
        assert_eq!(plugin_label("path:/opt/plugins/portrait"), "portrait");
        assert_eq!(
            plugin_label("git+https://github.com/madebydamo/portrait"),
            "portrait"
        );
    }

    #[test]
    fn plugin_label_strips_query_and_trailing_slash() {
        assert_eq!(
            plugin_label("git+file:/home/me/high_sea?ref=main"),
            "high_sea"
        );
        assert_eq!(plugin_label("path:/opt/plugins/portrait/"), "portrait");
        assert_eq!(
            plugin_label("github:madebydamo/highsea.neo?dir=modules"),
            "highsea.neo"
        );
    }

    #[test]
    fn plugin_label_can_overlap() {
        assert_eq!(plugin_label("git+file:/a/high_sea"), "high_sea");
        assert_eq!(plugin_label("github:other/high_sea"), "high_sea");
        assert_eq!(plugin_label("path:/tmp/high_sea"), "high_sea");
    }

    #[test]
    fn plugin_label_falls_back_to_trimmed_url() {
        assert_eq!(plugin_label("   "), "");
        assert_eq!(plugin_label("local"), "local");
    }

    #[test]
    fn remove_only_services_owned_solely_by_removed_plugins() {
        let mut owners = HashMap::new();
        owners.insert("jellyfin".into(), vec!["git+file:/p/high_sea".into()]);
        owners.insert("filebrowser".into(), vec![]); // core
        let old = vec!["git+file:/p/high_sea".into(), "github:me/portrait".into()];
        let new = vec!["github:me/portrait".into()];
        let gone = services_to_remove(&owners, &old, &new);
        assert_eq!(gone, vec!["jellyfin"]);
    }

    #[test]
    fn overlapping_owners_keep_service_if_one_plugin_remains() {
        let mut owners = HashMap::new();
        owners.insert(
            "shared".into(),
            vec![
                "git+file:/a/high_sea".into(),
                "github:other/high_sea".into(),
            ],
        );
        let old = vec![
            "git+file:/a/high_sea".into(),
            "github:other/high_sea".into(),
        ];
        let new = vec!["github:other/high_sea".into()];
        let gone = services_to_remove(&owners, &old, &new);
        assert!(gone.is_empty(), "shared service must stay: {gone:?}");
    }

    #[test]
    fn overlapping_owners_strip_when_all_declaring_plugins_go() {
        let mut owners = HashMap::new();
        owners.insert(
            "shared".into(),
            vec![
                "git+file:/a/high_sea".into(),
                "github:other/high_sea".into(),
            ],
        );
        let old = vec![
            "git+file:/a/high_sea".into(),
            "github:other/high_sea".into(),
        ];
        let new: Vec<String> = vec![];
        assert_eq!(services_to_remove(&owners, &old, &new), vec!["shared"]);
    }

    #[test]
    fn unknown_service_not_in_owners_is_core_and_stays() {
        let owners = HashMap::new();
        let old = vec!["git+file:/p/high_sea".into()];
        let new: Vec<String> = vec![];
        assert!(services_to_remove(&owners, &old, &new).is_empty());
    }

    #[test]
    fn adding_plugins_removes_nothing() {
        let mut owners = HashMap::new();
        owners.insert("jellyfin".into(), vec!["git+file:/p/high_sea".into()]);
        let old: Vec<String> = vec![];
        let new = vec!["git+file:/p/high_sea".into()];
        assert!(services_to_remove(&owners, &old, &new).is_empty());
    }

    #[test]
    fn display_label_disambiguates_when_labels_overlap() {
        let counts = count_labels(["high_sea", "high_sea", "portrait"]);
        assert_eq!(
            display_label("git+file:/a/high_sea", "high_sea", &counts),
            "high_sea · a/high_sea"
        );
        assert_eq!(
            display_label("github:me/portrait", "portrait", &counts),
            "portrait"
        );
    }

    #[test]
    fn plugins_from_doc_reads_core_list() {
        let raw = r#"
[core]
plugins = [
  "git+file:/p/high_sea",
  "github:me/portrait",
]
"#;
        let doc: DocumentMut = raw.parse().unwrap();
        assert_eq!(
            plugins_from_doc(&doc),
            vec!["git+file:/p/high_sea", "github:me/portrait"]
        );
    }

    #[test]
    fn apply_plugin_removal_strips_owned_tables_using_current_doc_list() {
        let raw = r#"
[core]
plugins = ["git+file:/p/high_sea", "github:me/portrait"]

[services.jellyfin]
enabled = true

[services.filebrowser]
enabled = true
"#;
        let mut doc: DocumentMut = raw.parse().unwrap();
        let mut owners = HashMap::new();
        owners.insert("jellyfin".into(), vec!["git+file:/p/high_sea".into()]);
        let gone = apply_plugin_removal(&mut doc, &["github:me/portrait".into()], &owners);
        assert_eq!(gone, vec!["jellyfin"]);
        let services = doc.get("services").unwrap().as_table().unwrap();
        assert!(services.get("jellyfin").is_none());
        assert!(services.get("filebrowser").is_some());
    }

    #[test]
    fn strip_service_tables_removes_only_named_services() {
        let raw = r#"
[services.jellyfin]
enabled = true
token = "secret"

[services.filebrowser]
enabled = true
"#;
        let mut doc: DocumentMut = raw.parse().unwrap();
        assert!(strip_service_tables(&mut doc, &["jellyfin".into()]));
        let services = doc.get("services").unwrap().as_table().unwrap();
        assert!(services.get("jellyfin").is_none());
        assert!(services.get("filebrowser").is_some());
    }
}
