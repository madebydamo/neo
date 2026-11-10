//! Chat drawer routes (static/agent_chat.js): a proxy to the local Hermes
//! gateway, shared by every page that shows the drawer. Behind tinyauth; the
//! gateway key stays on the server.
//!
//! What differs per server (neo web, the security report) is a [`ChatHost`]:
//! which chat contexts exist, the instructions a chat starts with, the `@`
//! candidates and how a message's references are resolved. The server manages
//! a [`Host`] and mounts [`routes`].

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use rocket::http::{ContentType, Header, Status};
use rocket::request::Request;
use rocket::response::content::RawHtml;
use rocket::response::stream::ByteStream;
use rocket::response::{self, Responder};
use rocket::serde::json::Json;
use rocket::{get, post, State};
use serde::Deserialize;
use serde_json::{json, Value};

use super::agent::{self, valid_run_id, valid_session_id};
use super::agent_refs::RefItem;

pub type BoxFut<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// The page-specific half of the chat.
pub trait ChatHost: Send + Sync {
    /// The chat context for a `data-neo-agent` value (`general`,
    /// `service:<name>`, …), normalized, or None when this page has no such
    /// context.
    fn context(&self, raw: &str) -> Option<String>;
    /// Whether a past chat of this context is listed under Chats here.
    fn owns(&self, context: &str) -> bool;
    /// Display label and icon per context (`service:pihole` → Pi-hole).
    fn labels(&self) -> BoxFut<'_, HashMap<String, (String, Option<String>)>>;
    /// Instructions sent with the first message of a chat of `context`.
    fn instructions<'a>(&'a self, context: &'a str) -> BoxFut<'a, String>;
    /// Candidates for the `@` popup.
    fn refs(&self) -> BoxFut<'_, Result<Arc<Vec<RefItem>>, String>>;
    /// The `<neo-references>` block for a message's `@tokens`, if any resolve.
    fn ref_block<'a>(&'a self, message: &'a str) -> BoxFut<'a, Option<String>>;
    /// Assistant Markdown → HTML.
    fn render<'a>(&'a self, text: &'a str) -> BoxFut<'a, String>;
}

pub type Host = Arc<dyn ChatHost>;

pub fn routes() -> Vec<rocket::Route> {
    rocket::routes![
        agent_open,
        agent_chat,
        agent_approve,
        agent_stop,
        agent_render,
        agent_ref_list,
        agent_sessions,
        agent_session_delete,
        agent_run,
    ]
}

/// Server-sent events with no proxy buffering.
pub struct Sse<R>(R);

impl<'r, 'o: 'r, R: Responder<'r, 'o>> Responder<'r, 'o> for Sse<R> {
    fn respond_to(self, req: &'r Request<'_>) -> response::Result<'o> {
        let mut res = self.0.respond_to(req)?;
        res.set_header(ContentType::EventStream);
        res.set_header(Header::new("Cache-Control", "no-cache"));
        res.set_header(Header::new("X-Accel-Buffering", "no"));
        Ok(res)
    }
}

fn frame(event: &str, data: Value) -> Vec<u8> {
    format!("event: {event}\ndata: {data}\n\n").into_bytes()
}

/// An error for the drawer: a status and `{"error": …}` (plus `ended` on approvals).
pub struct ApiError(Status, Value);

impl ApiError {
    fn new(status: Status, msg: impl Into<String>) -> Self {
        ApiError(status, json!({ "error": msg.into() }))
    }

    fn bad_request(msg: &str) -> Self {
        Self::new(Status::BadRequest, msg)
    }

    fn upstream(msg: impl ToString) -> Self {
        Self::new(Status::BadGateway, msg.to_string())
    }
}

impl<'r> Responder<'r, 'static> for ApiError {
    fn respond_to(self, req: &'r Request<'_>) -> rocket::response::Result<'static> {
        (self.0, Json(self.1)).respond_to(req)
    }
}

