//! Settings tools for the assistant (MCP over streamable HTTP, JSON responses).
//!
//! `POST /internal/mcp` with `Authorization: Bearer <token>`; the token is read
//! from `$NEO_AGENT_TOKEN_FILE` (rendered by the Hermes module). Without a
//! token file the endpoint does not exist (404). SWAG never forwards
//! `/internal/`. The tools read settings (secrets redacted) and create
//! proposals; saving and applying stay with the user (routes/proposals.rs).
use std::sync::Arc;

use rocket::http::Status;
use rocket::request::{FromRequest, Outcome, Request};
use rocket::response::content::RawJson;
use rocket::serde::json::Json;
use rocket::{get, post, State};
use serde_json::{json, Value};

use super::proposals::create_proposal;
use crate::commands::web::proposals::{self, ProposalRequest};
use crate::commands::web::schema_cache::load_options;
use crate::commands::web::types::{AppConfig, OptionSchema};
use crate::commands::web::util::service_name_ok;

const PROTOCOL: &str = "2025-06-18";

/// Request carried a valid bearer token.
pub struct AgentToken;

fn expected_token() -> Option<String> {
    let path = std::env::var("NEO_AGENT_TOKEN_FILE").ok()?;
    let t = std::fs::read_to_string(path).ok()?;
    let t = t.trim().to_string();
    (!t.is_empty()).then_some(t)
}

fn same(a: &str, b: &str) -> bool {
    a.len() == b.len()
        && a.bytes()
            .zip(b.bytes())
            .fold(0u8, |acc, (x, y)| acc | (x ^ y))
            == 0
}

#[rocket::async_trait]
impl<'r> FromRequest<'r> for AgentToken {
    type Error = ();
    async fn from_request(req: &'r Request<'_>) -> Outcome<Self, ()> {
        let Some(want) = expected_token() else {
            return Outcome::Error((Status::NotFound, ()));
        };
        let got = req
            .headers()
            .get_one("Authorization")
            .and_then(|h| h.strip_prefix("Bearer "))
            .unwrap_or_default();
        if same(got.trim(), &want) {
            Outcome::Success(AgentToken)
        } else {
            Outcome::Error((Status::Unauthorized, ()))
        }
    }
}

fn tools() -> Value {
    json!([
        {
            "name": "neo_settings_schema",
            "description": "List the settings of one service (or core) with label, description, type, default and current value. Secrets show only whether they are set.",
            "inputSchema": {
                "type": "object",
                "properties": {"service": {"type": "string", "description": "Service name, e.g. pihole, or \"core\"."}},
                "required": ["service"]
            }
        },
        {
            "name": "neo_settings_get",
            "description": "Current values of settings by option path, e.g. services.pihole.localIP or core.timeZone. Secrets show only whether they are set.",
            "inputSchema": {
                "type": "object",
                "properties": {"paths": {"type": "array", "items": {"type": "string"}}},
                "required": ["paths"]
            }
        },
        {
            "name": "neo_settings_propose",
            "description": "Suggest a settings change. Writes nothing: the user sees a card in the neo web UI and clicks Save or Save & apply. Never send secret values; use op \"generate\" for secrets with a Generate button, or ask the user to enter them. Returns errors to fix, or the proposal id and a link for the user.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "summary": {"type": "string", "description": "One plain sentence for the user, e.g. Turn on Pi-hole for your home network."},
                    "changes": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "op": {"type": "string", "enum": ["set", "unset", "add", "remove", "generate"]},
                                "path": {"type": "string", "description": "Option path, e.g. services.pihole.enabled"},
                                "key": {"type": "string", "description": "Entry name for named-entry settings (attrsOf)."},
                                "value": {"description": "New value (set), or the list item (add/remove)."}
                            },
                            "required": ["op", "path"]
                        }
                    }
                },
                "required": ["summary", "changes"]
            }
        },
        {
            "name": "neo_settings_status",
            "description": "Whether the user saved, applied or rejected a proposal: pending, saved, applied, rejected, stale or failed.",
            "inputSchema": {
                "type": "object",
                "properties": {"id": {"type": "string"}},
                "required": ["id"]
            }
        }
    ])
}

fn redacted(o: &OptionSchema) -> Value {
    let current = o.current.clone().unwrap_or(Value::Null);
    if o.secret {
        let set = !(current.is_null() || current == json!(""));
        json!(if set { "<set>" } else { "<unset>" })
    } else {
        current
    }
}

pub(crate) fn describe(prefix: &str, o: &OptionSchema) -> Value {
    json!({
        "path": format!("{prefix}.{}", o.name),
        "label": o.label,
        "description": o.description,
        "type": o.typeLabel,
        "default": if o.secret { Value::Null } else { o.default.clone() },
        "current": redacted(o),
        "secret": o.secret,
        "generate": o.secret && o.helper.as_ref().is_some_and(|h| h.kind == "button" && h.inputs.is_empty()),
    })
}

async fn pane_options(
    config: &AppConfig,
    service: &str,
) -> Result<(String, Vec<OptionSchema>), String> {
    if service == "core" {
        return load_options(config, true, "core")
            .await
            .map(|o| ("core".to_string(), o));
    }
    if !service_name_ok(service) {
        return Err(format!("{service}: unknown service"));
    }
    load_options(config, false, service)
        .await
        .map(|o| (format!("services.{service}"), o))
}

