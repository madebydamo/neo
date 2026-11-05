//! Client for the local Hermes gateway API (session chat, approvals), used by
//! the chat drawer routes (routes/agent.rs). The gateway address and key come
//! from the Hermes module: `NEO_HERMES_URL`, `NEO_HERMES_TOKEN_FILE`. Without
//! them the assistant is not available and its buttons are not rendered.

use anyhow::{anyhow, Context, Result};
use serde_json::Value;

pub struct Gateway {
    pub base: String,
    token: String,
}

/// The gateway, when Hermes is on and its chat key is readable.
pub fn gateway() -> Option<Gateway> {
    let base = std::env::var("NEO_HERMES_URL")
        .ok()
        .filter(|s| !s.is_empty())?;
    let path = std::env::var("NEO_HERMES_TOKEN_FILE").ok()?;
    let token = std::fs::read_to_string(path).ok()?.trim().to_string();
    (!token.is_empty()).then_some(Gateway {
        base: base.trim_end_matches('/').to_string(),
        token,
    })
}

/// Whether the chat drawer and AI buttons should be shown.
pub fn available() -> bool {
    gateway().is_some()
}

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
        let t = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or_default();
        out.push_str(&format!("-{t}"));
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

/// Longest tool argument / result text kept in the replayed history.
const MAX_TOOL_TEXT: usize = 4000;

fn clip(s: &str) -> String {
    if s.chars().count() <= MAX_TOOL_TEXT {
        return s.to_string();
    }
    let cut: String = s.chars().take(MAX_TOOL_TEXT).collect();
    format!("{cut}\n… (cut, {} characters in all)", s.chars().count())
}

/// Text of a message `content` (a string, or OpenAI-style parts).
fn content_text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Array(parts) => parts
            .iter()
            .filter_map(|p| p["text"].as_str().or(p.as_str()))
            .collect::<Vec<_>>()
            .join("\n"),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

/// One `tool_calls` entry → `{id, name, args}` (args parsed when JSON).
fn tool_call(tc: &Value) -> Value {
    let f = if tc["function"].is_object() {
        &tc["function"]
    } else {
        tc
    };
    let raw = &f["arguments"];
    let args = match raw {
        Value::String(s) => {
            serde_json::from_str::<Value>(s).unwrap_or_else(|_| Value::String(s.clone()))
        }
        v => v.clone(),
    };
    let text = args.to_string();
    let args = if text.chars().count() > MAX_TOOL_TEXT {
        Value::String(clip(&text))
    } else {
        args
    };
    serde_json::json!({
        "id": tc["id"].as_str().or(tc["call_id"].as_str()).unwrap_or_default(),
        "name": f["name"].as_str().unwrap_or_default(),
        "args": args,
    })
}

/// A session's messages as the drawer replays them: user text (without the
/// `@` reference block), assistant text, and `{"role": "tools", "calls":
/// [{name, args, result}]}` for each run of tool calls between texts.
pub fn history(raw: &[Value]) -> Vec<Value> {
    let mut out: Vec<Value> = Vec::new();
    for m in raw {
        if m["display_kind"].as_str() == Some("hidden") {
            continue;
        }
        let text = content_text(&m["content"]);
        match m["role"].as_str() {
            Some("user") => {
                let t = super::agent_refs::strip_block(&text).trim();
                if !t.is_empty() {
                    out.push(serde_json::json!({"role": "user", "content": t}));
                }
            }
            Some("assistant") => {
                if !text.trim().is_empty() {
                    out.push(serde_json::json!({"role": "assistant", "content": text}));
                }
                let calls: Vec<Value> = m["tool_calls"]
                    .as_array()
                    .map(|a| a.iter().map(tool_call).collect())
                    .unwrap_or_default();
                if calls.is_empty() {
                    continue;
                }
                match out.last_mut() {
                    Some(last) if last["role"] == "tools" => {
                        if let Some(a) = last["calls"].as_array_mut() {
                            a.extend(calls);
                        }
                    }
                    _ => out.push(serde_json::json!({"role": "tools", "calls": calls})),
                }
            }
            Some("tool") => {
                let id = m["tool_call_id"].as_str().unwrap_or_default();
                let call = out
                    .iter_mut()
                    .rev()
                    .filter(|e| e["role"] == "tools")
                    .flat_map(|e| e["calls"].as_array_mut().into_iter().flatten())
                    .find(|c| !id.is_empty() && c["id"] == id && c.get("result").is_none());
                if let Some(c) = call {
                    c["result"] = Value::String(clip(&text));
                }
            }
            _ => {}
        }
    }
    out
}

