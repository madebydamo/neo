//! `core.plugins` in settings.toml: plugin instances keyed by id.
//!
//! ```toml
//! [core.plugins.portrait-dev]
//! url = "github:madebydamo/portrait/dev"
//! prefix = "dev"
//! ```
//!
//! The old form (a list of URLs) is still read; its ids come from the
//! repository name, `-<index>` on a clash. The same rules live in
//! `nix/lib/plugin.nix` and `templates/homeserver/modules/_plugins.nix`, so the
//! flake input `plugin-<id>` is the same whichever form a config uses.
//! Design: docs/design/plugin-instances.md.

use toml_edit::{DocumentMut, Item, Table};

/// One plugin instance.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PluginEntry {
    pub id: String,
    pub url: String,
    /// `""` keeps the plugin's service names; `dev` turns `portrait` into `dev-portrait`.
    pub prefix: String,
}

/// Display label from a flake URL: the repository name for `github:` /
/// `gitlab:` / `sourcehut:` refs (a trailing `/<branch>` is not the name),
/// otherwise the last path segment; query/fragment and `.git` stripped.
/// Labels may overlap; never use this as an identity.
pub fn plugin_label(url: &str) -> String {
    let trimmed = url.trim();
    let path = url_path(url);
    let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    let forge = ["github:", "gitlab:", "sourcehut:"]
        .iter()
        .any(|p| trimmed.starts_with(p));
    let name = match segments.as_slice() {
        [_, repo, ..] if forge => repo,
        [.., last] => last,
        [] => trimmed,
    };
    name.strip_suffix(".git").unwrap_or(name).to_string()
}

