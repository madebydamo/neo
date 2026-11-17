//! HTTP client for the local Hermes gateway API (session chat, approvals),
//! used by the chat drawer routes.
use anyhow::{anyhow, Context, Result};
use serde_json::Value;

use super::history::{chronological, history, run_view, session_entry, HISTORY_PAGE};
use crate::commands::web::util::read_token_file;
use crate::utils::command::now_secs;

pub struct Gateway {
    pub base: String,
    token: String,
}

/// The gateway, when Hermes is on and its chat key is readable.
pub fn gateway() -> Option<Gateway> {
    let base = std::env::var("NEO_HERMES_URL")
        .ok()
        .filter(|s| !s.is_empty())?;
    Some(Gateway {
        base: base.trim_end_matches('/').to_string(),
        token: read_token_file("NEO_HERMES_TOKEN_FILE")?,
    })
}

/// Whether the chat drawer and AI buttons should be shown.
pub fn available() -> bool {
    gateway().is_some()
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
            let now = now_secs() as i64;
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
