//! Chat drawer: proxy to the local Hermes gateway. Behind tinyauth like every
//! neo web route; the gateway key stays on the server.
use std::collections::HashMap;
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

use super::agent_tools::describe;
use super::guides::load_guides;
use crate::commands::web::agent::{self, valid_run_id, valid_session_id};
use crate::commands::web::agent_refs;
use crate::commands::web::guides::{render, RenderCtx};
use crate::commands::web::panes::load_options;
use crate::commands::web::types::AppConfig;
use crate::commands::web::util::service_name_ok;

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

/// `(kind, name)` of a chat context: `general`, `service:<name>`, `guide:<id>`.
fn parse_context(ctx: &str) -> Option<(&str, &str)> {
    match ctx.split_once(':') {
        Some((k @ ("service" | "guide"), n)) if service_name_ok(n) => Some((k, n)),
        None if ctx.is_empty() || ctx == "general" => Some(("general", "")),
        _ => None,
    }
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

/// Display label and icon of each chat context, keyed by its base session id
/// (`neo-service-pihole`): apps and guides from the `@` candidates.
async fn context_labels(config: &AppConfig) -> HashMap<String, (String, Option<String>)> {
    let mut out = HashMap::new();
    if let Ok(items) = agent_refs::cached_candidates(config).await {
        for it in items.iter() {
            let ctx = match it.kind {
                "service" => format!("service:{}", it.token),
                "guide" => it.token.clone(),
                _ => continue,
            };
            out.insert(
                agent::session_id(&ctx, false),
                (it.label.clone(), it.icon.clone()),
            );
        }
    }
    out
}

/// Start a new chat (an id only: Hermes gets the session with the first
/// message, so opening the drawer leaves no empty chats) or reopen a past one
/// with its latest messages.
#[post("/agent/open", data = "<req>")]
pub async fn agent_open(req: Json<OpenReq>) -> ApiResult {
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
    let (kind, name) =
        parse_context(&req.context).ok_or_else(|| ApiError::bad_request("unknown chat context"))?;
    let ctx = if kind == "general" {
        "general".to_string()
    } else {
        format!("{kind}:{name}")
    };
    Ok(Json(json!({
        "session_id": agent::session_id(&ctx, true),
        "context": ctx,
        "messages": [],
        "truncated": false,
    })))
}

/// Past chats started here, newest first, with their context's label and icon.
/// `here` marks the chats of `context` (the page the drawer was opened from).
#[get("/agent/sessions?<context>")]
pub async fn agent_sessions(config: &State<Arc<AppConfig>>, context: Option<String>) -> ApiResult {
    let gw = gateway()?;
    let list = gw.sessions().await.map_err(ApiError::upstream)?;
    let labels = context_labels(config).await;
    let here = context.as_deref().filter(|c| parse_context(c).is_some());
    let items: Vec<Value> = list
        .into_iter()
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
            s["here"] = Value::from(here.is_some_and(|c| agent::in_context(&id, c)));
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
pub async fn agent_chat(
    config: &State<Arc<AppConfig>>,
    req: Json<ChatReq>,
) -> Sse<ByteStream![Vec<u8>]> {
    let req = req.into_inner();
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
            let (kind, name) = parse_context(&req.context).unwrap_or(("general", ""));
            // The chat starts with its first message, titled after it.
            let prefix = match kind {
                "general" => None,
                _ => context_labels(config)
                    .await
                    .remove(&agent::session_id(&format!("{kind}:{name}"), false))
                    .map(|(label, _)| label)
                    .or_else(|| Some(name.to_string())),
            };
            let title = agent::chat_title(prefix.as_deref(), message);
            gw.ensure_session(&req.session_id, Some(&title))
                .await
                .map_err(|e| e.to_string())?;
            Some(instructions(config, kind, name).await)
        } else {
            None
        };
        // `@` references: what each one is, appended for the assistant (and
        // cut off again when the history is shown).
        let message = match agent_refs::context_for(config, message).await {
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
pub async fn agent_ref_list(config: &State<Arc<AppConfig>>) -> ApiResult {
    gateway()?;
    let items = agent_refs::cached_candidates(config)
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

/// Assistant Markdown → HTML with neo links (same renderer as guides).
#[post("/agent/render", data = "<req>")]
pub async fn agent_render(config: &State<Arc<AppConfig>>, req: Json<RenderReq>) -> RawHtml<String> {
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
        &req.text,
        &RenderCtx {
            broken: &[],
            service_icons: &icons,
            assistant: false,
        },
    );
    RawHtml(html)
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