type ApiResult = Result<Json<Value>, ApiError>;

fn gateway() -> Result<agent::Gateway, ApiError> {
    agent::gateway().ok_or_else(|| ApiError::new(Status::NotFound, "The assistant is not set up."))
}

/// Labels keyed by base session id, which is what a past chat's lossy
/// context maps back to.
async fn labels_by_session(host: &dyn ChatHost) -> HashMap<String, (String, Option<String>)> {
    host.labels()
        .await
        .into_iter()
        .map(|(ctx, v)| (agent::session_id(&ctx, false), v))
        .collect()
}

#[derive(Deserialize)]
pub struct OpenReq {
    #[serde(default)]
    context: String,
    /// Kept for old clients: every open without `session_id` is a new chat.
    #[serde(default)]
    #[allow(dead_code)]
    fresh: bool,
    /// Reopen this past chat instead of starting a new one.
    #[serde(default)]
    session_id: Option<String>,
}

/// Start a new chat (an id only: Hermes gets the session with the first
/// message, so opening the drawer leaves no empty chats) or reopen a past one
/// with its latest messages.
#[post("/agent/open", data = "<req>")]
pub async fn agent_open(host: &State<Host>, req: Json<OpenReq>) -> ApiResult {
    let gw = gateway()?;
    if let Some(id) = req.session_id.as_deref() {
        if !valid_session_id(id) {
            return Err(ApiError::bad_request("unknown chat"));
        }
        let h = gw.messages(id).await.map_err(ApiError::upstream)?;
        return Ok(Json(json!({
            "session_id": id,
            "context": agent::context_of(id),
            "messages": h.messages,
            "truncated": h.truncated,
        })));
    }
    let ctx = host
        .context(&req.context)
        .ok_or_else(|| ApiError::bad_request("unknown chat context"))?;
    Ok(Json(json!({
        "session_id": agent::session_id(&ctx, true),
        "context": ctx,
        "messages": [],
        "truncated": false,
    })))
}

/// Past chats started on this page's server, newest first, with their
/// context's label and icon. `here` marks the chats of `context` (the page
/// the drawer was opened from).
#[get("/agent/sessions?<context>")]
pub async fn agent_sessions(host: &State<Host>, context: Option<String>) -> ApiResult {
    let gw = gateway()?;
    let list = gw.sessions().await.map_err(ApiError::upstream)?;
    let labels = labels_by_session(host.as_ref()).await;
    let here = context.as_deref().and_then(|c| host.context(c));
    let items: Vec<Value> = list
        .into_iter()
        .filter(|s| host.owns(s["context"].as_str().unwrap_or("general")))
        .map(|mut s| {
            let id = s["id"].as_str().unwrap_or_default().to_string();
            let ctx = s["context"].as_str().unwrap_or("general").to_string();
            let (label, icon) = match labels.get(&agent::session_id(&ctx, false)) {
                Some((l, i)) => (l.clone(), i.clone()),
                None if ctx == "general" => ("General".to_string(), None),
                None => (
                    ctx.split_once(':')
                        .map(|(_, n)| n)
                        .unwrap_or(&ctx)
                        .to_string(),
                    None,
                ),
            };
            s["label"] = Value::from(label);
            s["icon"] = icon.map(Value::from).unwrap_or(Value::Null);
            s["here"] = Value::from(here.as_deref().is_some_and(|c| agent::in_context(&id, c)));
            s
        })
        .collect();
    Ok(Json(json!({ "items": items })))
}

#[rocket::delete("/agent/sessions/<id>")]
pub async fn agent_session_delete(id: &str) -> ApiResult {
    let gw = gateway()?;
    if !valid_session_id(id) {
        return Err(ApiError::bad_request("unknown chat"));
    }
    match gw.delete_session(id).await {
        Ok(()) => Ok(Json(json!({"ok": true}))),
        Err(r) if r.code.as_deref() == Some("session_active_turn") => Err(ApiError::new(
            Status::Conflict,
            "The assistant is still answering in this chat. Stop it first.",
        )),
        Err(r) => Err(ApiError::upstream(format!(
            "Could not delete the chat: {}",
            r.message
        ))),
    }
}

