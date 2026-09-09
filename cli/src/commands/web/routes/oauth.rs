//! POST /widget/oauth — run a schema-declared OAuth helper for providerAuth.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use rocket::http::Status;
use rocket::serde::json::Json;
use rocket::{post, State};
use serde::{Deserialize, Serialize};

use crate::commands::web::helper_exec::run_widget_script;
use crate::commands::web::structs::{AppConfig, OptionSchema, OptionUiOauth};
use crate::commands::web::util::{
    oauth_env_key_ok, oauth_session_ok, option_name_ok, provider_id_ok, run_as_ok, service_name_ok,
};

#[derive(Deserialize)]
pub struct OauthRunRequest {
    pub service: String,
    pub option: String,
    #[serde(default)]
    pub is_core: bool,
    pub action: String,
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub session: Option<String>,
    #[serde(default)]
    pub code: Option<String>,
}

#[derive(Serialize)]
pub struct OauthRunResponse {
    pub ok: bool,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

fn err_resp(status: Status, msg: impl Into<String>) -> (Status, Json<OauthRunResponse>) {
    let mut extra = serde_json::Map::new();
    extra.insert("error".into(), serde_json::Value::String(msg.into()));
    (status, Json(OauthRunResponse { ok: false, extra }))
}

fn action_ok(action: &str) -> bool {
    matches!(action, "status" | "login" | "poll" | "submit" | "refresh")
}

fn timeout_for(action: &str) -> Duration {
    match action {
        "login" | "submit" => Duration::from_secs(45),
        "refresh" => Duration::from_secs(30),
        _ => Duration::from_secs(20),
    }
}

async fn load_options(
    config: &AppConfig,
    is_core: bool,
    name: &str,
) -> Result<Vec<OptionSchema>, String> {
    {
        let cache = config.schema_cache.read().await;
        if let Some(opts) = cache.get(is_core, name) {
            return Ok(opts);
        }
    }
    let mut ev = config.evaluator.lock().await;
    let pane = if is_core {
        ev.extract_neo_section(name).await
    } else {
        ev.extract_service_options(name).await
    };
    drop(ev);
    if let Some(err) = pane.eval_error.error {
        return Err(err);
    }
    let opts = pane.options;
    {
        let mut cache = config.schema_cache.write().await;
        cache.put(is_core, name, opts.clone());
    }
    Ok(opts)
}

fn find_oauth<'a>(options: &'a [OptionSchema], option: &str) -> Option<&'a OptionUiOauth> {
    options
        .iter()
        .find(|o| o.name == option)
        .and_then(|o| o.ui.as_ref())
        .and_then(|ui| ui.oauth.as_ref())
}

fn sanitized_env(env: &Option<BTreeMap<String, String>>) -> Result<Vec<(String, String)>, String> {
    let mut out = Vec::new();
    let Some(map) = env else {
        return Ok(out);
    };
    for (k, v) in map {
        if !oauth_env_key_ok(k) {
            return Err(format!("invalid oauth env key {k}"));
        }
        if v.bytes().any(|b| b == 0 || b == b'\n' || b == b'\r') {
            return Err(format!("invalid oauth env value for {k}"));
        }
        out.push((k.clone(), v.clone()));
    }
    Ok(out)
}

