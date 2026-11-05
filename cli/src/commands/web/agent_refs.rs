//! `@` references in the chat drawer (static/agent_chat.js).
//!
//! `GET /agent/refs` lists what can be referenced (services, settings, guides,
//! privacy categories); the drawer matches them fuzzily on the client. A sent
//! message's `@tokens` are resolved here into a short context block appended
//! to the message, so the assistant sees what each reference is and its
//! current value (secrets redacted, like the settings tools). The block is
//! cut off again when the chat history is shown.
//!
//! Tokens: `@nextcloud` (service), `@nextcloud.subdomain` / `@core.timeZone`
//! (setting), `@guide:<id>`, `@privacy:<id>`.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use serde::Serialize;
use serde_json::{json, Value};

use super::guides::{option_url, Guide};
use super::nix::{OptionIndex, OptionIndexEntry};
use super::redact;
use super::routes::privacy::PrivacyCategory;
use super::types::{AppConfig, OptionSchema, Service};
use super::util::service_name_ok;

/// Start of the context block appended to a sent message.
pub const BLOCK_START: &str = "\n\n<neo-references>";
const BLOCK_END: &str = "</neo-references>";
/// References resolved per message (more are left as plain text).
const MAX_REFS: usize = 8;
/// Settings listed for a referenced service.
const MAX_SERVICE_SETTINGS: usize = 40;
/// Longest value (as JSON text) copied into the block.
const MAX_VALUE_CHARS: usize = 400;

/// One candidate in the `@` popup.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct RefItem {
    /// `service` | `option` | `guide` | `privacy`
    pub kind: &'static str,
    /// What follows `@` in the message.
    pub token: String,
    pub label: String,
    /// Second line: tagline, owning service, guide summary.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub hint: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    /// Service installed (for a setting: its service is).
    pub enabled: bool,
    /// Web UI page of the item (chip link).
    pub url: String,
    /// Plumbing (`ui.group = advanced`): ranked last.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub advanced: bool,
    /// A secret setting: its value never leaves the server.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub secret: bool,
}

fn first_line(s: &str) -> String {
    s.lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or_default()
        .trim()
        .to_string()
}

fn option_item(root: &str, e: &OptionIndexEntry, enabled: bool, hint: &str) -> RefItem {
    let token = format!("{root}.{}", e.path);
    RefItem {
        kind: "option",
        url: option_url(&token),
        label: e.label.clone().unwrap_or_else(|| e.path.clone()),
        hint: hint.to_string(),
        icon: None,
        enabled,
        advanced: e.group.as_deref() == Some("advanced"),
        secret: e.secret,
        token,
    }
}

/// Candidates in display order for an empty query: installed services, guides,
/// privacy categories, then settings (installed services and core first).
pub fn candidates(
    services: &[Service],
    index: &OptionIndex,
    guides: &[Guide],
    privacy: &[PrivacyCategory],
) -> Vec<RefItem> {
    let mut svcs: Vec<&Service> = services.iter().collect();
    svcs.sort_by(|a, b| b.enabled.cmp(&a.enabled).then(a.name.cmp(&b.name)));
    let mut out: Vec<RefItem> = svcs
        .iter()
        .map(|s| RefItem {
            kind: "service",
            token: s.name.clone(),
            label: s.name.clone(),
            hint: if s.tagline.is_empty() {
                first_line(&s.description)
            } else {
                s.tagline.clone()
            },
            icon: s.icon.clone(),
            enabled: s.enabled,
            url: format!("/configuration/option/{}", s.name),
            advanced: false,
            secret: false,
        })
        .collect();
    out.extend(guides.iter().map(|g| RefItem {
        kind: "guide",
        token: format!("guide:{}", g.id),
        label: g.title.clone(),
        hint: g.summary.clone(),
        icon: g.icon.clone(),
        enabled: true,
        url: format!("/configuration/guides/{}", g.id),
        advanced: false,
        secret: false,
    }));
    out.extend(privacy.iter().map(|c| RefItem {
        kind: "privacy",
        token: format!("privacy:{}", c.id),
        label: c.label.clone(),
        hint: "Privacy map".to_string(),
        // Category icons are theme-tinted masks on the map; plain <img> here
        // would not follow the theme, so the popup shows its glyph instead.
        icon: None,
        enabled: c.home_now,
        url: format!("/configuration/privacy#{}", c.id),
        advanced: false,
        secret: false,
    }));
    let enabled = |name: &str| services.iter().any(|s| s.name == name && s.enabled);
    let mut opts: Vec<RefItem> = Vec::new();
    for (name, entries) in &index.services {
        let on = enabled(name);
        // `@<service>` already says whether it is installed.
        opts.extend(
            entries
                .iter()
                .filter(|e| e.path != "enabled")
                .map(|e| option_item(name, e, on, name)),
        );
    }
    opts.extend(
        index
            .core
            .iter()
            .map(|e| option_item("core", e, true, "Core settings")),
    );
    // Stable: installed first, plumbing last, otherwise extract order.
    opts.sort_by_key(|o| (!o.enabled, o.advanced));
    out.extend(opts);
    out
}

