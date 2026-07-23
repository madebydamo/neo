//! POST /helper/run — execute a declared option helper and return a value to fill in the form.

use std::path::Path;
use std::sync::Arc;

use rocket::http::Status;
use rocket::serde::json::Json;
use rocket::{post, State};
use serde::{Deserialize, Serialize};

use crate::commands::web::helper_exec::{parse_helper_value, run_helper_script};
use crate::commands::web::structs::{AppConfig, OptionHelper, OptionSchema};

/// Where to apply a helper result in nested option forms.
/// `key` is client-only (attrsOf map entry); serde ignores unknown client fields.
#[derive(Deserialize, Default)]
pub struct HelperApplyTarget {
    pub index: Option<usize>,
    pub field: Option<String>,
}

#[derive(Deserialize)]
pub struct HelperRunRequest {
    pub service: String,
    pub option: String,
    #[serde(default)]
    pub is_core: bool,
    #[serde(default)]
    pub target: Option<HelperApplyTarget>,
    #[serde(default)]
    pub inputs: serde_json::Map<String, serde_json::Value>,
}

#[derive(Serialize)]
pub struct HelperRunResponse {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub apply: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

fn err_resp(status: Status, msg: impl Into<String>) -> (Status, Json<HelperRunResponse>) {
    (
        status,
        Json(HelperRunResponse {
            ok: false,
            value: None,
            apply: None,
            error: Some(msg.into()),
        }),
    )
}

fn find_helper<'a>(
    options: &'a [OptionSchema],
    option: &str,
    target: &Option<HelperApplyTarget>,
) -> Option<&'a OptionHelper> {
    let opt = options.iter().find(|o| o.name == option)?;
    if let Some(t) = target {
        if let Some(field) = t.field.as_deref() {
            let fields = opt.r#type.elem.as_ref()?.fields.as_ref()?;
            return fields.iter().find(|f| f.name == field)?.helper.as_ref();
        }
    }
    opt.helper.as_ref()
}

fn target_type_kind(
    options: &[OptionSchema],
    option: &str,
    target: &Option<HelperApplyTarget>,
) -> String {
    let opt = match options.iter().find(|o| o.name == option) {
        Some(o) => o,
        None => return String::new(),
    };
    if let Some(t) = target {
        if let Some(field) = t.field.as_deref() {
            if let Some(fields) = opt.r#type.elem.as_ref().and_then(|e| e.fields.as_ref()) {
                if let Some(f) = fields.iter().find(|f| f.name == field) {
                    return f.r#type.kind.clone();
                }
            }
        }
    }
    opt.r#type.kind.clone()
}

fn validate_inputs(
    helper: &OptionHelper,
    inputs: &serde_json::Map<String, serde_json::Value>,
) -> Result<(), String> {
    let declared: std::collections::HashSet<&str> =
        helper.inputs.iter().map(|i| i.name.as_str()).collect();
    for k in inputs.keys() {
        if !declared.contains(k.as_str()) {
            return Err(format!("unknown input: {k}"));
        }
    }
    for inp in &helper.inputs {
        let val = inputs.get(&inp.name);
        let empty = match val {
            None => true,
            Some(serde_json::Value::Null) => true,
            Some(serde_json::Value::String(s)) => s.is_empty(),
            Some(_) => false,
        };
        if inp.required && empty {
            return Err(format!("{} is required", inp.label));
        }
        if inp.name == "username" {
            if let Some(serde_json::Value::String(u)) = val {
                if u.contains(':') {
                    return Err("username must not contain ':'".into());
                }
            }
        }
    }
    Ok(())
}

fn has_password_input(helper: &OptionHelper) -> bool {
    helper.inputs.iter().any(|i| i.input_type == "password")
}

