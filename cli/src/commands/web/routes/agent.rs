//! neo web's half of the chat drawer (the routes are in `web/chat.rs`):
//! contexts `general`, `service:<name>`, `guide:<id>`, instructions with the
//! service's settings or the guide skill, `@` references to apps, settings,
//! guides and privacy categories.
use std::collections::HashMap;
use std::sync::Arc;

use serde_json::json;

use super::agent_tools::describe;
use super::guides::load_guides;
use crate::commands::web::agent_refs::{self, RefItem};
use crate::commands::web::chat::{BoxFut, ChatHost};
use crate::commands::web::guides::{render, RenderCtx};
use crate::commands::web::panes::load_options;
use crate::commands::web::types::AppConfig;
use crate::commands::web::util::service_name_ok;

const STYLE: &str = "You are answering in the chat panel of the neo web UI. The person may not be technical: use plain words and short numbered steps, one step at a time, and keep tool use to what the question needs. Refer to settings by their label. To point at a setting, write a Markdown link like [Home network address](option:pihole.localIP); for an app [Pi-hole](service:pihole); for a guide [title](guide:<id>). Never show a password, token or key in the chat or in commands you run (write *** instead), never ask the user to paste one into the chat, and never make one up yourself.";

/// How settings changes work, depending on whether the neo tools are connected.
fn settings_rules() -> &'static str {
    let tools = std::env::var("NEO_AGENT_TOKEN_FILE")
        .ok()
        .is_some_and(|p| std::path::Path::new(&p).exists());
    if tools {
        "Read settings with neo_settings_schema / neo_settings_get and change them only with neo_settings_propose, never by editing files. For a secret setting use the op `copy` or `move` when another setting already has the value (whole settings, entries or fields, with `from`; a rename is a move, and you never see the value), `ask` (the user types or generates it on the card; changes with the same `group` share one input) or `generate`, never a value of your own. Never decrypt or edit the secrets files, use the machine's keys or read secrets out of containers; if the tools cannot do something, say so plainly instead of trying a workaround. After proposing, say that a card with Save / Save & apply appears here in the chat, include the proposal link from the tool result, and wait for the user."
    } else {
        "The neo settings tools are not connected here, so do not look for them (no tool_search or connection setup for them). Read the current settings from the context below or with read-only commands, and never edit settings files yourself: tell the user which setting to change, as an option: link they can click, and that they save it and press Apply at the top. To let you prepare changes yourself, the user can generate the Settings tools token on the Hermes page."
    }
}

/// `(kind, name)` of a chat context: `general`, `service:<name>`, `guide:<id>`.
fn parse_context(ctx: &str) -> Option<(&str, &str)> {
    match ctx.split_once(':') {
        Some((k @ ("service" | "guide"), n)) if service_name_ok(n) => Some((k, n)),
        None if ctx.is_empty() || ctx == "general" => Some(("general", "")),
        _ => None,
    }
}

/// Display label and icon of each chat context, keyed by context
/// (`service:pihole`): apps and guides from the `@` candidates.
async fn context_labels(config: &AppConfig) -> HashMap<String, (String, Option<String>)> {
    let mut out = HashMap::new();
    if let Ok(items) = agent_refs::cached_candidates(config).await {
        for it in items.iter() {
            let ctx = match it.kind {
                "service" => format!("service:{}", it.token),
                "guide" => it.token.clone(),
                _ => continue,
            };
            out.insert(ctx, (it.label.clone(), it.icon.clone()));
        }
    }
    out
}

/// Instructions for the first turn of a context (settings snapshot for a
/// service, the guide skill for a guide).
async fn instructions(config: &AppConfig, kind: &str, name: &str) -> String {
    match kind {
        "service" => {
            let settings = match load_options(config, false, name).await {
                Ok(opts) => json!(opts
                    .iter()
                    .map(|o| describe(&format!("services.{name}"), o))
                    .collect::<Vec<_>>())
                .to_string(),
                Err(_) => "[]".to_string(),
            };
            format!("{STYLE} {}\n\nThe user opened the chat from the {name} settings page. If a neo-{name} skill exists, load it. Current settings (secrets redacted): {settings}", settings_rules())
        }
        "guide" => {
            let title = load_guides(config)
                .await
                .ok()
                .and_then(|x| x.guides.into_iter().find(|g| g.id == name))
                .map(|g| g.title)
                .unwrap_or_else(|| name.to_string());
            format!("{STYLE} {}\n\nThe user wants to follow the guide \"{title}\". Load the neo-guide-{name} skill and walk them through it step by step: first check what is already done, ask only what you cannot find out yourself, and run each check before moving on.", settings_rules())
        }
        _ => format!("{STYLE} {}", settings_rules()),
    }
}

/// Assistant Markdown → HTML with neo links (same renderer as guides).
async fn render_md(config: &AppConfig, text: &str) -> String {
    let icons: HashMap<String, String> = load_guides(config)
        .await
        .map(|x| {
            x.guides
                .iter()
                .flat_map(|g| g.services.iter().chain(g.optional.iter()))
                .filter_map(|s| s.icon.clone().map(|i| (s.name.clone(), i)))
                .collect()
        })
        .unwrap_or_default();
    let (html, _) = render(
        text,
        &RenderCtx {
            broken: &[],
            service_icons: &icons,
            assistant: false,
        },
    );
    html
}

/// The chat host of neo web.
pub struct NeoHost(pub Arc<AppConfig>);

impl ChatHost for NeoHost {
    fn context(&self, raw: &str) -> Option<String> {
        parse_context(raw).map(|(kind, name)| match kind {
            "general" => "general".to_string(),
            _ => format!("{kind}:{name}"),
        })
    }

    fn owns(&self, context: &str) -> bool {
        parse_context(context).is_some()
    }

    fn labels(&self) -> BoxFut<'_, HashMap<String, (String, Option<String>)>> {
        Box::pin(context_labels(&self.0))
    }

    fn instructions<'a>(&'a self, context: &'a str) -> BoxFut<'a, String> {
        Box::pin(async move {
            let (kind, name) = parse_context(context).unwrap_or(("general", ""));
            instructions(&self.0, kind, name).await
        })
    }

    fn refs(&self) -> BoxFut<'_, Result<Arc<Vec<RefItem>>, String>> {
        Box::pin(agent_refs::cached_candidates(&self.0))
    }

    fn ref_block<'a>(&'a self, message: &'a str) -> BoxFut<'a, Option<String>> {
        Box::pin(agent_refs::context_for(&self.0, message))
    }

    fn render<'a>(&'a self, text: &'a str) -> BoxFut<'a, String> {
        Box::pin(render_md(&self.0, text))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contexts() {
        assert_eq!(parse_context("general"), Some(("general", "")));
        assert_eq!(parse_context(""), Some(("general", "")));
        assert_eq!(parse_context("service:pihole"), Some(("service", "pihole")));
        assert_eq!(
            parse_context("guide:adblock-at-home"),
            Some(("guide", "adblock-at-home"))
        );
        assert_eq!(parse_context("guide:../x"), None);
        assert_eq!(parse_context("other:x"), None);
    }
}
