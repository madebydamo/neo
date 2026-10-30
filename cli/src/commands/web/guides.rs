//! Guides (`neo.guides`): extract types, Markdown rendering with neo links and
//! callouts, related guides. See docs/design/descriptions-and-guides.md §3.

use std::collections::HashMap;

use pulldown_cmark::{
    html, BlockQuoteKind, CodeBlockKind, CowStr, Event, HeadingLevel, Options, Parser, Tag, TagEnd,
};
use serde::{Deserialize, Serialize};

/// A service a guide uses, with its state on this machine.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct GuideService {
    pub name: String,
    #[serde(default)]
    pub exists: bool,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default)]
    pub tagline: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Guide {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub difficulty: String,
    #[serde(default)]
    pub minutes: Option<u32>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub rank: Option<i64>,
    #[serde(default)]
    pub privacy: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default)]
    pub services: Vec<GuideService>,
    #[serde(default)]
    pub optional: Vec<GuideService>,
    /// Every service the guide touches (required, optional, linked).
    #[serde(default)]
    pub mentions: Vec<String>,
    #[serde(default, rename = "guideLinks")]
    pub guide_links: Vec<String>,
    #[serde(default, rename = "brokenLinks")]
    pub broken_links: Vec<String>,
    /// Markdown with `{{domain}}` / `{{service.x.url}}` already filled in.
    #[serde(default, skip_serializing)]
    pub body: String,
}

impl Guide {
    /// Required services that are not switched on yet.
    pub fn missing(&self) -> Vec<String> {
        self.services
            .iter()
            .filter(|s| s.exists && !s.enabled)
            .map(|s| s.name.clone())
            .collect()
    }
}

/// Shape returned by `extract_guides.nix`.
#[derive(Deserialize, Default, Clone)]
pub struct GuidesExtract {
    #[serde(default)]
    pub guides: Vec<Guide>,
    #[serde(default, rename = "hermesEnabled")]
    pub hermes_enabled: bool,
    /// False when the operator turned guides off (neo.services.neo.guides).
    #[serde(default = "yes")]
    pub enabled: bool,
}

fn yes() -> bool {
    true
}

/// One H2 of a rendered guide (table of contents).
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct TocEntry {
    pub id: String,
    pub title: String,
}

pub struct RenderCtx<'a> {
    /// `scheme:target` links that do not resolve on this machine.
    pub broken: &'a [String],
    /// Service name → icon URL for service chips.
    pub service_icons: &'a HashMap<String, String>,
    /// Show `neo-check` blocks (only useful when the assistant is on).
    pub assistant: bool,
}

fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

pub fn slug(text: &str) -> String {
    let mut out = String::new();
    let mut dash = false;
    for c in text.chars().flat_map(char::to_lowercase) {
        if c.is_alphanumeric() {
            out.push(c);
            dash = false;
        } else if !dash && !out.is_empty() {
            out.push('-');
            dash = true;
        }
    }
    out.trim_end_matches('-').to_string()
}

/// htmx navigation into `#config-content` (same as service cards).
fn nav_attrs(url: &str) -> String {
    let u = escape(url);
    format!(
        r##"href="{u}" hx-get="{u}" hx-target="#config-content" hx-swap="innerHTML" hx-push-url="true""##
    )
}

/// Web UI URL for an `option:` target (`pihole.localIP`, `core.volumes.appdata`).
fn option_url(target: &str) -> String {
    let mut parts = target.splitn(2, '.');
    let root = parts.next().unwrap_or_default();
    let rest = parts.next().unwrap_or_default();
    if root == "core" {
        let section = rest.split('.').next().unwrap_or_default();
        format!("/configuration/core/{section}?focus={rest}")
    } else {
        format!("/configuration/option/{root}?focus={rest}")
    }
}

