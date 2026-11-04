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

impl Gateway {
    fn client() -> reqwest::Client {
        reqwest::Client::new()
    }

    fn post(&self, path: &str) -> reqwest::RequestBuilder {
        Self::client()
            .post(format!("{}{path}", self.base))
            .bearer_auth(&self.token)
    }

    /// Create the session if it does not exist yet (a conflict means it does).
    pub async fn ensure_session(&self, id: &str, title: &str) -> Result<()> {
        let r = self
            .post("/api/sessions")
            .json(&serde_json::json!({"id": id, "title": title}))
            .send()
            .await
            .context("assistant not reachable")?;
        let s = r.status();
        if s.is_success() || s == reqwest::StatusCode::CONFLICT {
            return Ok(());
        }
        Err(anyhow!("assistant refused the chat ({s})"))
    }

    /// Visible messages of a session (see [`history`]).
    pub async fn messages(&self, id: &str) -> Result<Vec<Value>> {
        let r = Self::client()
            .get(format!(
                "{}/api/sessions/{id}/messages?limit=200",
                self.base
            ))
            .bearer_auth(&self.token)
            .send()
            .await
            .context("assistant not reachable")?;
        if r.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(vec![]);
        }
        let v: Value = r.error_for_status()?.json().await?;
        Ok(history(
            v["data"].as_array().map(Vec::as_slice).unwrap_or_default(),
        ))
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
    ) -> Result<()> {
        let mut body = serde_json::json!({"choice": choice});
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
            .context("assistant not reachable")?;
        if !r.status().is_success() {
            let s = r.status();
            let text = r.text().await.unwrap_or_default();
            return Err(anyhow!(
                "assistant refused the answer ({s}): {}",
                text.chars().take(300).collect::<String>()
            ));
        }
        Ok(())
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
}