/// URL without scheme, query and fragment (`github:o/r?dir=x` → `o/r`).
pub fn url_path(url: &str) -> String {
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

/// Id from free text: lowercase `[a-z0-9-]`, never empty, no leading digit.
pub fn sanitize_id(s: &str) -> String {
    let lower = s.to_lowercase();
    let joined = lower
        .split(|c: char| !(c.is_ascii_lowercase() || c.is_ascii_digit()))
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    if joined.is_empty() {
        "plugin".to_string()
    } else if joined.starts_with(|c: char| c.is_ascii_digit()) {
        format!("plugin-{joined}")
    } else {
        joined
    }
}

/// `[a-z][a-z0-9-]*`
pub fn valid_id(id: &str) -> bool {
    id.starts_with(|c: char| c.is_ascii_lowercase())
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// Empty, or `[a-z][a-z0-9]*` (prefixed names stay valid subdomains).
pub fn valid_prefix(prefix: &str) -> bool {
    prefix.is_empty()
        || (prefix.starts_with(|c: char| c.is_ascii_lowercase())
            && prefix
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()))
}

/// Ids for the old list form, in list order.
pub fn ids_for_urls(urls: &[String]) -> Vec<String> {
    let ids: Vec<String> = urls.iter().map(|u| sanitize_id(&plugin_label(u))).collect();
    ids.iter()
        .enumerate()
        .map(|(i, id)| {
            if ids.iter().filter(|x| *x == id).count() > 1 {
                format!("{id}-{i}")
            } else {
                id.clone()
            }
        })
        .collect()
}

/// Read `core.plugins` in either form, in file order.
pub fn plugins_from_doc(doc: &DocumentMut) -> Vec<PluginEntry> {
    let Some(item) = doc.get("core").and_then(|c| c.get("plugins")) else {
        return Vec::new();
    };
    if let Some(arr) = item.as_array() {
        let urls: Vec<String> = arr
            .iter()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect();
        return ids_for_urls(&urls)
            .into_iter()
            .zip(urls)
            .map(|(id, url)| PluginEntry {
                id,
                url,
                prefix: String::new(),
            })
            .collect();
    }
    let Some(tbl) = item.as_table_like() else {
        return Vec::new();
    };
    tbl.iter()
        .map(|(id, v)| {
            let field = |k: &str| {
                v.as_table_like()
                    .and_then(|t| t.get(k))
                    .and_then(|x| x.as_str())
                    .unwrap_or_default()
                    .to_string()
            };
            PluginEntry {
                id: id.to_string(),
                url: field("url"),
                prefix: field("prefix"),
            }
        })
        .collect()
}

/// True when `core.plugins` is already `[core.plugins.<id>]` tables.
pub fn plugins_are_tables(doc: &DocumentMut) -> bool {
    match doc.get("core").and_then(|c| c.get("plugins")) {
        None => true,
        Some(Item::Table(t)) => t.iter().all(|(_, v)| v.is_table()),
        Some(_) => false,
    }
}

/// Write `core.plugins` as `[core.plugins.<id>]` tables (empty prefix left out).
/// Entries already present keep their table, so an edited URL is a one-line change.
pub fn write_plugins(doc: &mut DocumentMut, entries: &[PluginEntry]) {
    let Some(core) = super::settings::ensure_table(doc.as_table_mut(), "core") else {
        return;
    };
    if entries.is_empty() {
        core.remove("plugins");
        return;
    }
    let mut old = match core.remove("plugins") {
        Some(Item::Table(t)) => t,
        _ => Table::new(),
    };
    let mut plugins = Table::new();
    plugins.set_implicit(true);
    for e in entries {
        let mut t = match old.remove(&e.id) {
            Some(Item::Table(t)) => t,
            _ => Table::new(),
        };
        if t.get("url").and_then(|v| v.as_str()) != Some(e.url.as_str()) {
            t.insert("url", toml_edit::value(e.url.clone()));
        }
        if e.prefix.is_empty() {
            t.remove("prefix");
        } else if t.get("prefix").and_then(|v| v.as_str()) != Some(e.prefix.as_str()) {
            t.insert("prefix", toml_edit::value(e.prefix.clone()));
        }
        plugins.insert(&e.id, Item::Table(t));
    }
    core.insert("plugins", Item::Table(plugins));
}

/// Problems with a set of plugin instances (empty = fine).
pub fn validate_plugins(entries: &[PluginEntry]) -> Vec<String> {
    let mut errs = Vec::new();
    for (i, e) in entries.iter().enumerate() {
        if !valid_id(&e.id) {
            errs.push(format!(
                "plugin id '{}': use lowercase letters, digits and -, starting with a letter",
                e.id
            ));
        }
        if e.url.trim().is_empty() {
            errs.push(format!("plugin '{}': the address is empty", e.id));
        }
        if !valid_prefix(&e.prefix) {
            errs.push(format!(
                "plugin '{}': the prefix may only have lowercase letters and digits, starting with a letter",
                e.id
            ));
        }
        let same_repo = |o: &PluginEntry| repo_key(&o.url) == repo_key(&e.url);
        if entries[..i]
            .iter()
            .any(|o| same_repo(o) && o.prefix == e.prefix)
        {
            errs.push(format!(
                "plugin '{}': this plugin is already added{}. Give it a different prefix",
                e.id,
                if e.prefix.is_empty() {
                    " without a prefix".to_string()
                } else {
                    format!(" with the prefix '{}'", e.prefix)
                }
            ));
        }
    }
    errs
}

/// Repository identity of a URL, ignoring branch / ref (`github:o/r/dev` → `github:o/r`).
pub fn repo_key(url: &str) -> String {
    let trimmed = url.trim();
    let base = trimmed.split(['?', '#']).next().unwrap_or(trimmed);
    let base = base.trim_end_matches('/');
    for forge in ["github:", "gitlab:", "sourcehut:"] {
        if let Some(rest) = base.strip_prefix(forge) {
            let parts: Vec<&str> = rest.split('/').take(2).collect();
            return format!("{forge}{}", parts.join("/")).to_lowercase();
        }
    }
    let file = base
        .strip_prefix("git+file://")
        .or_else(|| base.strip_prefix("git+file:"))
        .or_else(|| base.strip_prefix("file://"))
        .or_else(|| base.strip_prefix("file:"));
    match file {
        Some(rest) => format!("git+file:/{}", rest.trim_start_matches('/')),
        None => base.trim_end_matches(".git").to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str, url: &str, prefix: &str) -> PluginEntry {
        PluginEntry {
            id: id.into(),
            url: url.into(),
            prefix: prefix.into(),
        }
    }

    #[test]
    fn plugin_label_uses_repo_or_last_segment() {
        assert_eq!(plugin_label("github:madebydamo/highsea.neo"), "highsea.neo");
        assert_eq!(plugin_label("github:madebydamo/high_sea/prod"), "high_sea");
        assert_eq!(
            plugin_label("git+file:/home/me/high_sea?ref=main"),
            "high_sea"
        );
        assert_eq!(plugin_label("path:/opt/plugins/portrait/"), "portrait");
        assert_eq!(
            plugin_label("git+https://github.com/me/portrait.git?ref=prod"),
            "portrait"
        );
        assert_eq!(plugin_label("   "), "");
    }

    #[test]
    fn sanitize_id_makes_valid_ids() {
        assert_eq!(sanitize_id("highsea.neo"), "highsea-neo");
        assert_eq!(sanitize_id("bielCrawler"), "bielcrawler");
        assert_eq!(sanitize_id("high_sea"), "high-sea");
        assert_eq!(sanitize_id("3d"), "plugin-3d");
        assert_eq!(sanitize_id("..."), "plugin");
        assert!(valid_id(&sanitize_id("Some Thing!")));
    }

    #[test]
    fn list_ids_disambiguate_clashes_by_index() {
        let urls = vec![
            "github:a/portrait".to_string(),
            "github:madebydamo/highsea.neo".to_string(),
            "path:/x/portrait".to_string(),
        ];
        assert_eq!(
            ids_for_urls(&urls),
            vec!["portrait-0", "highsea-neo", "portrait-2"]
        );
    }

    #[test]
    fn reads_list_and_table_forms() {
        let list: DocumentMut = "[core]\nplugins = [\"github:me/portrait\"]\n"
            .parse()
            .unwrap();
        assert_eq!(
            plugins_from_doc(&list),
            vec![entry("portrait", "github:me/portrait", "")]
        );
        let tables: DocumentMut = r#"
[core.plugins.portrait]
url = "github:me/portrait"

[core.plugins.portrait-dev]
url = "github:me/portrait/dev"
prefix = "dev"
"#
        .parse()
        .unwrap();
        assert_eq!(
            plugins_from_doc(&tables),
            vec![
                entry("portrait", "github:me/portrait", ""),
                entry("portrait-dev", "github:me/portrait/dev", "dev"),
            ]
        );
        assert!(plugins_are_tables(&tables));
        assert!(!plugins_are_tables(&list));
    }

    #[test]
    fn write_plugins_turns_a_list_into_tables() {
        let mut doc: DocumentMut = "[core]\nhostname = \"x\"\nplugins = [\"github:me/portrait\"]\n"
            .parse()
            .unwrap();
        let entries = plugins_from_doc(&doc);
        write_plugins(&mut doc, &entries);
        let out = doc.to_string();
        assert!(
            out.contains("[core.plugins.portrait]\nurl = \"github:me/portrait\"\n"),
            "{out}"
        );
        assert!(!out.contains("[core.plugins]\n"), "{out}");
        assert_eq!(plugins_from_doc(&doc), entries);
    }

    #[test]
    fn editing_a_url_changes_one_line() {
        let before = r#"[core.plugins.a]
url = "github:me/a"

[core.plugins.portrait]
url = "github:me/portrait"
"#;
        let mut doc: DocumentMut = before.parse().unwrap();
        let mut entries = plugins_from_doc(&doc);
        entries[1].url = "github:me/portrait/dev".into();
        write_plugins(&mut doc, &entries);
        let after = doc.to_string();
        let changed: Vec<_> = before
            .lines()
            .zip(after.lines())
            .filter(|(a, b)| a != b)
            .collect();
        assert_eq!(
            changed,
            vec![(
                "url = \"github:me/portrait\"",
                "url = \"github:me/portrait/dev\""
            )]
        );
        assert_eq!(before.lines().count(), after.lines().count());
    }

    #[test]
    fn write_plugins_drops_empty_prefix_and_the_table_when_empty() {
        let mut doc: DocumentMut = "[core.plugins.a]\nurl = \"u\"\nprefix = \"x\"\n"
            .parse()
            .unwrap();
        write_plugins(&mut doc, &[entry("a", "u", "")]);
        assert!(!doc.to_string().contains("prefix"));
        write_plugins(&mut doc, &[]);
        assert!(doc.get("core").unwrap().get("plugins").is_none());
    }

    #[test]
    fn validate_rejects_same_repo_with_same_prefix() {
        let ok = vec![
            entry("portrait", "github:me/portrait", ""),
            entry("portrait-dev", "github:me/portrait/dev", "dev"),
        ];
        assert!(validate_plugins(&ok).is_empty());
        let clash = vec![
            entry("portrait", "github:me/portrait", ""),
            entry("portrait-dev", "github:me/portrait/dev", ""),
        ];
        assert_eq!(validate_plugins(&clash).len(), 1);
        let bad = vec![entry("1x", "", "Dev-")];
        assert_eq!(validate_plugins(&bad).len(), 3);
    }

    #[test]
    fn repo_key_ignores_refs() {
        assert_eq!(repo_key("github:Me/Portrait/dev"), "github:me/portrait");
        assert_eq!(repo_key("github:me/portrait"), "github:me/portrait");
        assert_eq!(
            repo_key("git+file:///home/me/p?ref=dev"),
            repo_key("git+file:/home/me/p")
        );
    }
}