async fn call(config: &AppConfig, name: &str, args: &Value) -> Result<Value, String> {
    match name {
        "neo_settings_schema" => {
            let service = args["service"].as_str().unwrap_or_default();
            let (prefix, opts) = pane_options(config, service).await?;
            Ok(json!(opts
                .iter()
                .map(|o| describe(&prefix, o))
                .collect::<Vec<_>>()))
        }
        "neo_settings_get" => {
            let mut out = Vec::new();
            for p in args["paths"].as_array().cloned().unwrap_or_default() {
                let path = p.as_str().unwrap_or_default().to_string();
                let (service, rest) = match path.strip_prefix("services.") {
                    Some(r) => {
                        match r.split_once('.') {
                            Some((s, rest)) => (s.to_string(), rest.to_string()),
                            None => {
                                out.push(json!({"path": path, "error": "use services.<service>.<option>"}));
                                continue;
                            }
                        }
                    }
                    None => match path.strip_prefix("core.") {
                        Some(rest) => ("core".to_string(), rest.to_string()),
                        None => {
                            out.push(json!({"path": path, "error": "use services.<service>.<option> or core.<option>"}));
                            continue;
                        }
                    },
                };
                match pane_options(config, &service).await {
                    Ok((prefix, opts)) => match opts.iter().find(|o| o.name == rest) {
                        Some(o) => out.push(describe(&prefix, o)),
                        None => out.push(json!({"path": path, "error": "no such setting"})),
                    },
                    Err(e) => out.push(json!({"path": path, "error": e})),
                }
            }
            Ok(json!(out))
        }
        "neo_settings_propose" => {
            let req: ProposalRequest =
                serde_json::from_value(args.clone()).map_err(|e| format!("bad arguments: {e}"))?;
            let out = create_proposal(config, req).await;
            let mut v = serde_json::to_value(&out).unwrap_or_default();
            if out.ok {
                let link = match std::env::var("NEO_PUBLIC_URL") {
                    Ok(base) if !base.is_empty() => {
                        format!(
                            "{}{}",
                            base.trim_end_matches('/'),
                            out.url.clone().unwrap_or_default()
                        )
                    }
                    _ => out.url.clone().unwrap_or_default(),
                };
                v["link"] = json!(link);
                v["next"] = json!("Tell the user what you suggest and ask them to review it: the card is in the neo chat, or at the link. Check neo_settings_status before continuing.");
            }
            Ok(v)
        }
        "neo_settings_status" => {
            let id = args["id"].as_str().unwrap_or_default();
            match proposals::get(id) {
                Some(p) => Ok(json!({"id": p.id, "status": p.status, "message": p.message})),
                None => Err(format!("{id}: no such proposal (they expire after a day)")),
            }
        }
        other => Err(format!("unknown tool {other}")),
    }
}

fn rpc_result(id: &Value, result: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "result": result})
}

fn rpc_error(id: &Value, code: i64, message: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
}

#[post("/internal/mcp", data = "<body>")]
pub async fn mcp(
    _token: AgentToken,
    config: &State<Arc<AppConfig>>,
    body: Json<Value>,
) -> (Status, RawJson<String>) {
    let msg = body.into_inner();
    let id = msg.get("id").cloned().unwrap_or(Value::Null);
    let method = msg["method"].as_str().unwrap_or_default();
    // Notifications (no id) get 202 and no body.
    if msg.get("id").is_none() {
        return (Status::Accepted, RawJson(String::new()));
    }
    let reply = match method {
        "initialize" => {
            let version = msg["params"]["protocolVersion"]
                .as_str()
                .unwrap_or(PROTOCOL);
            rpc_result(
                &id,
                json!({
                    "protocolVersion": version,
                    "capabilities": {"tools": {}},
                    "serverInfo": {"name": "neo", "version": env!("CARGO_PKG_VERSION")},
                    "instructions": "Neo homeserver settings. Read with neo_settings_schema / neo_settings_get; change settings only with neo_settings_propose, which the user then saves or applies in the web UI."
                }),
            )
        }
        "ping" => rpc_result(&id, json!({})),
        "tools/list" => rpc_result(&id, json!({"tools": tools()})),
        "tools/call" => {
            let name = msg["params"]["name"].as_str().unwrap_or_default();
            let args = msg["params"]["arguments"].clone();
            match call(config, name, &args).await {
                Ok(v) => rpc_result(
                    &id,
                    json!({"content": [{"type": "text", "text": v.to_string()}], "isError": false}),
                ),
                Err(e) => rpc_result(
                    &id,
                    json!({"content": [{"type": "text", "text": e}], "isError": true}),
                ),
            }
        }
        _ => rpc_error(&id, -32601, "method not found"),
    };
    (Status::Ok, RawJson(reply.to_string()))
}

/// No server-to-client stream; clients fall back to plain POST.
#[get("/internal/mcp")]
pub fn mcp_get(_token: AgentToken) -> Status {
    Status::MethodNotAllowed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constant_time_compare() {
        assert!(same("abc", "abc"));
        assert!(!same("abc", "abd"));
        assert!(!same("abc", "abcd"));
    }

    #[test]
    fn tool_list_is_complete() {
        let names: Vec<String> = tools()
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(
            names,
            [
                "neo_settings_schema",
                "neo_settings_get",
                "neo_settings_propose",
                "neo_settings_status"
            ]
        );
    }
}