/// Opening tag for a link, or `None` to leave a plain link untouched.
fn link_open(dest: &str, ctx: &RenderCtx) -> Option<String> {
    let (scheme, target) = dest.split_once(':')?;
    let neo = matches!(scheme, "service" | "option" | "guide" | "privacy");
    if !neo {
        if scheme == "http" || scheme == "https" {
            let d = escape(dest);
            return Some(format!(
                r#"<a href="{d}" target="_blank" rel="noopener" class="link">"#
            ));
        }
        return None;
    }
    if ctx.broken.iter().any(|b| b == dest) {
        return Some(format!(
            r#"<span class="neo-guide-broken" title="Not available here: {}">"#,
            escape(dest)
        ));
    }
    Some(match scheme {
        "service" => {
            let icon = ctx
                .service_icons
                .get(target)
                .map(|i| format!(r#"<img src="{}" alt="" loading="lazy">"#, escape(i)))
                .unwrap_or_default();
            format!(
                r#"<a {} class="neo-guide-chip" data-guide-service="{}">{icon}"#,
                nav_attrs(&format!("/configuration/option/{target}")),
                escape(target)
            )
        }
        "option" => format!(
            r#"<a {} class="neo-guide-option" data-guide-option="{}">"#,
            nav_attrs(&option_url(target)),
            escape(target)
        ),
        "guide" => format!(
            r#"<a {} class="link link-primary">"#,
            nav_attrs(&format!("/configuration/guides/{target}"))
        ),
        _ => format!(
            r#"<a {} class="link link-primary">"#,
            nav_attrs(&format!("/configuration/privacy#{target}"))
        ),
    })
}

fn link_close(dest: &str, ctx: &RenderCtx) -> &'static str {
    if ctx.broken.iter().any(|b| b == dest) {
        "</span>"
    } else {
        "</a>"
    }
}

fn callout_label(kind: BlockQuoteKind) -> (&'static str, &'static str) {
    match kind {
        BlockQuoteKind::Note => ("note", "Note"),
        BlockQuoteKind::Tip => ("tip", "Tip"),
        // `> [!check]` is rewritten to IMPORTANT before parsing.
        BlockQuoteKind::Important => ("check", "Check"),
        BlockQuoteKind::Warning => ("warning", "Warning"),
        BlockQuoteKind::Caution => ("caution", "Caution"),
    }
}

/// `> [!check] Title` → `> [!IMPORTANT]` + `> **Title**` (GFM has no check kind).
fn rewrite_checks(md: &str) -> String {
    let mut out = String::with_capacity(md.len() + 64);
    for line in md.lines() {
        let trimmed = line.trim_start();
        let lower = trimmed.to_ascii_lowercase();
        if let Some(rest) = lower.strip_prefix(">") {
            let rest_trim = rest.trim_start();
            if rest_trim.starts_with("[!check]") {
                let indent = &line[..line.len() - trimmed.len()];
                // Same offset in the original-case text.
                let start = trimmed.len() - rest_trim.len() + "[!check]".len();
                let title = trimmed[start..].trim();
                out.push_str(indent);
                out.push_str("> [!IMPORTANT]\n");
                if !title.is_empty() {
                    out.push_str(indent);
                    out.push_str("> **");
                    out.push_str(title);
                    out.push_str("**\n");
                    out.push_str(indent);
                    out.push_str(">\n");
                }
                continue;
            }
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

/// Render a guide body to HTML. Raw HTML in the Markdown is shown as text.
pub fn render(md: &str, ctx: &RenderCtx) -> (String, Vec<TocEntry>) {
    let source = rewrite_checks(md);
    let opts = Options::ENABLE_TABLES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_GFM;
    let mut events: Vec<Event> = Parser::new_ext(&source, opts).collect();

    // Heading ids (and the H2 table of contents).
    let mut toc = Vec::new();
    let mut used: HashMap<String, usize> = HashMap::new();
    let mut i = 0;
    while i < events.len() {
        if let Event::Start(Tag::Heading { level, .. }) = &events[i] {
            let level = *level;
            let mut text = String::new();
            let mut j = i + 1;
            while j < events.len() && !matches!(events[j], Event::End(TagEnd::Heading(_))) {
                if let Event::Text(t) | Event::Code(t) = &events[j] {
                    text.push_str(t);
                }
                j += 1;
            }
            let base = slug(&text);
            let n = used.entry(base.clone()).or_insert(0);
            let id = if *n == 0 {
                base.clone()
            } else {
                format!("{base}-{n}")
            };
            *n += 1;
            if level == HeadingLevel::H2 {
                toc.push(TocEntry {
                    id: id.clone(),
                    title: text,
                });
            }
            if let Event::Start(Tag::Heading { id: hid, .. }) = &mut events[i] {
                *hid = Some(CowStr::from(id));
            }
            i = j;
        }
        i += 1;
    }

    let mut out: Vec<Event> = Vec::with_capacity(events.len());
    let mut links: Vec<Option<String>> = Vec::new();
    let mut skip_code = false;
    for ev in events {
        if skip_code {
            if matches!(ev, Event::End(TagEnd::CodeBlock)) {
                skip_code = false;
            }
            continue;
        }
        match ev {
            Event::Html(h) | Event::InlineHtml(h) => out.push(Event::Text(h)),
            Event::Start(Tag::Link { dest_url, .. }) => match link_open(&dest_url, ctx) {
                Some(open) => {
                    out.push(Event::Html(CowStr::from(open)));
                    links.push(Some(dest_url.to_string()));
                }
                None => {
                    links.push(None);
                    out.push(Event::Html(CowStr::from(format!(
                        r#"<a href="{}" class="link">"#,
                        escape(&dest_url)
                    ))));
                }
            },
            Event::End(TagEnd::Link) => {
                let close = match links.pop().flatten() {
                    Some(dest) => link_close(&dest, ctx),
                    None => "</a>",
                };
                out.push(Event::Html(CowStr::from(close)));
            }
            Event::Start(Tag::BlockQuote(Some(kind))) => {
                let (class, label) = callout_label(kind);
                out.push(Event::Html(CowStr::from(format!(
                    r#"<div class="neo-callout neo-callout--{class}" role="note"><div class="neo-callout-label">{label}</div>"#
                ))));
            }
            Event::End(TagEnd::BlockQuote(Some(_))) => {
                out.push(Event::Html(CowStr::from("</div>")));
            }
            Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(lang)))
                if lang.as_ref() == "neo-check" =>
            {
                if ctx.assistant {
                    out.push(Event::Html(CowStr::from(
                        r#"<details class="neo-guide-check-cmd"><summary>How the assistant checks this</summary><pre><code>"#,
                    )));
                    // Text events follow; close at End(CodeBlock) below.
                    links.push(Some("\u{0}neo-check".to_string()));
                } else {
                    skip_code = true;
                }
            }
            Event::End(TagEnd::CodeBlock)
                if links.last().and_then(|l| l.as_deref()) == Some("\u{0}neo-check") =>
            {
                links.pop();
                out.push(Event::Html(CowStr::from("</code></pre></details>")));
            }
            Event::Text(t) if links.last().and_then(|l| l.as_deref()) == Some("\u{0}neo-check") => {
                out.push(Event::Html(CowStr::from(escape(&t))));
            }
            other => out.push(other),
        }
    }

    let mut html_out = String::new();
    html::push_html(&mut html_out, out.into_iter());
    (html_out, toc)
}

/// Guides most related to `guide`: explicit links either way, then shared
/// services and tags. At most `limit`.
pub fn related<'a>(guide: &Guide, all: &'a [Guide], limit: usize) -> Vec<&'a Guide> {
    let mut scored: Vec<(i32, &Guide)> = all
        .iter()
        .filter(|g| g.id != guide.id)
        .map(|g| {
            let mut s = 0;
            if guide.guide_links.contains(&g.id) || g.guide_links.contains(&guide.id) {
                s += 10;
            }
            s += 3 * g
                .mentions
                .iter()
                .filter(|m| guide.mentions.contains(m))
                .count() as i32;
            s += g.tags.iter().filter(|t| guide.tags.contains(t)).count() as i32;
            (s, g)
        })
        .filter(|(s, _)| *s > 0)
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.title.cmp(&b.1.title)));
    scored.into_iter().take(limit).map(|(_, g)| g).collect()
}