/// State of a running turn (the drawer polls it after a reload or a broken
/// stream): `status`, `done`, pending approval prompts.
#[get("/agent/run/<run_id>")]
pub async fn agent_run(run_id: &str) -> ApiResult {
    let gw = gateway()?;
    if !valid_run_id(run_id) {
        return Err(ApiError::bad_request("unknown run"));
    }
    Ok(Json(
        match gw.run(run_id).await.map_err(ApiError::upstream)? {
            Some(v) => v,
            // Hermes forgets finished runs after a while: it is over either way.
            None => {
                json!({"status": "gone", "done": true, "approvals": [], "approvals_known": true})
            }
        },
    ))
}

#[derive(Deserialize)]
pub struct ChatReq {
    session_id: String,
    message: String,
    #[serde(default)]
    context: String,
    /// First message of the chat: send the context instructions with it.
    #[serde(default)]
    first: bool,
}

/// One turn, streamed: Hermes' session SSE events passed through as-is.
#[post("/agent/chat", data = "<req>")]
pub async fn agent_chat(host: &State<Host>, req: Json<ChatReq>) -> Sse<ByteStream![Vec<u8>]> {
    let req = req.into_inner();
    let host = host.inner().clone();
    let start: Result<reqwest::Response, String> = async {
        let gw = agent::gateway().ok_or("The assistant is not set up.")?;
        if !valid_session_id(&req.session_id) {
            return Err("unknown chat".into());
        }
        let message = req.message.trim();
        if message.is_empty() || message.len() > 20_000 {
            return Err("empty or too long message".into());
        }
        let extra = if req.first {
            let ctx = host
                .context(&req.context)
                .unwrap_or_else(|| "general".to_string());
            // The chat starts with its first message, titled after it.
            let prefix = if ctx == "general" {
                None
            } else {
                labels_by_session(host.as_ref())
                    .await
                    .remove(&agent::session_id(&ctx, false))
                    .map(|(label, _)| label)
                    .or_else(|| ctx.split_once(':').map(|(_, n)| n.to_string()))
            };
            let title = agent::chat_title(prefix.as_deref(), message);
            gw.ensure_session(&req.session_id, Some(&title))
                .await
                .map_err(|e| e.to_string())?;
            Some(host.instructions(&ctx).await)
        } else {
            None
        };
        // `@` references: what each one is, appended for the assistant (and
        // cut off again when the history is shown).
        let message = match host.ref_block(message).await {
            Some(block) => format!("{message}{block}"),
            None => message.to_string(),
        };
        gw.chat_stream(&req.session_id, &message, extra.as_deref())
            .await
            .map_err(|e| e.to_string())
    }
    .await;
    // Hermes interrupts a session turn when its stream reader goes away, so the
    // upstream is read by a task of its own: a closed tab or a reload leaves the
    // turn running (the drawer then polls /agent/run/<id>).
    let mut rx = start.map(|mut resp| {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<Vec<u8>>();
        tokio::spawn(async move {
            loop {
                match resp.chunk().await {
                    Ok(Some(b)) => {
                        // Nobody listening any more: keep draining.
                        let _ = tx.send(b.to_vec());
                    }
                    Ok(None) => break,
                    Err(e) => {
                        let _ = tx.send(frame(
                            "error",
                            json!({"message": format!("connection to the assistant broke: {e}")}),
                        ));
                        let _ = tx.send(frame("done", json!({})));
                        break;
                    }
                }
            }
        });
        rx
    });
    Sse(ByteStream! {
        match rx.as_mut() {
            Ok(rx) => {
                while let Some(b) = rx.recv().await {
                    yield b;
                }
            }
            Err(e) => {
                yield frame("error", json!({"message": e}));
                yield frame("done", json!({}));
            }
        }
    })
}

