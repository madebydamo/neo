//! Activation/update progress timeline (daisyUI horizontal steps).
#[derive(Clone, Copy)]
pub(crate) enum OpKind {
    Activation,
    Update,
}

/// Ordered UI steps for an operation. Last label is always "Finished" so the
/// final working phase is never shown as a full bar (which reads as "done").
pub(crate) struct ProgressSteps {
    pub labels: &'static [&'static str],
}

const ACTIVATION_STEPS: ProgressSteps = ProgressSteps {
    labels: &[
        "Start",
        "Write Flake",
        "Build",
        "Save Checkpoints",
        "Switch",
        "Finished",
    ],
};

const UPDATE_STEPS: ProgressSteps = ProgressSteps {
    labels: &[
        "Start",
        "Reinitialize",
        "Write Flake",
        "Update Dependencies",
        "Migrate",
        "Finished",
    ],
};

impl OpKind {
    pub(crate) fn monitor_id(self) -> &'static str {
        match self {
            OpKind::Activation => "activation-monitor",
            OpKind::Update => "update-monitor",
        }
    }

    pub(crate) fn title(self) -> &'static str {
        match self {
            OpKind::Activation => "Activation",
            OpKind::Update => "Update",
        }
    }

    pub(crate) fn log_div_id(self) -> &'static str {
        match self {
            OpKind::Activation => "act-log",
            OpKind::Update => "update-log",
        }
    }

    pub(crate) fn status_div_id(self) -> &'static str {
        match self {
            OpKind::Activation => "act-status",
            OpKind::Update => "update-status",
        }
    }

    pub(crate) fn log_path(self, id: &str) -> String {
        match self {
            OpKind::Activation => format!("/activation/log/{}", id),
            OpKind::Update => format!("/update/log/{}", id),
        }
    }

    pub(crate) fn status_path(self, id: &str) -> String {
        match self {
            OpKind::Activation => format!("/activation/status/{}", id),
            OpKind::Update => format!("/update/status/{}", id),
        }
    }

    pub(crate) fn steps(self) -> ProgressSteps {
        match self {
            OpKind::Activation => ACTIVATION_STEPS,
            OpKind::Update => UPDATE_STEPS,
        }
    }

    /// Map the JSON `phase` string written by activate/update onto a step index.
    /// Terminal "complete*" phases land on the final "Finished" step.
    pub(crate) fn step_index(self, phase: &str) -> usize {
        match self {
            OpKind::Activation => match phase {
                "triggered" | "starting" => 0,
                "write-flake" | "write-flake-done" => 1,
                "toplevel-build" | "toplevel-built" => 2,
                "git-add" | "build-branch" | "build-commit" | "branches-created" | "amend-add"
                | "amend-commit" | "branch-failed" => 3,
                "pre-rebuild" | "rebuild-failed" => 4,
                "completed" | "completed-with-warnings" | "complete" => 5,
                _ => 0,
            },
            OpKind::Update => match phase {
                "triggered" | "starting" => 0,
                "flake init" | "post-init restore" => 1,
                "write-flake" => 2,
                "flake update" => 3,
                "migrate" => 4,
                "complete" => 5,
                _ => 0,
            },
        }
    }

    pub(crate) fn active_color(self) -> &'static str {
        match self {
            OpKind::Activation => "text-warning",
            OpKind::Update => "text-info",
        }
    }
}

/// Checkmark icon for completed timeline steps.
const ICON_DONE: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" fill="currentColor" class="h-5 w-5"><path fill-rule="evenodd" d="M10 18a8 8 0 100-16 8 8 0 000 16zm3.857-9.809a.75.75 0 00-1.214-.882l-3.483 4.79-1.88-1.88a.75.75 0 10-1.06 1.061l2.5 2.5a.75.75 0 001.137-.089l4-5.5z" clip-rule="evenodd" /></svg>"#;

/// Hollow circle for pending timeline steps.
const ICON_PENDING: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" fill="currentColor" class="h-5 w-5 opacity-30"><path fill-rule="evenodd" d="M10 18a8 8 0 100-16 8 8 0 000 16zm0-2a6 6 0 100-12 6 6 0 000 12z" clip-rule="evenodd" /></svg>"#;