/// Guides that touch `service` (for the service pane).
pub fn for_service<'a>(service: &str, all: &'a [Guide]) -> Vec<&'a Guide> {
    all.iter()
        .filter(|g| g.mentions.iter().any(|m| m == service))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx<'a>(broken: &'a [String], icons: &'a HashMap<String, String>) -> RenderCtx<'a> {
        RenderCtx {
            broken,
            service_icons: icons,
            assistant: true,
        }
    }

    #[test]
    fn links_become_neo_navigation() {
        let icons = HashMap::from([("pihole".to_string(), "/service-icons/p.svg".to_string())]);
        let (html, _) = render(
            "[Pi-hole](service:pihole), [IP](option:pihole.localIP), [g](guide:x), [tz](option:core.timeZone)",
            &ctx(&[], &icons),
        );
        assert!(html.contains(r#"hx-get="/configuration/option/pihole""#));
        assert!(html.contains(r#"src="/service-icons/p.svg""#));
        assert!(html.contains(r#"/configuration/option/pihole?focus=localIP"#));
        assert!(html.contains(r#"/configuration/guides/x"#));
        assert!(html.contains(r#"/configuration/core/timeZone?focus=timeZone"#));
    }

    #[test]
    fn broken_links_render_as_text() {
        let broken = vec!["guide:missing".to_string()];
        let (html, _) = render("see [this](guide:missing)", &ctx(&broken, &HashMap::new()));
        assert!(html.contains(r#"<span class="neo-guide-broken""#));
        assert!(!html.contains("/configuration/guides/missing"));
    }

    #[test]
    fn raw_html_is_escaped() {
        let (html, _) = render("hi <script>alert(1)</script>", &ctx(&[], &HashMap::new()));
        assert!(!html.contains("<script>"));
        assert!(html.contains("&lt;script&gt;"));
    }

    #[test]
    fn callouts_and_checks() {
        let md = "> [!tip]\n> Nice.\n\n> [!check] Ads are gone\n> Open a site.\n> ```neo-check\n> dig x\n> ```\n";
        let (html, _) = render(md, &ctx(&[], &HashMap::new()));
        assert!(html.contains("neo-callout--tip"));
        assert!(html.contains("neo-callout--check"));
        assert!(html.contains("<strong>Ads are gone</strong>"));
        assert!(html.contains("How the assistant checks this"));

        let icons = HashMap::new();
        let off = RenderCtx {
            broken: &[],
            service_icons: &icons,
            assistant: false,
        };
        let (html, _) = render(md, &off);
        assert!(!html.contains("dig x"));
    }

    #[test]
    fn headings_get_ids_and_toc() {
        let (html, toc) = render(
            "## 1. Turn on Pi-hole\n\n## Done\n\n## Done\n",
            &ctx(&[], &HashMap::new()),
        );
        assert!(html.contains(r#"<h2 id="1-turn-on-pi-hole">"#));
        assert_eq!(toc.len(), 3);
        assert_eq!(toc[2].id, "done-1");
    }

    #[test]
    fn related_prefers_links_then_services() {
        let g = |id: &str, mentions: &[&str], links: &[&str]| Guide {
            id: id.into(),
            title: id.into(),
            mentions: mentions.iter().map(|s| s.to_string()).collect(),
            guide_links: links.iter().map(|s| s.to_string()).collect(),
            ..Default::default()
        };
        let all = vec![
            g("a", &["pihole"], &["c"]),
            g("b", &["pihole"], &[]),
            g("c", &["tailscale"], &[]),
            g("d", &["immich"], &[]),
        ];
        let r: Vec<&str> = related(&all[0], &all, 3)
            .iter()
            .map(|g| g.id.as_str())
            .collect();
        assert_eq!(r, vec!["c", "b"]);
    }
}
