//! Plugin ownership, badges and settings cleanup when a plugin instance is removed.
//!
//! Instances are keyed by their `core.plugins` id (crate::utils::plugins).
//! Ownership comes from extract_plugin_inventory.nix and always keys on the id,
//! so editing an instance's URL never drops its settings.

use std::collections::{HashMap, HashSet};

use toml_edit::DocumentMut;

use super::types::{PluginInventoryEntry, Service, ServicePlugin};
pub use crate::utils::plugins::plugin_label;

/// Services whose settings tables should be dropped when instances are removed.
///
/// A service is removed only when **every** instance that declares it is gone.
/// Core services (no owners) stay. A service declared by two instances stays if
/// either remains.
pub fn services_to_remove(
    owners: &HashMap<String, Vec<String>>,
    old_ids: &[String],
    new_ids: &[String],
) -> Vec<String> {
    let new_set: HashSet<&String> = new_ids.iter().collect();
    let removed: HashSet<&String> = old_ids.iter().filter(|id| !new_set.contains(id)).collect();
    if removed.is_empty() {
        return Vec::new();
    }
    let mut gone: Vec<String> = owners
        .iter()
        .filter(|(_, ids)| !ids.is_empty() && ids.iter().all(|id| removed.contains(id)))
        .map(|(service, _)| service.clone())
        .collect();
    gone.sort();
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

/// Badge/filter records for instance ids (ids are unique, so they are the label).
pub fn plugin_badges(ids: &[String], inventory: &[PluginInventoryEntry]) -> Vec<ServicePlugin> {
    ids.iter()
        .map(|id| {
            let url = inventory
                .iter()
                .find(|p| &p.id == id)
                .map(|p| p.url.clone())
                .unwrap_or_default();
            ServicePlugin {
                id: id.clone(),
                url,
                display: id.clone(),
            }
        })
        .collect()
}

pub fn plugin_filters(inventory: &[PluginInventoryEntry]) -> Vec<ServicePlugin> {
    let ids: Vec<String> = inventory.iter().map(|p| p.id.clone()).collect();
    plugin_badges(&ids, inventory)
}

pub fn attach_service_plugin_badges(services: &mut [Service], inventory: &[PluginInventoryEntry]) {
    for svc in services {
        svc.plugins = plugin_badges(&svc.plugin_ids, inventory);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn remove_only_services_owned_solely_by_removed_instances() {
        let mut owners = HashMap::new();
        owners.insert("jellyfin".into(), ids(&["high-sea"]));
        owners.insert("filebrowser".into(), vec![]); // core
        let gone = services_to_remove(
            &owners,
            &ids(&["high-sea", "portrait"]),
            &ids(&["portrait"]),
        );
        assert_eq!(gone, vec!["jellyfin"]);
    }

    #[test]
    fn removing_one_of_two_instances_keeps_the_others_tables() {
        let mut owners = HashMap::new();
        owners.insert("portrait".into(), ids(&["portrait"]));
        owners.insert("dev-portrait".into(), ids(&["portrait-dev"]));
        let gone = services_to_remove(
            &owners,
            &ids(&["portrait", "portrait-dev"]),
            &ids(&["portrait"]),
        );
        assert_eq!(gone, vec!["dev-portrait"]);
    }

    #[test]
    fn shared_owner_keeps_service_while_one_instance_remains() {
        let mut owners = HashMap::new();
        owners.insert("shared".into(), ids(&["a", "b"]));
        assert!(services_to_remove(&owners, &ids(&["a", "b"]), &ids(&["b"])).is_empty());
        assert_eq!(
            services_to_remove(&owners, &ids(&["a", "b"]), &[]),
            vec!["shared"]
        );
    }

    #[test]
    fn same_ids_remove_nothing() {
        let mut owners = HashMap::new();
        owners.insert("jellyfin".into(), ids(&["high-sea"]));
        assert!(services_to_remove(&owners, &ids(&["high-sea"]), &ids(&["high-sea"])).is_empty());
        assert!(services_to_remove(&owners, &[], &ids(&["high-sea"])).is_empty());
    }

    #[test]
    fn badges_use_the_id_and_carry_the_url() {
        let inv = vec![PluginInventoryEntry {
            id: "portrait-dev".into(),
            url: "github:me/portrait/dev".into(),
            prefix: "dev".into(),
            services: ids(&["dev-portrait"]),
        }];
        let b = plugin_badges(&ids(&["portrait-dev"]), &inv);
        assert_eq!(b[0].display, "portrait-dev");
        assert_eq!(b[0].url, "github:me/portrait/dev");
    }

    #[test]
    fn strip_service_tables_removes_only_named_services() {
        let raw = r#"
[services.jellyfin]
enabled = true

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