/// Error X for the failed step.
const ICON_FAILED: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" fill="currentColor" class="h-5 w-5"><path fill-rule="evenodd" d="M10 18a8 8 0 100-16 8 8 0 000 16zM8.28 7.22a.75.75 0 00-1.06 1.06L8.94 10l-1.72 1.72a.75.75 0 101.06 1.06L10 11.06l1.72 1.72a.75.75 0 101.06-1.06L11.06 10l1.72-1.72a.75.75 0 00-1.06-1.06L10 8.94 8.28 7.22z" clip-rule="evenodd" /></svg>"#;

const ICON_SPINNER: &str = r#"<span class="loading loading-ring loading-sm"></span>"#;

/// Build a daisyUI horizontal timeline for the given step progress.
pub(crate) fn build_timeline_html(
    kind: OpKind,
    labels: &[&str],
    idx: usize,
    status: &str,
) -> String {
    let n = labels.len();
    let mut items = String::new();
    for (i, label) in labels.iter().enumerate() {
        let done = status == "success" || i < idx;
        let current = i == idx && status != "success";
        let failed = current && status == "failed";
        let running = current && status == "in_progress";

        let (icon, icon_cls, box_cls) = if failed {
            (ICON_FAILED, "text-error", "timeline-box border-error")
        } else if done {
            (ICON_DONE, "text-success", "timeline-box")
        } else if running {
            (
                ICON_SPINNER,
                kind.active_color(),
                "timeline-box border-current",
            )
        } else {
            (ICON_PENDING, "opacity-50", "timeline-box opacity-60")
        };

        // Connector only fills once this step is done (next bullet has started).
        // Do not color while the current step is still running — that looked
        // like progress had already advanced to the next step.
        let hr_after = if i + 1 < n {
            if done {
                r#"<hr class="bg-success"/>"#.to_string()
            } else {
                "<hr/>".to_string()
            }
        } else {
            String::new()
        };

        // Leading connector mirrors the previous step's trailing one.
        let hr_before = if i > 0 {
            let prev_done = status == "success" || i - 1 < idx;
            if prev_done {
                r#"<hr class="bg-success"/>"#.to_string()
            } else {
                "<hr/>".to_string()
            }
        } else {
            String::new()
        };

        // Active (running) step: wrap the label box in daisyUI aura so it
        // stands out as a rotating border light around the stage.
        //
        // The status strip is hx-swapped every 1s (outerHTML), which remounts
        // the node and would reset CSS animations. Negative animation-delay
        // phase-locks to wall clock so the ring continues mid-cycle across
        // remounts (daisyUI default aura period is 6s).
        // Poll with `every 1s` only — never `load` with outerHTML (each swap
        // remounts and would re-fire load → request storm).
        let end = if running {
            let phase_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() % 6000)
                .unwrap_or(0);
            format!(
                r#"<div class="timeline-end"><div class="aura aura-sm {color}" style="animation-delay:-{phase_ms}ms"><div class="{box_cls} bg-base-100 text-[10px] whitespace-nowrap">{label}</div></div></div>"#,
                color = kind.active_color(),
                phase_ms = phase_ms,
                box_cls = box_cls,
                label = label,
            )
        } else {
            format!(
                r#"<div class="timeline-end {box_cls} text-[10px] whitespace-nowrap">{label}</div>"#,
                box_cls = box_cls,
                label = label,
            )
        };

        items.push_str(&format!(
            r#"<li>{hr_before}<div class="timeline-middle {icon_cls}">{icon}</div>{end}{hr_after}</li>"#,
            hr_before = hr_before,
            icon_cls = icon_cls,
            icon = icon,
            end = end,
            hr_after = hr_after,
        ));
    }
    format!(
        r#"<ul class="timeline timeline-horizontal timeline-compact w-full justify-center overflow-x-auto">{items}</ul>"#,
        items = items
    )
}
