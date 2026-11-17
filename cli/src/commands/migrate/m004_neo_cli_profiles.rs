use toml_edit::{DocumentMut, Item};

use super::{insert_dotted, profile_has_key, remove_dotted};

/// Move top-level neo-cli.configPath into local/server profile tables.
pub(super) fn migrate(doc: &mut DocumentMut) {
    let path_item = remove_dotted(doc, "neo-cli.configPath");
    let path_str = path_item
        .as_ref()
        .and_then(|i| i.as_str())
        .map(|s| s.to_string());

    if let Some(path) = path_str {
        let localish = is_local_config_path(&path);
        let serverish = is_server_config_path(&path);

        let has_local = profile_has_key(doc, "local", "configPath");
        let has_server = profile_has_key(doc, "server", "configPath");

        if serverish && !localish {
            if !has_server {
                insert_dotted(
                    doc,
                    "neo-cli.server.configPath",
                    Item::Value(toml_edit::Value::from(path.as_str())),
                );
            }
        } else if localish && !serverish {
            if !has_local {
                insert_dotted(
                    doc,
                    "neo-cli.local.configPath",
                    Item::Value(toml_edit::Value::from(path.as_str())),
                );
            }
        } else {
            // Ambiguous: keep on both profiles if missing.
            if !has_local {
                insert_dotted(
                    doc,
                    "neo-cli.local.configPath",
                    Item::Value(toml_edit::Value::from(path.as_str())),
                );
            }
            if !has_server {
                insert_dotted(
                    doc,
                    "neo-cli.server.configPath",
                    Item::Value(toml_edit::Value::from(path.as_str())),
                );
            }
        }
    }
}

fn is_local_config_path(path: &str) -> bool {
    let p = path.trim();
    if p.is_empty() {
        return false;
    }
    if p.starts_with("./") || p.starts_with("../") {
        return true;
    }
    if !p.starts_with('/') {
        return true; // relative
    }
    // Absolute under a home directory is typically laptop scaffolding.
    p.starts_with("/home/") || p.starts_with("/Users/")
}

fn is_server_config_path(path: &str) -> bool {
    let p = path.trim();
    p.contains("/var/neo") || p.contains("AppData/configuration") || p.contains("/DATA/AppData/")
}
