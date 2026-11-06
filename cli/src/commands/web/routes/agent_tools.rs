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
use crate::commands::web::redact::{redacted_current, redacted_default};
use crate::commands::web::schema_cache::load_options;
use crate::commands::web::types::{AppConfig, OptionHelper, OptionSchema, OptionType};
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
            "description": "List the settings of one service (or core) with label, description, type, default and current value. Secrets (also inside entries) show only <set> or <unset>. Settings with named entries (attrsOf) list their entry names and the fields of an entry.",
            "inputSchema": {
                "type": "object",
                "properties": {"service": {"type": "string", "description": "Service name, e.g. pihole, or \"core\"."}},
                "required": ["service"]
            }
        },
        {
            "name": "neo_settings_get",
            "description": "Current values of settings by option path, e.g. services.pihole.localIP or core.timeZone. Secrets (also inside entries) show only <set> or <unset>.",
            "inputSchema": {
                "type": "object",
                "properties": {"paths": {"type": "array", "items": {"type": "string"}}},
                "required": ["paths"]
            }
        },
        {
            "name": "neo_settings_propose",
            "description": concat!(
                "Suggest a settings change. Writes nothing: the user sees a card in the neo web UI and clicks Save or Save & apply. ",
                "Address one entry of a named-entry setting with key and one field inside it with field; never rewrite the whole map to add or change one entry. ",
                "Never send secret values (also not inside a value): use op copy or move when another setting already has the value, op generate (the secret's Generate button, field generate=true) or op ask (the card shows a password input; ask changes with the same group share one input). ",
                "copy and move take from {path, key?, field?} and work on a whole setting, one entry or one field, secrets included: the server reads the value when the user saves and you never see it, so this is how to copy accounts between services, rename an entry (move inside one setting) or move a secret. Secrets can only go to secret settings. ",
                "Example, add login tmr to accounts: ",
                r#"{"op":"set","path":"services.x.accounts","key":"tmr","field":"username","value":"tmr"}, "#,
                r#"{"op":"ask","path":"services.x.accounts","key":"tmr","field":"password","group":"tmr-password"}. "#,
                "Example, copy an account with its password to another service: ",
                r#"{"op":"copy","path":"services.b.accounts","key":"tinyauth","from":{"path":"services.a.accounts","key":"tinyauth"}}. "#,
                "Example, rename an entry: ",
                r#"{"op":"move","path":"services.x.accounts","key":"new","from":{"path":"services.x.accounts","key":"old"}}. "#,
                "Returns errors to fix, or the proposal id and a link for the user."
            ),
            "inputSchema": {
                "type": "object",
                "properties": {
                    "summary": {"type": "string", "description": "One plain sentence for the user, e.g. Turn on Pi-hole for your home network."},
                    "changes": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "op": {"type": "string", "enum": ["set", "unset", "add", "remove", "generate", "ask", "copy", "move"], "description": "set a value, unset (back to the default; with key: remove that entry), add/remove a list item, generate a secret with its Generate button, ask the user for a secret on the card, copy or move the value of another setting (from), secrets included."},
                                "path": {"type": "string", "description": "Option path, e.g. services.pihole.enabled or services.x.accounts"},
                                "key": {"type": "string", "description": "Entry name inside a named-entry setting (attrsOf), e.g. tmr. The entry is created when it does not exist."},
                                "field": {"type": "string", "description": "Field inside the entry, e.g. password (dotted for nested fields). See fields in neo_settings_schema."},
                                "value": {"description": "New value (set), or the list item (add/remove). Never a secret."},
                                "group": {"type": "string", "description": "ask only: changes with the same group share one input on the card (the same password for several settings)."},
                                "from": {
                                    "type": "object",
                                    "description": "copy and move only: the setting to read. path, plus key and field like the target.",
                                    "properties": {"path": {"type": "string"}, "key": {"type": "string"}, "field": {"type": "string"}},
                                    "required": ["path"]
                                }
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

fn can_generate(secret: bool, helper: Option<&OptionHelper>) -> bool {
    secret && helper.is_some_and(|h| h.kind == "button" && h.inputs.is_empty())
}

fn strip_null(t: &OptionType) -> &OptionType {
    match (t.kind.as_str(), t.elem.as_deref()) {
        ("nullOr", Some(e)) => strip_null(e),
        _ => t,
    }
}

/// Fields of an entry (attrsOf of submodule) or of a submodule: the shape
/// without values, so the assistant can address one with `field`.
fn entry_fields(t: &OptionType) -> Option<Vec<Value>> {
    let t = strip_null(t);
    let t = if t.kind == "attrsOf" {
        strip_null(t.elem.as_deref()?)
    } else {
        t
    };
    let fields = t.fields.as_deref().filter(|f| !f.is_empty())?;
    Some(
        fields
            .iter()
            .map(|f| {
                json!({
                    "field": f.name,
                    "label": f.label,
                    "type": f.typeLabel,
                    "secret": f.secret,
                    "generate": can_generate(f.secret, f.helper.as_ref()),
                })
            })
            .collect(),
    )
}

pub(crate) fn describe(prefix: &str, o: &OptionSchema) -> Value {
    let mut v = json!({
        "path": format!("{prefix}.{}", o.name),
        "label": o.label,
        "description": o.description,
        "type": o.typeLabel,
        "default": redacted_default(o),
        "current": redacted_current(o),
        "secret": o.secret,
        "generate": can_generate(o.secret, o.helper.as_ref()),
    });
    if strip_null(&o.r#type).kind == "attrsOf" {
        let keys: Vec<&String> = o
            .current
            .as_ref()
            .and_then(Value::as_object)
            .map(|m| m.keys().collect())
            .unwrap_or_default();
        v["entries"] = json!(keys);
    }
    if let Some(fields) = entry_fields(&o.r#type) {
        v["fields"] = json!(fields);
    }
    v
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
                    "instructions": "Neo homeserver settings. Read with neo_settings_schema / neo_settings_get; change settings only with neo_settings_propose, which the user then saves or applies in the web UI. Change one entry or field with key / field instead of rewriting a whole map. Never send secrets: use op copy or move (from another setting, you never see the value), generate, or ask (the user types it on the card). Never read or edit the secrets files or read secrets out of containers."
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
    fn describe_redacts_and_shows_entry_shape() {
        let o = crate::commands::web::redact::tests::accounts();
        let v = describe("services.x", &o);
        let text = v.to_string();
        assert!(!text.contains("thispasswordisfortestingonly"), "{text}");
        assert_eq!(v["current"]["tinyauth"]["password"], "<set>");
        assert_eq!(v["entries"], json!(["tinyauth"]));
        let fields = v["fields"].as_array().unwrap();
        let pw = fields.iter().find(|f| f["field"] == "password").unwrap();
        assert_eq!(pw["secret"], true);
        assert_eq!(pw["generate"], true);
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
