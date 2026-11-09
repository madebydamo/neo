//! Settings proposals from the assistant: create (JSON), show as a card, and
//! save / apply / reject on the user's click. Saving runs `apply_edit` and
//! applying runs `apply_or_activate`, exactly like the form and Apply button.
use std::collections::HashMap;
use std::sync::Arc;

use rocket::form::Form;

use rocket::http::Status as HttpStatus;
use rocket::response::content::RawHtml;
use rocket::serde::json::Json;
use rocket::{get, post, State};
use rocket_dyn_templates::{Metadata, Template};
use serde::Serialize;

use super::changes::apply_or_activate;
use super::pages::configuration_shell;
use crate::commands::web::git::{collect_changes, DiffRange};
use crate::commands::web::proposals::{
    self, AskInput, ChangeView, Proposal, ProposalRequest, Status,
};
use crate::commands::web::settings::edit::{apply_edit, Edit};
use crate::commands::web::types::AppConfig;
use crate::commands::web::util::{proc, Htmx};

#[derive(Serialize)]
pub struct ProposalCreated {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub changes: Vec<ChangeView>,
}

/// Create a proposal (used by the assistant tools). Writes nothing.
pub async fn create_proposal(config: &AppConfig, req: ProposalRequest) -> ProposalCreated {
    match proposals::create(config, req).await {
        Ok(p) => ProposalCreated {
            ok: true,
            url: Some(format!("/proposals/{}", p.id)),
            id: Some(p.id),
            errors: vec![],
            changes: p.views,
        },
        Err(errors) => ProposalCreated {
            ok: false,
            id: None,
            url: None,
            errors,
            changes: vec![],
        },
    }
}

#[post("/proposals", data = "<req>")]
pub async fn proposals_create(
    config: &State<Arc<AppConfig>>,
    req: Json<ProposalRequest>,
) -> (HttpStatus, Json<ProposalCreated>) {
    let out = create_proposal(config, req.into_inner()).await;
    let code = if out.ok {
        HttpStatus::Created
    } else {
        HttpStatus::UnprocessableEntity
    };
    (code, Json(out))
}

#[derive(Serialize)]
struct CardContext {
    found: bool,
    id: String,
    summary: String,
    changes: Vec<ChangeView>,
    /// Password inputs for asked secrets (one per group).
    asks: Vec<AskInput>,
    status: Status,
    pending: bool,
    done: bool,
    message: String,
    /// Other saved-but-not-applied changes (Apply activates them too).
    other_pending: bool,
}

async fn has_pending_changes(config: &AppConfig) -> bool {
    let dir = config.config_dir();
    proc::blocking(move || collect_changes(&dir, DiffRange::Worktree).is_ok_and(|s| !s.is_empty()))
        .await
        .unwrap_or(false)
}

async fn card(config: &AppConfig, p: Option<Proposal>) -> Template {
    Template::render("proposal_card", card_ctx(config, p).await)
}

async fn card_ctx(config: &AppConfig, p: Option<Proposal>) -> CardContext {
    let other_pending = has_pending_changes(config).await;
    {
        match p {
            Some(p) => CardContext {
                found: true,
                pending: p.status.is_actionable(),
                done: matches!(p.status, Status::Saved | Status::Applied | Status::Rejected),
                id: p.id,
                summary: p.summary,
                changes: p.views,
                asks: p.asks,
                status: p.status,
                message: p.message,
                other_pending,
            },
            None => CardContext {
                found: false,
                id: String::new(),
                summary: String::new(),
                changes: vec![],
                asks: vec![],
                status: Status::Stale,
                pending: false,
                done: false,
                message: String::new(),
                other_pending: false,
            },
        }
    }
}

#[get("/proposals/<id>/card")]
pub async fn proposal_card(config: &State<Arc<AppConfig>>, id: &str) -> Template {
    card(config, proposals::get(id)).await
}

/// Values typed into the card's secret inputs (`ask_*` form fields). They go
/// straight into the patch: never stored, logged or shown to the assistant.
pub type Asked = Option<Form<HashMap<String, String>>>;

fn asked(form: Asked) -> HashMap<String, String> {
    form.map(Form::into_inner)
        .unwrap_or_default()
        .into_iter()
        .filter(|(k, _)| k.starts_with("ask_"))
        .collect()
}

/// Save through the form's write path; `Err` is a message for the card.
async fn save(
    config: &AppConfig,
    p: &Proposal,
    asked: &HashMap<String, String>,
) -> Result<(), (Status, String)> {
    let ops = proposals::ops_for_save(config, p, asked).await?;
    apply_edit(config, Edit::Patch { ops })
        .await
        .map_err(|e| (Status::Failed, e.message().to_string()))
}

#[post("/proposals/<id>/save", data = "<form>")]
pub async fn proposal_save(config: &State<Arc<AppConfig>>, id: &str, form: Asked) -> Template {
    let Some(p) = proposals::get(id).filter(|p| p.status.is_actionable()) else {
        return card(config, proposals::get(id)).await;
    };
    let p = match save(config, &p, &asked(form)).await {
        Ok(()) => proposals::set_status(
            id,
            Status::Saved,
            "Saved. Apply from the bar at the top when you are ready.",
        ),
        Err((s, m)) => proposals::set_status(id, s, &m),
    };
    card(config, p).await
}