#[post("/widget/oauth", data = "<body>")]
pub async fn run_oauth(
    config: &State<Arc<AppConfig>>,
    body: Json<OauthRunRequest>,
) -> (Status, Json<OauthRunResponse>) {
    let req = body.into_inner();
    if !service_name_ok(&req.service) || !option_name_ok(&req.option) {
        return err_resp(Status::BadRequest, "invalid service or option");
    }
    if !action_ok(&req.action) {
        return err_resp(Status::BadRequest, "invalid action");
    }
    let provider = req.provider.as_deref().unwrap_or("").trim();
    if matches!(req.action.as_str(), "status" | "login" | "refresh") {
        if !provider_id_ok(provider) {
            return err_resp(Status::BadRequest, "invalid provider");
        }
    }
    let session = req.session.as_deref().unwrap_or("").trim();
    if matches!(req.action.as_str(), "poll" | "submit") {
        if !oauth_session_ok(session) {
            return err_resp(Status::BadRequest, "invalid session");
        }
    }

    let options = match load_options(config, req.is_core, &req.service).await {
        Ok(o) => o,
        Err(e) => return err_resp(Status::ServiceUnavailable, e),
    };
    let oauth = match find_oauth(&options, &req.option) {
        Some(o) => o.clone(),
        None => return err_resp(Status::NotFound, "no oauth helper on this option"),
    };
    if oauth.script.is_empty() {
        return err_resp(Status::InternalServerError, "oauth helper has no script");
    }
    let script_path = Path::new(&oauth.script);
    if !script_path.is_absolute() {
        return err_resp(Status::InternalServerError, "invalid oauth script path");
    }
    let run_as = oauth.run_as.as_deref();
    if let Some(user) = run_as {
        if !run_as_ok(user) {
            return err_resp(Status::InternalServerError, "invalid oauth runAs");
        }
    }
    let env_pairs = match sanitized_env(&oauth.env) {
        Ok(v) => v,
        Err(e) => return err_resp(Status::InternalServerError, e),
    };
    let env_refs: Vec<(&str, &str)> = env_pairs
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();

    let mut stdin_map = serde_json::Map::new();
    stdin_map.insert(
        "action".into(),
        serde_json::Value::String(req.action.clone()),
    );
    if !provider.is_empty() {
        stdin_map.insert(
            "provider".into(),
            serde_json::Value::String(provider.to_string()),
        );
    }
    if !session.is_empty() {
        stdin_map.insert(
            "session".into(),
            serde_json::Value::String(session.to_string()),
        );
    }
    if let Some(code) = req.code.as_deref() {
        if code.len() > 4096 {
            return err_resp(Status::BadRequest, "code too long");
        }
        stdin_map.insert("code".into(), serde_json::Value::String(code.to_string()));
    }
    let stdin = serde_json::Value::Object(stdin_map).to_string();

    let result = match run_widget_script(
        script_path,
        &stdin,
        &env_refs,
        run_as,
        timeout_for(&req.action),
    )
    .await
    {
        Ok(r) => r,
        Err(e) => {
            eprintln!("web: oauth helper failed to start: {e:#}");
            return err_resp(Status::InternalServerError, "OAuth helper failed to start");
        }
    };
    if result.timed_out {
        return err_resp(Status::GatewayTimeout, "OAuth helper timed out");
    }

    let trimmed = result.stdout.trim();
    let parsed: serde_json::Value = match serde_json::from_str(trimmed) {
        Ok(v) => v,
        Err(_) => {
            let snippet: String = result.stderr.chars().take(400).collect();
            let msg = if snippet.trim().is_empty() {
                format!("OAuth helper failed (exit {})", result.exit_code)
            } else {
                snippet.trim().to_string()
            };
            return err_resp(Status::UnprocessableEntity, msg);
        }
    };
    let mut extra = match parsed {
        serde_json::Value::Object(map) => map,
        other => {
            let mut m = serde_json::Map::new();
            m.insert("value".into(), other);
            m
        }
    };
    // Never forward secrets if a helper regresses.
    for key in [
        "access_token",
        "refresh_token",
        "api_key",
        "token",
        "accessToken",
    ] {
        extra.remove(key);
    }
    let ok = extra
        .get("ok")
        .and_then(|v| v.as_bool())
        .unwrap_or(result.exit_code == 0);
    extra.remove("ok");
    let status = if ok {
        Status::Ok
    } else {
        Status::UnprocessableEntity
    };
    (status, Json(OauthRunResponse { ok, extra }))
}
