//! The chat drawer's side of the local Hermes gateway: chat session ids and
//! contexts here, the replayable history and run state in history.rs, the HTTP
//! client in gateway.rs. The gateway address and key come from the Hermes
//! module: `NEO_HERMES_URL`, `NEO_HERMES_TOKEN_FILE`. Without them the
//! assistant is not available and its buttons are not rendered.

mod gateway;
mod history;

pub use gateway::{available, gateway, Gateway, Refusal};

use crate::utils::command::now_secs;

/// Session id for a chat context: `guide:adblock-at-home` →
/// `neo-guide-adblock-at-home`; `fresh` adds a timestamp for a new chat.
pub fn session_id(context: &str, fresh: bool) -> String {
    let mut out = String::from("neo");
    let mut dash = true;
    for c in context.chars().flat_map(char::to_lowercase) {
        if c.is_ascii_alphanumeric() {
            if dash {
                out.push('-');
            }
            out.push(c);
            dash = false;
        } else {
            dash = true;
        }
    }
    if out == "neo" {
        out.push_str("-general");
    }
    out.truncate(80);
    if fresh {
        out.push_str(&format!("-{}", now_secs()));
    }
    out
}

pub fn valid_session_id(id: &str) -> bool {
    id.starts_with("neo-")
        && id.len() <= 100
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

pub fn valid_run_id(id: &str) -> bool {
    id.starts_with("run_") && id.len() <= 64 && id[4..].chars().all(|c| c.is_ascii_hexdigit())
}

/// Whether `t` is the timestamp suffix of a fresh chat id.
fn is_stamp(t: &str) -> bool {
    t.len() >= 9 && t.chars().all(|c| c.is_ascii_digit())
}

/// The chat context a session id belongs to (best effort, `session_id` is
/// lossy): `neo-service-pi-hole-1730000000` → `service:pi-hole`.
pub fn context_of(id: &str) -> String {
    let mut rest = id.strip_prefix("neo-").unwrap_or(id);
    if let Some((head, tail)) = rest.rsplit_once('-') {
        if is_stamp(tail) {
            rest = head;
        }
    }
    // `security`: the security report's chats (`security:general`, `security:<finding>`).
    for kind in ["service", "guide", "security"] {
        if let Some(name) = rest.strip_prefix(kind).and_then(|r| r.strip_prefix('-')) {
            if !name.is_empty() {
                return format!("{kind}:{name}");
            }
        }
    }
    "general".to_string()
}

/// Whether session `id` is a chat of `context` (its plain id or a fresh one).
pub fn in_context(id: &str, context: &str) -> bool {
    let base = session_id(context, false);
    id == base
        || id
            .strip_prefix(&base)
            .and_then(|r| r.strip_prefix('-'))
            .is_some_and(is_stamp)
}

/// Longest title text taken from the first message (before the context prefix).
const TITLE_CHARS: usize = 60;

/// Chat title from the first message: whitespace collapsed, the reference
/// block dropped, cut at a word near 60 characters, with the context label in
/// front ("Pi-hole: why are ads still shown?").
pub fn chat_title(prefix: Option<&str>, message: &str) -> String {
    let text = super::agent_refs::strip_block(message);
    let mut out = String::new();
    for w in text.split_whitespace() {
        let len = out.chars().count() + usize::from(!out.is_empty()) + w.chars().count();
        if len > TITLE_CHARS {
            if out.is_empty() {
                out = w.chars().take(TITLE_CHARS).collect();
            }
            out.push('…');
            break;
        }
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(w);
    }
    if out.is_empty() {
        out = "New chat".to_string();
    }
    match prefix.map(str::trim).filter(|p| !p.is_empty()) {
        Some(p) => format!("{p}: {out}"),
        None => out,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_ids() {
        assert_eq!(
            session_id("guide:adblock-at-home", false),
            "neo-guide-adblock-at-home"
        );
        assert_eq!(session_id("service:Pi hole", false), "neo-service-pi-hole");
        assert_eq!(session_id("", false), "neo-general");
        assert!(session_id("general", true).starts_with("neo-general-"));
        assert!(valid_session_id("neo-guide-adblock-at-home"));
        assert!(!valid_session_id("neo-../x"));
        assert!(!valid_session_id("other"));
        assert!(valid_run_id("run_0123abcd"));
        assert!(!valid_run_id("run_../"));
    }

    #[test]
    fn contexts_from_ids() {
        assert_eq!(context_of("neo-general"), "general");
        assert_eq!(context_of("neo-general-1730000000"), "general");
        assert_eq!(
            context_of("neo-service-pi-hole-1730000000"),
            "service:pi-hole"
        );
        assert_eq!(
            context_of("neo-security-general-1730000000"),
            "security:general"
        );
        assert_eq!(
            context_of("neo-guide-adblock-at-home"),
            "guide:adblock-at-home"
        );
        assert!(in_context(
            "neo-service-pihole-1730000000",
            "service:pihole"
        ));
        assert!(in_context("neo-service-pihole", "service:pihole"));
        assert!(!in_context(
            "neo-service-pihole-extra-1730000000",
            "service:pihole"
        ));
        assert!(in_context(&session_id("guide:x", true), "guide:x"));
    }

    #[test]
    fn titles_from_first_message() {
        assert_eq!(
            chat_title(Some("Pi-hole"), "  why are\n ads   still shown? "),
            "Pi-hole: why are ads still shown?"
        );
        let long = "word ".repeat(30);
        let t = chat_title(None, &long);
        assert!(t.ends_with('…'));
        assert!(t.chars().count() <= TITLE_CHARS + 1);
        assert_eq!(chat_title(Some(""), ""), "New chat");
        let block = format!(
            "is @pihole on?{}\n{{}}\n</neo-references>",
            super::super::agent_refs::BLOCK_START
        );
        assert_eq!(chat_title(None, &block), "is @pihole on?");
    }
}