/// Save, then activate like the Apply button. The activation output goes to
/// the changes dialog (out-of-band swap), the card shows the result.
#[post("/proposals/<id>/apply", data = "<form>")]
pub async fn proposal_apply(
    config: &State<Arc<AppConfig>>,
    id: &str,
    form: Asked,
    meta: Metadata<'_>,
) -> RawHtml<String> {
    let Some(p) = proposals::get(id).filter(|p| p.status.is_actionable()) else {
        return render_card(&meta, config, proposals::get(id), String::new()).await;
    };
    if let Err((s, m)) = save(config, &p, &asked(form)).await {
        return render_card(
            &meta,
            config,
            proposals::set_status(id, s, &m),
            String::new(),
        )
        .await;
    }
    let (p, extra) = match apply_or_activate().await {
        Ok(out) => (
            proposals::set_status(
                id,
                Status::Applied,
                "Saved and applying. Progress is shown in the dialog.",
            ),
            format!(
                r#"<div id="changes-body" hx-swap-oob="innerHTML">{}</div>"#,
                out.0
            ),
        ),
        Err(b) => (
            proposals::set_status(
                id,
                Status::Saved,
                &format!(
                    "Saved, but not applied yet: {} Apply from the bar at the top later.",
                    b.0
                ),
            ),
            String::new(),
        ),
    };
    render_card(&meta, config, p, extra).await
}

async fn render_card(
    meta: &Metadata<'_>,
    config: &AppConfig,
    p: Option<Proposal>,
    extra: String,
) -> RawHtml<String> {
    let ctx = card_ctx(config, p).await;
    let html = meta
        .render("proposal_card", ctx)
        .map(|(_, h)| h)
        .unwrap_or_default();
    RawHtml(format!("{html}{extra}"))
}

#[post("/proposals/<id>/reject")]
pub async fn proposal_reject(config: &State<Arc<AppConfig>>, id: &str) -> Template {
    let p = proposals::get(id)
        .filter(|p| p.status.is_actionable() || p.status == Status::Stale)
        .and_then(|_| proposals::set_status(id, Status::Rejected, "Rejected. Nothing was changed."))
        .or_else(|| proposals::get(id));
    card(config, p).await
}

/// Full page for a proposal (link from Telegram or the assistant's dashboard).
#[get("/proposals/<id>")]
pub async fn proposal_page(
    config: &State<Arc<AppConfig>>,
    id: &str,
    htmx: Htmx,
    meta: Metadata<'_>,
) -> Template {
    if htmx.is_htmx() {
        return card(config, proposals::get(id)).await;
    }
    let ctx = card_ctx(config, proposals::get(id)).await;
    let html = meta
        .render("proposal_card", ctx)
        .map(|(_, h)| format!(r#"<div class="max-w-2xl">{h}</div>"#));
    configuration_shell(
        config,
        &format!("/proposals/{id}"),
        "services",
        Some("Suggested change".into()),
        html,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asked_values_come_from_ask_fields() {
        let f: HashMap<String, String> = Form::parse_encoded(rocket::http::RawStr::new(
            "ask_g_tmr-password=p%40ss+w&other=x&ask_1=",
        ))
        .unwrap();
        let a = asked(Some(Form::from(f)));
        assert_eq!(
            a.get("ask_g_tmr-password").map(String::as_str),
            Some("p@ss w")
        );
        assert_eq!(a.get("ask_1").map(String::as_str), Some(""));
        assert!(!a.contains_key("other"));
        assert!(asked(None).is_empty());
    }

    #[test]
    fn card_renders_actions_warnings_and_asks() {
        use rocket_dyn_templates::handlebars::Handlebars;
        let src = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/templates/proposal_card.html.hbs"
        ))
        .unwrap();
        let mut hb = Handlebars::new();
        hb.register_template_string("card", src).unwrap();
        let view = |action: &str, warnings: Vec<String>| ChangeView {
            service: "x".into(),
            label: "Accounts › tmr › Password".into(),
            path: "services.x.accounts.tmr.password".into(),
            action: action.into(),
            old: "not set".into(),
            new: "you enter it below".into(),
            secret: true,
            warnings,
            notes: vec!["Adds entry tmr".into()],
        };
        let ctx = CardContext {
            found: true,
            id: "0123456789abcdef".into(),
            summary: "Add a test login".into(),
            changes: vec![
                view("add", vec![]),
                view("change", vec!["Removes entry tinyauth".into()]),
            ],
            asks: vec![AskInput {
                name: "ask_g_tmr".into(),
                label: "Password".into(),
                used_for: vec![
                    "a: Accounts › tmr › Password".into(),
                    "b: Accounts › tmr › Password".into(),
                ],
            }],
            status: Status::Pending,
            pending: true,
            done: false,
            message: String::new(),
            other_pending: false,
        };
        let html = hb.render("card", &ctx).unwrap();
        assert!(html.contains(">New</span>"));
        assert!(html.contains("Removes entry tinyauth"));
        assert!(html.contains(r#"name="ask_g_tmr""#));
        assert!(html.contains(r##"hx-include="#proposal-0123456789abcdef [data-neo-ask]""##));
        assert!(html.contains("Used for a: Accounts"));
    }
}
