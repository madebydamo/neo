//! Background operation kinds (activation / update / store repair / generation switch):
//! titles and the ordered progress steps the monitor shows. The kind itself is
//! [`OperationKind`], shared with the CLI.

use crate::utils::ops::OperationKind;

/// Ordered UI steps for activation / update. Last label is always "Finished" so the
/// final working phase is never shown as complete while it still runs.
const ACTIVATION_STEPS: &[&str] = &[
    "Start",
    "Write flake",
    "Build",
    "Save checkpoints",
    "Switch",
    "Finished",
];

const RESCUE_STEPS: &[&str] = &["Start", "Prepare", "Config repo", "Restore", "Finished"];

const UPDATE_STEPS: &[&str] = &[
    "Start",
    "Reinitialize",
    "Write flake",
    "Update dependencies",
    "Migrate",
    "Finished",
];

impl OperationKind {
    pub(crate) fn title(self) -> &'static str {
        match self {
            OperationKind::Activation => "Activation",
            OperationKind::Update => "Update",
            OperationKind::Repair => "Nix store repair",
            OperationKind::Generation => "Generation switch",
            OperationKind::Rescue => "Restore to a new server",
        }
    }

    /// Progress steps; empty for kinds that only report a free-form phase.
    pub(crate) fn steps(self) -> &'static [&'static str] {
        match self {
            OperationKind::Activation => ACTIVATION_STEPS,
            OperationKind::Update => UPDATE_STEPS,
            OperationKind::Rescue => RESCUE_STEPS,
            OperationKind::Repair | OperationKind::Generation => &[],
        }
    }

    /// Map the JSON `phase` string written by activate/update onto a step index.
    /// Terminal "complete*" phases land on the final "Finished" step.
    pub(crate) fn step_index(self, phase: &str) -> usize {
        match self {
            OperationKind::Activation => match phase {
                "triggered" | "starting" => 0,
                "write-flake" | "write-flake-done" => 1,
                "toplevel-build" | "toplevel-built" => 2,
                "git-add" | "build-branch" | "build-commit" | "branches-created" | "amend-add"
                | "amend-commit" | "branch-failed" => 3,
                "pre-rebuild" | "rebuild-failed" => 4,
                "completed" | "completed-with-warnings" | "complete" => 5,
                _ => 0,
            },
            OperationKind::Update => match phase {
                "triggered" | "starting" => 0,
                "flake init" | "post-init restore" => 1,
                "write-flake" => 2,
                "flake update" => 3,
                "migrate" => 4,
                "complete" => 5,
                _ => 0,
            },
            OperationKind::Rescue => match phase {
                "triggered" | "starting" | "start" => 0,
                "prepare" => 1,
                "config" => 2,
                "restore" => 3,
                "complete" => 4,
                _ => 0,
            },
            OperationKind::Repair | OperationKind::Generation => 0,
        }
    }
}

/// Timestamp part of an op id (after the kind prefix), for the dialog subtitle.
pub(crate) fn id_timestamp(id: &str) -> &str {
    id.split_once('_').map(|(_, ts)| ts).unwrap_or(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_from_ids() {
        assert_eq!(
            OperationKind::from_id("activation_20260928-1"),
            Some(OperationKind::Activation)
        );
        assert_eq!(
            OperationKind::from_id("update_x"),
            Some(OperationKind::Update)
        );
        assert_eq!(
            OperationKind::from_id("repair_x"),
            Some(OperationKind::Repair)
        );
        assert_eq!(
            OperationKind::from_id("genswitch_switch-3-x"),
            Some(OperationKind::Generation)
        );
        assert_eq!(OperationKind::from_id("nope"), None);
        assert_eq!(id_timestamp("genswitch_switch-3-x"), "switch-3-x");
    }

    #[test]
    fn terminal_phase_is_last_step() {
        let k = OperationKind::Activation;
        assert_eq!(k.step_index("completed"), k.steps().len() - 1);
        let k = OperationKind::Update;
        assert_eq!(k.step_index("complete"), k.steps().len() - 1);
    }
}