fn value_ok_for_kind(
    kind: &str,
    apply: &str,
    value: &serde_json::Value,
    element_set: bool,
) -> Result<(), String> {
    match (kind, apply) {
        ("listOf", "append") => {
            if value.is_string() || value.is_number() || value.is_boolean() {
                Ok(())
            } else {
                Err("append value must be a scalar".into())
            }
        }
        // Filling one list row via target.index: value is the element type (usually string).
        ("listOf", "set") if element_set => {
            if value.is_string() || value.is_number() || value.is_boolean() {
                Ok(())
            } else {
                Err("list element value must be a scalar".into())
            }
        }
        ("listOf", "set") => {
            if value.is_array() {
                Ok(())
            } else {
                Err("set on listOf requires an array value".into())
            }
        }
        ("str" | "path", _) | ("nullOr", _) => {
            if value.is_string() {
                Ok(())
            } else {
                Err("value must be a string".into())
            }
        }
        ("int" | "port", _) => {
            if value.as_i64().is_some() {
                Ok(())
            } else {
                Err("value must be an integer".into())
            }
        }
        ("float", _) => {
            if value.as_f64().is_some() {
                Ok(())
            } else {
                Err("value must be a number".into())
            }
        }
        ("bool", _) => {
            if value.is_boolean() {
                Ok(())
            } else {
                Err("value must be a boolean".into())
            }
        }
        _ => {
            // Default: accept string (v1 helpers produce strings)
            if value.is_string() {
                Ok(())
            } else {
                Err("value type not accepted for this field".into())
            }
        }
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

#[post("/helper/run", data = "<body>")]
pub async fn run_helper(
    config: &State<Arc<AppConfig>>,
    body: Json<HelperRunRequest>,
) -> (Status, Json<HelperRunResponse>) {
    let req = body.into_inner();
    if req.service.is_empty() || req.option.is_empty() {
        return err_resp(Status::BadRequest, "service and option are required");
    }

    let options = match load_options(config, req.is_core, &req.service).await {
        Ok(o) => o,
        Err(e) => return err_resp(Status::ServiceUnavailable, e),
    };

    let helper = match find_helper(&options, &req.option, &req.target) {
        Some(h) => h.clone(),
        None => return err_resp(Status::NotFound, "no helper on this option"),
    };

    if helper.kind != "button" && helper.kind != "form" {
        return err_resp(Status::InternalServerError, "invalid helper kind");
    }
    if helper.apply != "set" && helper.apply != "append" {
        return err_resp(Status::InternalServerError, "invalid helper apply mode");
    }
    if helper.script.is_empty() {
        return err_resp(Status::InternalServerError, "helper has no script");
    }

    if let Err(e) = validate_inputs(&helper, &req.inputs) {
        return err_resp(Status::UnprocessableEntity, e);
    }

    let script_path = Path::new(&helper.script);
    // Security invariant: executable path only from server-side schema.
    if !script_path.is_absolute() {
        return err_resp(Status::InternalServerError, "invalid helper script path");
    }

    let stdin = serde_json::Value::Object(req.inputs.clone()).to_string();
    let passwordish = has_password_input(&helper);
    let env_extra = [
        ("NEO_HELPER_ID", helper.id.as_str()),
        ("NEO_HELPER_SERVICE", req.service.as_str()),
        ("NEO_HELPER_OPTION", req.option.as_str()),
    ];

    let result = match run_helper_script(script_path, &stdin, &env_extra).await {
        Ok(r) => r,
        Err(e) => {
            eprintln!(
                "web: helper {} failed to start (id={}): {e:#}",
                helper.id, helper.id
            );
            return err_resp(Status::InternalServerError, "Helper failed to start");
        }
    };

    if result.timed_out {
        eprintln!("web: helper {} timed out", helper.id);
        return err_resp(Status::GatewayTimeout, "Helper timed out");
    }

    if result.exit_code != 0 {
        // Exit 127 = command not found; safe to surface without leaking secrets.
        if result.exit_code == 127 {
            eprintln!(
                "web: helper {} exited 127 (tool missing on PATH): {}",
                helper.id,
                result
                    .stderr
                    .chars()
                    .take(200)
                    .collect::<String>()
                    .replace('\n', " ")
            );
            return err_resp(
                Status::UnprocessableEntity,
                "Helper tool not found on server PATH (need htpasswd/mkpasswd and jq; set NEO_HELPER_PATH or activate neo-web)",
            );
        }
        if passwordish {
            eprintln!(
                "web: helper {} exited {} (password helper; stderr suppressed)",
                helper.id, result.exit_code
            );
            return err_resp(
                Status::UnprocessableEntity,
                format!("Helper failed (exit {})", result.exit_code),
            );
        }
        let snippet: String = result.stderr.chars().take(512).collect();
        eprintln!(
            "web: helper {} exited {}: {}",
            helper.id,
            result.exit_code,
            snippet.replace('\n', " ")
        );
        let msg = if snippet.trim().is_empty() {
            format!("Helper failed (exit {})", result.exit_code)
        } else {
            snippet.trim().to_string()
        };
        return err_resp(Status::UnprocessableEntity, msg);
    }

    let value = match parse_helper_value(&result.stdout) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("web: helper {} bad output: {e:#}", helper.id);
            return err_resp(
                Status::InternalServerError,
                "Helper produced invalid output",
            );
        }
    };

    let kind = target_type_kind(&options, &req.option, &req.target);
    let element_set = req
        .target
        .as_ref()
        .map(|t| t.index.is_some() && t.field.is_none())
        .unwrap_or(false);
    if let Err(e) = value_ok_for_kind(&kind, &helper.apply, &value, element_set) {
        return err_resp(Status::UnprocessableEntity, e);
    }

    (
        Status::Ok,
        Json(HelperRunResponse {
            ok: true,
            value: Some(value),
            apply: Some(helper.apply),
            error: None,
        }),
    )
}
