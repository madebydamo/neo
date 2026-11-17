//! The evaluation status pages show: `#neo-eval-sync` and `#neo-eval-error`.
use std::collections::HashMap;
use std::sync::atomic::Ordering;

use anyhow::anyhow;

use super::super::errors::eval_error_ui;
use super::query::{Cached, QueryKey};
use super::EvalPool;
use crate::commands::web::util::escape_html;

impl EvalPool {
    /// `#neo-eval-sync` (read by static/eval_sync.js) plus `#neo-eval-error`,
    /// as OOB swaps for the WS or inline for the page.
    pub fn status_html(&self, oob: bool) -> String {
        let st = self.st();
        let gen = st.serving.as_ref().map_or(0, |g| g.id);
        let theme = st
            .serving
            .as_ref()
            .and_then(|g| g.get(&QueryKey::Theme))
            .and_then(|c| match c {
                Cached::Theme(t) => Some((*t).clone()),
                _ => None,
            })
            .unwrap_or_default();
        let json = |m: &HashMap<String, u64>| {
            escape_html(&serde_json::to_string(m).unwrap_or_else(|_| "{}".into()))
        };
        let oob_attr = if oob { r#" hx-swap-oob="true""# } else { "" };
        let sync = format!(
            r#"<div id="neo-eval-sync" hidden{oob_attr} data-gen="{gen}" data-pending="{}" data-theme="{}" data-changed="{}" data-changed-overlaid="{}"></div>"#,
            self.pending.load(Ordering::Relaxed),
            escape_html(&theme),
            json(&st.changed),
            json(&st.changed_overlaid),
        );
        let failure = st
            .failed
            .as_ref()
            .filter(|f| f.content == st.latest.content && st.serving.is_some());
        let error = match failure {
            None => format!(r#"<div id="neo-eval-error"{oob_attr}></div>"#),
            Some(f) => {
                let ui = eval_error_ui(
                    "Your latest change does not evaluate",
                    &anyhow!("{}", f.message),
                );
                format!(
                    r#"<div id="neo-eval-error"{oob_attr}><div role="alert" class="alert alert-error alert-soft mb-3 text-sm items-start"><div class="min-w-0 flex-1"><div class="font-semibold">Your latest change does not evaluate</div><div class="mt-0.5 text-xs opacity-80">Pages show the last configuration that did, with your saved values. Fix the setting and save again.</div><div class="mt-2 text-xs whitespace-pre-wrap break-words font-mono opacity-80 max-h-40 overflow-auto">{}</div></div></div></div>"#,
                    escape_html(ui.error.as_deref().unwrap_or(&f.message)),
                )
            }
        };
        format!("{sync}{error}")
    }

    pub(super) fn push_status(&self) {
        let _ = self.push.send(self.status_html(true));
    }
}
