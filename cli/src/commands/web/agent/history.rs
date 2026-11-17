//! Pure transforms of what Hermes returns into what the drawer shows: the
//! replayable history of a session, a run's state and the past chats list.
use serde_json::Value;

use super::{context_of, valid_session_id};

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
                let t = crate::commands::web::agent_refs::strip_block(&text).trim();
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
pub(super) const HISTORY_PAGE: usize = 500;

/// Raw messages in conversation order: by Hermes' row id when every message has
/// one (a page read from the newest end must still replay oldest first).
pub(super) fn chronological(mut raw: Vec<Value>) -> Vec<Value> {
    if raw.iter().all(|m| m["id"].as_i64().is_some()) {
        raw.sort_by_key(|m| m["id"].as_i64().unwrap_or_default());
    }
    raw
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
pub(super) fn session_entry(s: &Value) -> Option<Value> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_keeps_text_and_tool_calls() {
        use serde_json::json;
        let raw = vec![
            json!({"role": "user", "content": format!("is @pihole on?{}\n{{}}\n</neo-references>", crate::commands::web::agent_refs::BLOCK_START)}),
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
}