/// Most messages one history read returns (Hermes' page limit).
const HISTORY_PAGE: usize = 500;

/// Raw messages in conversation order: by Hermes' row id when every message has
/// one (a page read from the newest end must still replay oldest first).
fn chronological(mut raw: Vec<Value>) -> Vec<Value> {
    if raw.iter().all(|m| m["id"].as_i64().is_some()) {
        raw.sort_by_key(|m| m["id"].as_i64().unwrap_or_default());
    }
    raw
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
    for kind in ["service", "guide"] {
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

/// A run's state for the drawer: `status`, whether it ended (`done`), and the
/// approval prompts still waiting, in the `approval.request` event shape
/// (`request_id`, `execution_generation`, `command`, `description`, `choices`,
/// `timeout_s`). `approvals_known`: the list is complete, so a prompt missing
/// from it has ended.
pub fn run_view(v: &Value) -> Value {
    let status = v["status"].as_str().unwrap_or("running").to_string();
    let done = matches!(
        status.as_str(),
        "completed" | "failed" | "cancelled" | "interrupted"
    );
    let pick = |p: &Value, id: &str| {
        let mut a = serde_json::json!({ "request_id": id });
        for k in [
            "execution_generation",
            "command",
            "description",
            "choices",
            "timeout_s",
        ] {
            if !p[k].is_null() {
                a[k] = p[k].clone();
            }
        }
        a
    };
    let mut approvals: Vec<Value> = Vec::new();
    let known = if let Some(list) = v["pending_controls"].as_array() {
        for p in list.iter().filter(|p| p["kind"] == "approval") {
            if let Some(id) = p["prompt_id"].as_str().or(p["request_id"].as_str()) {
                approvals.push(pick(p, id));
            }
        }
        true
    } else if status == "waiting_for_approval" {
        match v["approval"]["request_id"].as_str() {
            Some(id) => {
                approvals.push(pick(&v["approval"], id));
                true
            }
            None => false,
        }
    } else {
        // A session-stream run drops its prompt from the status once answered.
        true
    };
    let mut out = serde_json::json!({
        "status": status,
        "done": done,
        "approvals": approvals,
        "approvals_known": known,
    });
    if let Some(e) = v["error"].as_str().filter(|e| !e.is_empty()) {
        out["error"] = Value::from(e);
    }
    out
}

/// One past chat for the drawer's list (`None` for other clients' sessions
/// and empty ones).
fn session_entry(s: &Value) -> Option<Value> {
    let id = s["id"].as_str().filter(|id| valid_session_id(id))?;
    let count = s["message_count"].as_i64().unwrap_or_default();
    if count <= 0 || s["archived"].as_bool() == Some(true) {
        return None;
    }
    let last = s["last_active"]
        .as_f64()
        .or(s["ended_at"].as_f64())
        .or(s["started_at"].as_f64())
        .unwrap_or_default();
    Some(serde_json::json!({
        "id": id,
        "title": s["title"].as_str().unwrap_or_default(),
        "preview": s["preview"].as_str().unwrap_or_default(),
        "context": context_of(id),
        "last_active": last,
        "message_count": count,
    }))
}

/// Why Hermes refused a request: HTTP status (0 = not reachable), its error
/// `code`, a short text.
#[derive(Debug)]
pub struct Refusal {
    pub status: u16,
    pub code: Option<String>,
    pub message: String,
}

impl Refusal {
    async fn from(r: reqwest::Response) -> Self {
        let status = r.status().as_u16();
        let text = r.text().await.unwrap_or_default();
        let v: Value = serde_json::from_str(&text).unwrap_or_default();
        let code = v["error"]["code"]
            .as_str()
            .or(v["code"].as_str())
            .map(str::to_string);
        let message = v["error"]["message"]
            .as_str()
            .map(str::to_string)
            .unwrap_or_else(|| text.chars().take(300).collect());
        Refusal {
            status,
            code,
            message,
        }
    }

    fn unreachable(e: reqwest::Error) -> Self {
        Refusal {
            status: 0,
            code: None,
            message: format!("assistant not reachable: {e}"),
        }
    }

    /// The prompt or its run is over (timed out, answered, or the turn ended).
    pub fn ended(&self) -> bool {
        self.status == 404
            || matches!(
                self.code.as_deref(),
                Some(
                    "approval_not_pending"
                        | "stale_generation"
                        | "approval_not_active"
                        | "not_found"
                )
            )
    }
}

/// A session's replayable history; `truncated` when older messages exist
/// beyond the page that was read.
pub struct History {
    pub messages: Vec<Value>,
    pub truncated: bool,
}

impl Gateway {
    fn client() -> reqwest::Client {
        reqwest::Client::new()
    }

    fn post(&self, path: &str) -> reqwest::RequestBuilder {
        Self::client()
            .post(format!("{}{path}", self.base))
            .bearer_auth(&self.token)
    }

    fn get(&self, path: &str) -> reqwest::RequestBuilder {
        Self::client()
            .get(format!("{}{path}", self.base))
            .bearer_auth(&self.token)
    }

    /// Create the session if it does not exist yet (a conflict means it does).
    /// Titles are unique in Hermes: on a clash retry with the time, then the
    /// id's tail, then without a title.
    pub async fn ensure_session(&self, id: &str, title: Option<&str>) -> Result<()> {
        let mut titles: Vec<Option<String>> = Vec::new();
        if let Some(t) = title.map(str::trim).filter(|t| !t.is_empty()) {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or_default();
            let tail = &id[id.len().saturating_sub(4)..];
            titles.push(Some(t.to_string()));
            titles.push(Some(format!(
                "{t} · {}",
                crate::utils::locks::local_hhmm(now)
            )));
            titles.push(Some(format!("{t} · {tail}")));
        }
        titles.push(None);
        for t in titles {
            let mut body = serde_json::json!({ "id": id });
            if let Some(t) = &t {
                body["title"] = Value::from(t.as_str());
            }
            let r = self
                .post("/api/sessions")
                .json(&body)
                .send()
                .await
                .context("assistant not reachable")?;
            let s = r.status();
            if s.is_success() || s == reqwest::StatusCode::CONFLICT {
                return Ok(());
            }
            let refusal = Refusal::from(r).await;
            if refusal.code.as_deref() != Some("invalid_title") {
                return Err(anyhow!(
                    "assistant refused the chat ({s}): {}",
                    refusal.message
                ));
            }
        }
        Err(anyhow!("assistant refused the chat title"))
    }

    /// The newest page of a session's visible messages (see [`history`]), in
    /// conversation order. With an explicit limit Hermes returns the oldest
    /// page unless `order=latest` is asked for.
    pub async fn messages(&self, id: &str) -> Result<History> {
        let r = self
            .get(&format!(
                "/api/sessions/{id}/messages?order=latest&limit={HISTORY_PAGE}"
            ))
            .send()
            .await
            .context("assistant not reachable")?;
        if r.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(History {
                messages: vec![],
                truncated: false,
            });
        }
        let v: Value = r.error_for_status()?.json().await?;
        let raw = v["data"].as_array().cloned().unwrap_or_default();
        let truncated = raw.len() >= HISTORY_PAGE;
        Ok(History {
            messages: history(&chronological(raw)),
            truncated,
        })
    }

    /// The chats neo-web started (`neo-…` ids with messages), newest first.
    pub async fn sessions(&self) -> Result<Vec<Value>> {
        let v: Value = self
            .get("/api/sessions?limit=200")
            .send()
            .await
            .context("assistant not reachable")?
            .error_for_status()?
            .json()
            .await?;
        let mut out: Vec<Value> = v["data"]
            .as_array()
            .map(|a| a.iter().filter_map(session_entry).collect())
            .unwrap_or_default();
        out.sort_by(|a, b| {
            b["last_active"]
                .as_f64()
                .partial_cmp(&a["last_active"].as_f64())
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        Ok(out)
    }

    pub async fn delete_session(&self, id: &str) -> Result<(), Refusal> {
        let r = Self::client()
            .delete(format!("{}/api/sessions/{id}", self.base))
            .bearer_auth(&self.token)
            .send()
            .await
            .map_err(Refusal::unreachable)?;
        if r.status().is_success() || r.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(());
        }
        Err(Refusal::from(r).await)
    }

    /// Pollable state of a run ([`run_view`]); `None` once Hermes forgot it.
    pub async fn run(&self, run_id: &str) -> Result<Option<Value>> {
        let r = self
            .get(&format!("/v1/runs/{run_id}"))
            .send()
            .await
            .context("assistant not reachable")?;
        if r.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        let v: Value = r.error_for_status()?.json().await?;
        Ok(Some(run_view(&v)))
    }

    /// Start one streamed turn; the caller forwards the SSE body.
    pub async fn chat_stream(
        &self,
        id: &str,
        message: &str,
        instructions: Option<&str>,
    ) -> Result<reqwest::Response> {
        let mut body = serde_json::json!({"message": message});
        if let Some(i) = instructions {
            body["system_message"] = Value::String(i.to_string());
        }
        let r = self
            .post(&format!("/api/sessions/{id}/chat/stream"))
            .header("Accept", "text/event-stream")
            .json(&body)
            .send()
            .await
            .context("assistant not reachable")?;
        if !r.status().is_success() {
            let s = r.status();
            let text = r.text().await.unwrap_or_default();
            return Err(anyhow!(
                "assistant refused the message ({s}): {}",
                text.chars().take(300).collect::<String>()
            ));
        }
        Ok(r)
    }

    /// Answer an approval prompt (`once` / `session` / `always` / `deny`). Session
    /// chat runs need the exact prompt (`request_id`) and its `execution_generation`
    /// from the `approval.request` event; Hermes refuses any other field set there.
    pub async fn approve(
        &self,
        run_id: &str,
        choice: &str,
        request_id: Option<&str>,
        generation: Option<i64>,
    ) -> Result<(), Refusal> {
        let mut body = serde_json::json!({ "choice": choice });
        if let Some(id) = request_id {
            body["request_id"] = Value::from(id);
        }
        if let Some(g) = generation {
            body["execution_generation"] = Value::from(g);
        }
        let r = self
            .post(&format!("/v1/runs/{run_id}/approval"))
            .json(&body)
            .send()
            .await
            .map_err(Refusal::unreachable)?;
        if r.status().is_success() {
            return Ok(());
        }
        Err(Refusal::from(r).await)
    }

    pub async fn stop(&self, run_id: &str) -> Result<()> {
        self.post(&format!("/v1/runs/{run_id}/stop"))
            .send()
            .await
            .context("assistant not reachable")?;
        Ok(())
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
    fn history_keeps_text_and_tool_calls() {
        use serde_json::json;
        let raw = vec![
            json!({"role": "user", "content": format!("is @pihole on?{}\n{{}}\n</neo-references>", super::super::agent_refs::BLOCK_START)}),
            json!({"role": "assistant", "content": "Let me check.", "tool_calls": [
                {"id": "c1", "type": "function", "function": {"name": "mcp_neo_neo_settings_get", "arguments": "{\"paths\":[\"services.pihole.enabled\"]}"}}
            ]}),
            json!({"role": "tool", "tool_call_id": "c1", "content": "[{\"current\":true}]"}),
            json!({"role": "assistant", "content": "", "tool_calls": [
                {"id": "c2", "function": {"name": "terminal", "arguments": "not json"}}
            ]}),
            json!({"role": "tool", "tool_call_id": "c2", "content": "x".repeat(5000)}),
            json!({"role": "assistant", "content": "", "display_kind": "hidden"}),
            json!({"role": "assistant", "content": "Yes, it is on."}),
            json!({"role": "system", "content": "ignored"}),
        ];
        let h = history(&raw);
        assert_eq!(h.len(), 4);
        assert_eq!(h[0], json!({"role": "user", "content": "is @pihole on?"}));
        assert_eq!(h[1]["content"], "Let me check.");
        let calls = h[2]["calls"].as_array().unwrap();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0]["name"], "mcp_neo_neo_settings_get");
        assert_eq!(calls[0]["args"]["paths"][0], "services.pihole.enabled");
        assert_eq!(calls[0]["result"], "[{\"current\":true}]");
        assert_eq!(calls[1]["args"], "not json");
        assert!(calls[1]["result"]
            .as_str()
            .unwrap()
            .contains("5000 characters"));
        assert_eq!(h[3]["content"], "Yes, it is on.");
    }

    #[test]
    fn latest_page_replays_oldest_first() {
        use serde_json::json;
        // Newest-first rows (as an `order=latest` page could come back) replay in order.
        let raw = vec![
            json!({"id": 12, "role": "assistant", "content": "third"}),
            json!({"id": 11, "role": "user", "content": "second"}),
            json!({"id": 10, "role": "assistant", "content": "first"}),
        ];
        let h = history(&chronological(raw));
        let texts: Vec<_> = h.iter().map(|m| m["content"].as_str().unwrap()).collect();
        assert_eq!(texts, ["first", "second", "third"]);
        // Without ids the order is kept as given.
        let raw = vec![
            json!({"role": "user", "content": "a"}),
            json!({"role": "assistant", "content": "b"}),
        ];
        assert_eq!(history(&chronological(raw))[1]["content"], "b");
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

    #[test]
    fn run_views() {
        use serde_json::json;
        let v = run_view(&json!({
            "status": "waiting_for_approval",
            "pending_controls": [
                {"kind": "approval", "prompt_id": "p1", "execution_generation": 3,
                 "command": "rm -rf /tmp/x", "description": "delete", "choices": ["once", "deny"], "timeout_s": 300},
                {"kind": "clarify", "prompt_id": "c1"}
            ]
        }));
        assert_eq!(v["done"], false);
        assert_eq!(v["approvals_known"], true);
        let a = &v["approvals"][0];
        assert_eq!(a["request_id"], "p1");
        assert_eq!(a["execution_generation"], 3);
        assert_eq!(a["timeout_s"], 300);
        assert_eq!(v["approvals"].as_array().unwrap().len(), 1);

        let v = run_view(&json!({"status": "waiting_for_approval",
            "approval": {"request_id": "r9", "command": "ls", "choices": ["once"]}}));
        assert_eq!(v["approvals"][0]["request_id"], "r9");

        let v = run_view(&json!({"status": "failed", "error": "boom"}));
        assert_eq!(v["done"], true);
        assert_eq!(v["error"], "boom");
        assert_eq!(v["approvals"], json!([]));
    }

    #[test]
    fn session_entries_keep_neo_chats() {
        use serde_json::json;
        assert!(session_entry(&json!({"id": "api_1", "message_count": 3})).is_none());
        assert!(session_entry(&json!({"id": "neo-general", "message_count": 0})).is_none());
        let e = session_entry(
            &json!({"id": "neo-service-pihole-1730000000", "title": "Pi-hole: x",
            "message_count": 4, "last_active": 1730000100.5}),
        )
        .unwrap();
        assert_eq!(e["context"], "service:pihole");
        assert_eq!(e["last_active"], 1730000100.5);
    }

    #[test]
    fn ended_refusals() {
        let r = |status, code: Option<&str>| Refusal {
            status,
            code: code.map(str::to_string),
            message: String::new(),
        };
        assert!(r(409, Some("approval_not_pending")).ended());
        assert!(r(409, Some("stale_generation")).ended());
        assert!(!r(400, Some("invalid_approval_choice")).ended());
        assert!(!r(0, None).ended());
    }
}