/// Candidates for the drawer's `@` popup (matched on the client).
#[get("/agent/refs")]
pub async fn agent_ref_list(host: &State<Host>) -> ApiResult {
    gateway()?;
    let items = host
        .refs()
        .await
        .map_err(|e| ApiError::new(Status::InternalServerError, e))?;
    Ok(Json(json!({"items": items.as_ref()})))
}

#[derive(Deserialize)]
pub struct ApproveReq {
    run_id: String,
    choice: String,
    /// The prompt being answered (`approval.request` event `request_id`).
    request_id: Option<String>,
    /// The run generation that raised the prompt; Hermes' session runs refuse answers without it.
    execution_generation: Option<i64>,
}

#[post("/agent/approve", data = "<req>")]
pub async fn agent_approve(req: Json<ApproveReq>) -> ApiResult {
    let gw = gateway()?;
    if !valid_run_id(&req.run_id)
        || !matches!(req.choice.as_str(), "once" | "session" | "always" | "deny")
        || req.request_id.as_deref().is_some_and(|r| {
            r.is_empty()
                || r.len() > 256
                || !r
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || "-_:.".contains(c))
        })
    {
        return Err(ApiError::bad_request("bad approval"));
    }
    gw.approve(
        &req.run_id,
        &req.choice,
        req.request_id.as_deref(),
        req.execution_generation,
    )
    .await
    .map_err(|r| approval_refused(&r))?;
    Ok(Json(json!({"ok": true})))
}

/// A refused approval answer in plain words; `ended` tells the drawer the
/// prompt is over (timed out, answered elsewhere, or its turn ended).
fn approval_refused(r: &agent::Refusal) -> ApiError {
    if r.ended() {
        return ApiError(
            Status::Conflict,
            json!({
                "error": "This request already ended (it timed out or was answered). Ask the assistant to try again.",
                "ended": true,
            }),
        );
    }
    if r.status == 0 {
        return ApiError::upstream(
            "The assistant is not reachable right now. Try again in a moment.",
        );
    }
    ApiError::upstream(format!(
        "The assistant did not take the answer: {}",
        r.message
    ))
}

#[derive(Deserialize)]
pub struct StopReq {
    run_id: String,
}

#[post("/agent/stop", data = "<req>")]
pub async fn agent_stop(req: Json<StopReq>) -> Status {
    match agent::gateway() {
        Some(gw) if valid_run_id(&req.run_id) => match gw.stop(&req.run_id).await {
            Ok(()) => Status::Ok,
            Err(_) => Status::BadGateway,
        },
        Some(_) => Status::BadRequest,
        None => Status::NotFound,
    }
}

#[derive(Deserialize)]
pub struct RenderReq {
    text: String,
}

/// Assistant Markdown → HTML.
#[post("/agent/render", data = "<req>")]
pub async fn agent_render(host: &State<Host>, req: Json<RenderReq>) -> RawHtml<String> {
    RawHtml(host.render(&req.text).await)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn approval_refusals_in_plain_words() {
        let r = |status, code: &str| agent::Refusal {
            status,
            code: Some(code.to_string()),
            message: "raw".to_string(),
        };
        let ApiError(s, j) = approval_refused(&r(409, "approval_not_pending"));
        assert_eq!(s, Status::Conflict);
        assert_eq!(j["ended"], true);
        assert!(j["error"].as_str().unwrap().contains("already ended"));
        let ApiError(_, j) = approval_refused(&r(409, "stale_generation"));
        assert_eq!(j["ended"], true);
        let ApiError(s, j) = approval_refused(&r(400, "invalid_params"));
        assert_eq!(s, Status::BadGateway);
        assert!(j.get("ended").is_none());
    }
}