fn token_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | ':')
}

/// `@tokens` in a message, in order, unique, at most [`MAX_REFS`]. A token
/// starts after whitespace, an opening bracket or quote (so `me@host` is not
/// one); trailing sentence punctuation is dropped.
pub fn parse_refs(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let starts = chars[i] == '@'
            && (i == 0 || chars[i - 1].is_whitespace() || "([{\"'".contains(chars[i - 1]));
        if !starts {
            i += 1;
            continue;
        }
        let mut j = i + 1;
        while j < chars.len() && token_char(chars[j]) {
            j += 1;
        }
        let tok: String = chars[i + 1..j].iter().collect();
        let tok = tok.trim_end_matches(['.', ':', '-']).to_string();
        if tok
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphanumeric())
            && !out.contains(&tok)
        {
            out.push(tok);
            if out.len() == MAX_REFS {
                break;
            }
        }
        i = j.max(i + 1);
    }
    out
}

/// What a token names, before looking it up.
#[derive(Debug, PartialEq)]
pub enum RefTarget<'a> {
    Service(&'a str),
    /// `(service or "core", option path)`
    Option(&'a str, &'a str),
    Guide(&'a str),
    Privacy(&'a str),
}

pub fn target(token: &str) -> Option<RefTarget<'_>> {
    if let Some((kind, id)) = token.split_once(':') {
        let id_ok = !id.is_empty()
            && id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
        return match kind {
            "guide" if id_ok => Some(RefTarget::Guide(id)),
            "privacy" if id_ok => Some(RefTarget::Privacy(id)),
            _ => None,
        };
    }
    match token.split_once('.') {
        Some((root, path)) if (root == "core" || service_name_ok(root)) && !path.is_empty() => {
            Some(RefTarget::Option(root, path))
        }
        None if service_name_ok(token) && token != "core" => Some(RefTarget::Service(token)),
        _ => None,
    }
}

/// Text before the context block (what the user typed).
pub fn strip_block(content: &str) -> &str {
    match content.find(BLOCK_START) {
        Some(i) => &content[..i],
        None => content,
    }
}

/// A value small enough for the block: long text and big lists are cut.
fn compact(v: &Value) -> Value {
    let s = v.to_string();
    if s.chars().count() <= MAX_VALUE_CHARS {
        return v.clone();
    }
    let cut: String = s.chars().take(MAX_VALUE_CHARS).collect();
    json!(format!("{cut}… (truncated)"))
}

/// Current value with secrets redacted, also nested ones (same rule as the
/// settings tools).
fn redacted_current(o: &OptionSchema) -> Value {
    compact(&redact::redacted_current(o))
}

fn advanced(o: &OptionSchema) -> bool {
    o.ui.as_ref()
        .and_then(|u| u.group.as_ref())
        .is_some_and(|g| g.id == "advanced")
}

/// Context for one setting.
pub fn option_context(root: &str, o: &OptionSchema) -> Value {
    let path = format!("{root}.{}", o.name);
    let settings_path = if root == "core" {
        path.clone()
    } else {
        format!("services.{path}")
    };
    json!({
        "ref": format!("@{path}"),
        "kind": "setting",
        "path": settings_path,
        "label": o.label,
        "description": o.description,
        "type": o.typeLabel,
        "default": compact(&redact::redacted_default(o)),
        "current": redacted_current(o),
        "secret": o.secret,
        "link": format!("option:{path}"),
    })
}

/// Context for one service: what it is, whether it is installed, and its
/// everyday settings (plumbing left out).
pub fn service_context(s: &Service, opts: &[OptionSchema]) -> Value {
    let settings: Vec<Value> = opts
        .iter()
        .filter(|o| !advanced(o))
        .take(MAX_SERVICE_SETTINGS)
        .map(|o| json!({"path": format!("services.{}.{}", s.name, o.name), "label": o.label, "current": redacted_current(o)}))
        .collect();
    let description: String = s.description.chars().take(600).collect();
    json!({
        "ref": format!("@{}", s.name),
        "kind": "service",
        "name": s.name,
        "installed": s.enabled,
        "tagline": s.tagline,
        "description": description,
        "settings": settings,
        "link": format!("service:{}", s.name),
    })
}

pub fn guide_context(g: &Guide) -> Value {
    json!({
        "ref": format!("@guide:{}", g.id),
        "kind": "guide",
        "title": g.title,
        "summary": g.summary,
        "services": g.services.iter().map(|s| json!({"name": s.name, "installed": s.enabled})).collect::<Vec<_>>(),
        "optional": g.optional.iter().map(|s| s.name.clone()).collect::<Vec<_>>(),
        "skill": format!("neo-guide-{}", g.id),
        "link": format!("guide:{}", g.id),
    })
}

pub fn privacy_context(c: &PrivacyCategory) -> Value {
    json!({
        "ref": format!("@privacy:{}", c.id),
        "kind": "privacy category",
        "label": c.label,
        "in_the_cloud": c.cloud,
        "at_home": c.home,
        "kept_at_home_now": c.home_now,
        "services": c.services.iter().map(|s| json!({"name": s.name, "installed": s.enabled})).collect::<Vec<_>>(),
        "guides": c.guides.iter().map(|g| g.id.clone()).collect::<Vec<_>>(),
        "link": format!("privacy:{}", c.id),
    })
}

/// The block appended to the message, or `None` without resolved references.
pub fn block(items: &[Value]) -> Option<String> {
    if items.is_empty() {
        return None;
    }
    let mut out = String::from(BLOCK_START);
    out.push_str("\nThe user referenced these items with @ (neo web UI data at the time of this message; secrets redacted). Use the link values for Markdown links.\n");
    for v in items {
        out.push_str(&v.to_string());
        out.push('\n');
    }
    out.push_str(BLOCK_END);
    Some(out)
}

/// Resolve the message's references (unknown ones are skipped).
pub async fn context_for(config: &AppConfig, message: &str) -> Option<String> {
    use super::schema_cache::load_options;
    let tokens = parse_refs(message);
    if tokens.is_empty() {
        return None;
    }
    let targets: Vec<RefTarget> = tokens.iter().filter_map(|t| target(t)).collect();
    let need_services = targets.iter().any(|t| matches!(t, RefTarget::Service(_)));
    let need_guides = targets.iter().any(|t| matches!(t, RefTarget::Guide(_)));
    let need_privacy = targets.iter().any(|t| matches!(t, RefTarget::Privacy(_)));
    let services = if need_services {
        all_services(config).await
    } else {
        vec![]
    };
    let guides = if need_guides {
        super::routes::guides::load_guides(config)
            .await
            .map(|x| x.guides)
            .unwrap_or_default()
    } else {
        vec![]
    };
    let privacy = if need_privacy {
        config
            .evaluator
            .lock()
            .await
            .extract_privacy()
            .await
            .map(|x| x.categories)
            .unwrap_or_default()
    } else {
        vec![]
    };
    let mut items = Vec::new();
    for t in targets {
        match t {
            RefTarget::Service(name) => {
                if let Some(s) = services.iter().find(|s| s.name == name) {
                    let opts = load_options(config, false, name).await.unwrap_or_default();
                    items.push(service_context(s, &opts));
                }
            }
            RefTarget::Option(root, path) => {
                let opts = load_options(config, root == "core", root)
                    .await
                    .unwrap_or_default();
                if let Some(o) = opts.iter().find(|o| o.name == path) {
                    items.push(option_context(root, o));
                }
            }
            RefTarget::Guide(id) => {
                if let Some(g) = guides.iter().find(|g| g.id == id) {
                    items.push(guide_context(g));
                }
            }
            RefTarget::Privacy(id) => {
                if let Some(c) = privacy.iter().find(|c| c.id == id) {
                    items.push(privacy_context(c));
                }
            }
        }
    }
    block(&items)
}

async fn all_services(config: &AppConfig) -> Vec<Service> {
    let idx = config.evaluator.lock().await.extract_services().await;
    idx.groups.into_iter().flat_map(|g| g.services).collect()
}

/// Inputs whose change can change the candidates.
fn stamp(config: &AppConfig) -> Vec<Option<SystemTime>> {
    let dir = config
        .settings_path
        .parent()
        .unwrap_or(std::path::Path::new("."));
    [config.settings_path.clone(), dir.join("flake.lock")]
        .iter()
        .map(|p| std::fs::metadata(p).and_then(|m| m.modified()).ok())
        .collect()
}

struct Cached {
    stamp: Vec<Option<SystemTime>>,
    at: Instant,
    items: Arc<Vec<RefItem>>,
}

static CACHE: Mutex<Option<Cached>> = Mutex::new(None);
const TTL: Duration = Duration::from_secs(600);

/// Candidates, cached until settings.toml or flake.lock change (or 10 min).
pub async fn cached_candidates(config: &AppConfig) -> Result<Arc<Vec<RefItem>>, String> {
    let now = stamp(config);
    if let Ok(guard) = CACHE.lock() {
        if let Some(c) = guard.as_ref() {
            if c.stamp == now && c.at.elapsed() < TTL {
                return Ok(c.items.clone());
            }
        }
    }
    let services = all_services(config).await;
    let index = config
        .evaluator
        .lock()
        .await
        .extract_option_index()
        .await
        .map_err(|e| format!("{e:#}"))?;
    let (guides, guides_on) = match super::routes::guides::load_guides(config).await {
        Ok(x) => (x.guides, x.enabled),
        Err(_) => (vec![], false),
    };
    let privacy = if guides_on {
        config
            .evaluator
            .lock()
            .await
            .extract_privacy()
            .await
            .map(|x| if x.enabled { x.categories } else { vec![] })
            .unwrap_or_default()
    } else {
        vec![]
    };
    let guides = if guides_on { guides } else { vec![] };
    let items = Arc::new(candidates(&services, &index, &guides, &privacy));
    if let Ok(mut guard) = CACHE.lock() {
        *guard = Some(Cached {
            stamp: now,
            at: Instant::now(),
            items: items.clone(),
        });
    }
    Ok(items)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn svc(name: &str, enabled: bool) -> Service {
        serde_json::from_value(
            json!({"name": name, "enabled": enabled, "tagline": format!("{name} tagline")}),
        )
        .unwrap()
    }

    fn opt(name: &str, current: Value, secret: bool) -> OptionSchema {
        OptionSchema {
            name: name.into(),
            label: name.into(),
            current: Some(current),
            secret,
            ..Default::default()
        }
    }

    #[test]
    fn parses_tokens() {
        assert_eq!(
            parse_refs("@nextcloud: is @nextcloud.subdomain set? see (@guide:adblock-at-home)."),
            ["nextcloud", "nextcloud.subdomain", "guide:adblock-at-home"]
        );
        assert_eq!(parse_refs("mail me@example.com or @"), Vec::<String>::new());
        assert_eq!(parse_refs("@a @a @b"), ["a", "b"]);
        assert_eq!(parse_refs("@.x @-y"), Vec::<String>::new());
        let many = (0..20)
            .map(|i| format!("@s{i}"))
            .collect::<Vec<_>>()
            .join(" ");
        assert_eq!(parse_refs(&many).len(), MAX_REFS);
    }

    #[test]
    fn targets() {
        assert_eq!(target("pihole"), Some(RefTarget::Service("pihole")));
        assert_eq!(
            target("tinyauth.auth.publicPaths"),
            Some(RefTarget::Option("tinyauth", "auth.publicPaths"))
        );
        assert_eq!(
            target("core.timeZone"),
            Some(RefTarget::Option("core", "timeZone"))
        );
        assert_eq!(target("guide:adblock"), Some(RefTarget::Guide("adblock")));
        assert_eq!(target("privacy:photos"), Some(RefTarget::Privacy("photos")));
        assert_eq!(target("guide:../x"), None);
        assert_eq!(target("other:x"), None);
        assert_eq!(target("core"), None);
        assert_eq!(target("bad name"), None);
    }

    #[test]
    fn strips_the_block() {
        let msg = format!("hi @x{}\n{{}}\n{BLOCK_END}", BLOCK_START);
        assert_eq!(strip_block(&msg), "hi @x");
        assert_eq!(strip_block("plain"), "plain");
        assert!(block(&[]).is_none());
        let b = block(&[json!({"ref": "@x"})]).unwrap();
        assert!(b.starts_with(BLOCK_START) && b.ends_with(BLOCK_END));
    }

    #[test]
    fn secrets_never_reach_the_block() {
        let s = svc("nextcloud", true);
        let mut plumbing = opt("containers.nextcloud", json!({"image": "x"}), false);
        plumbing.ui =
            serde_json::from_value(json!({"group": {"id": "advanced", "label": "Advanced"}})).ok();
        assert!(plumbing.ui.is_some());
        let opts = vec![
            opt("subdomain", json!("cloud"), false),
            opt("dbPassword", json!("hunter2"), true),
            opt("adminPassword", json!(""), true),
            plumbing,
        ];
        let v = service_context(&s, &opts);
        let text = v.to_string();
        assert!(!text.contains("hunter2"));
        assert!(text.contains("<set>") && text.contains("<unset>"));
        assert!(!text.contains("containers.nextcloud"));
        assert_eq!(v["settings"][0]["current"], "cloud");
        let o = option_context("nextcloud", &opts[1]);
        assert_eq!(o["current"], "<set>");
        assert_eq!(o["default"], Value::Null);
        assert_eq!(o["path"], "services.nextcloud.dbPassword");
        assert_eq!(o["link"], "option:nextcloud.dbPassword");
        assert_eq!(
            option_context("core", &opt("timeZone", json!("UTC"), false))["path"],
            "core.timeZone"
        );
    }

    #[test]
    fn long_values_are_cut() {
        let o = opt("list", json!("x".repeat(5000)), false);
        let v = option_context("svc", &o);
        let s = v["current"].as_str().unwrap();
        assert!(s.ends_with("(truncated)") && s.len() < 600);
    }

    #[test]
    fn candidate_order() {
        let services = vec![svc("zeta", true), svc("alpha", false), svc("beta", true)];
        let index = OptionIndex {
            services: [
                (
                    "alpha".to_string(),
                    vec![
                        OptionIndexEntry {
                            path: "enabled".into(),
                            ..Default::default()
                        },
                        OptionIndexEntry {
                            path: "port".into(),
                            label: Some("Port".into()),
                            ..Default::default()
                        },
                    ],
                ),
                (
                    "beta".to_string(),
                    vec![
                        OptionIndexEntry {
                            path: "containers.beta".into(),
                            group: Some("advanced".into()),
                            ..Default::default()
                        },
                        OptionIndexEntry {
                            path: "subdomain".into(),
                            label: Some("Subdomain".into()),
                            ..Default::default()
                        },
                    ],
                ),
            ]
            .into_iter()
            .collect(),
            core: vec![OptionIndexEntry {
                path: "timeZone".into(),
                label: Some("Time zone".into()),
                ..Default::default()
            }],
        };
        let items = candidates(&services, &index, &[], &[]);
        let tokens: Vec<&str> = items.iter().map(|i| i.token.as_str()).collect();
        assert_eq!(
            tokens,
            [
                "beta",
                "zeta",
                "alpha",
                "beta.subdomain",
                "core.timeZone",
                "beta.containers.beta",
                "alpha.port"
            ]
        );
        let sub = &items[3];
        assert_eq!(sub.url, "/configuration/option/beta?focus=subdomain");
        assert_eq!(sub.hint, "beta");
        assert_eq!(items[4].url, "/configuration/core/timeZone?focus=timeZone");
        assert_eq!(items[0].hint, "beta tagline");
    }
}
