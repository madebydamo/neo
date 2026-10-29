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

    /// Visible messages of a session (user and assistant text).
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
        Ok(v["data"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter(|m| {
                matches!(m["role"].as_str(), Some("user" | "assistant"))
                    && m["display_kind"].as_str() != Some("hidden")
                    && m["content"].as_str().is_some_and(|c| !c.trim().is_empty())
            })
            .map(|m| serde_json::json!({"role": m["role"], "content": m["content"]}))
            .collect())
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

    /// Answer an approval prompt (`once` / `session` / `always` / `deny`).
    pub async fn approve(&self, run_id: &str, choice: &str) -> Result<()> {
        self.post(&format!("/v1/runs/{run_id}/approval"))
            .json(&serde_json::json!({"choice": choice}))
            .send()
            .await
            .context("assistant not reachable")?
            .error_for_status()?;
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
}
