//! Settings proposals from the assistant: create (JSON), show as a card, and
//! save / apply / reject on the user's click. Saving runs `apply_edit` and
//! applying runs `apply_or_activate`, exactly like the form and Apply button.
use std::sync::Arc;

use rocket::http::Status as HttpStatus;
use rocket::response::content::RawHtml;
use rocket::serde::json::Json;
use rocket::{get, post, State};
use rocket_dyn_templates::{Metadata, Template};
use serde::Serialize;

use super::changes::apply_or_activate;
use super::pages::configuration_shell;
use crate::commands::web::git::{collect_changes, DiffRange};
use crate::commands::web::proposals::{self, ChangeView, Proposal, ProposalRequest, Status};
use crate::commands::web::settings::edit::{apply_edit, Edit};
use crate::commands::web::types::AppConfig;
use crate::commands::web::util::{config_dir, Htmx};

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
    status: Status,
    pending: bool,
    done: bool,
    message: String,
    /// Other saved-but-not-applied changes (Apply activates them too).
    other_pending: bool,
}

fn has_pending_changes(config: &AppConfig) -> bool {
    let dir = config_dir(&config.settings_path);
    collect_changes(&dir, DiffRange::Worktree).is_ok_and(|s| !s.is_empty())
}

fn card(config: &AppConfig, p: Option<Proposal>) -> Template {
    Template::render("proposal_card", card_ctx(config, p))
}

fn card_ctx(config: &AppConfig, p: Option<Proposal>) -> CardContext {
    let other_pending = has_pending_changes(config);
    {
        match p {
            Some(p) => CardContext {
                found: true,
                pending: p.status == Status::Pending || p.status == Status::Failed,
                done: matches!(p.status, Status::Saved | Status::Applied | Status::Rejected),
                id: p.id,
                summary: p.summary,
                changes: p.views,
                status: p.status,
                message: p.message,
                other_pending,
            },
            None => CardContext {
                found: false,
                id: String::new(),
                summary: String::new(),
                changes: vec![],
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
pub fn proposal_card(config: &State<Arc<AppConfig>>, id: &str) -> Template {
    card(config, proposals::get(id))
}

/// Save through the form's write path; `Err` is a message for the card.
async fn save(config: &AppConfig, p: &Proposal) -> Result<(), (Status, String)> {
    let ops = proposals::ops_for_save(config, p)
        .await
        .map_err(|m| (Status::Stale, m))?;
    match apply_edit(config, Edit::Patch { ops }).await {
        Ok(s) if s == HttpStatus::Ok => Ok(()),
        Ok(s) => Err((
            Status::Failed,
            format!("Saving failed ({}). Nothing was changed.", s.code),
        )),
        Err(b) => Err((Status::Failed, b.0)),
    }
}

#[post("/proposals/<id>/save")]
pub async fn proposal_save(config: &State<Arc<AppConfig>>, id: &str) -> Template {
    let Some(p) =
        proposals::get(id).filter(|p| p.status == Status::Pending || p.status == Status::Failed)
    else {
        return card(config, proposals::get(id));
    };
    let p = match save(config, &p).await {
        Ok(()) => proposals::set_status(
            id,
            Status::Saved,
            "Saved. Apply from the bar at the top when you are ready.",
        ),
        Err((s, m)) => proposals::set_status(id, s, &m),
    };
    card(config, p)
}

/// Save, then activate like the Apply button. The activation output goes to
/// the changes dialog (out-of-band swap), the card shows the result.
#[post("/proposals/<id>/apply")]
pub async fn proposal_apply(
    config: &State<Arc<AppConfig>>,
    id: &str,
    meta: Metadata<'_>,
) -> RawHtml<String> {
    let render = |p: Option<Proposal>, extra: String| {
        let html = meta
            .render("proposal_card", card_ctx(config, p))
            .map(|(_, h)| h)
            .unwrap_or_default();
        RawHtml(format!("{html}{extra}"))
    };
    let Some(p) =
        proposals::get(id).filter(|p| p.status == Status::Pending || p.status == Status::Failed)
    else {
        return render(proposals::get(id), String::new());
    };
    if let Err((s, m)) = save(config, &p).await {
        return render(proposals::set_status(id, s, &m), String::new());
    }
    match apply_or_activate(config) {
        Ok(out) => render(
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
        Err(b) => render(
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
    }
}

#[post("/proposals/<id>/reject")]
pub fn proposal_reject(config: &State<Arc<AppConfig>>, id: &str) -> Template {
    let p = proposals::get(id)
        .filter(|p| {
            p.status == Status::Pending || p.status == Status::Failed || p.status == Status::Stale
        })
        .and_then(|_| proposals::set_status(id, Status::Rejected, "Rejected. Nothing was changed."))
        .or_else(|| proposals::get(id));
    card(config, p)
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
        return card(config, proposals::get(id));
    }
    let html = meta
        .render("proposal_card", card_ctx(config, proposals::get(id)))
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
