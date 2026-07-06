//! Stories for the project header states (the dissolved save strip).

use dioxus::prelude::*;
use lpa_studio_core::{
    ControllerId, DirtySummary, ProjectController, ProjectOp, UiAction, UiPaneAction,
};
use lpa_studio_web_story_macros::story;

use crate::app::project::ProjectHeader;

#[story(
    description = "Clean project: neutral header, visible 'unchanged' chip, no action icons, quiet detail trigger."
)]
pub(crate) fn unchanged() -> Element {
    rsx! {
        ProjectHeader {
            project_id: "studio-demo",
            dirty: DirtySummary::default(),
            overlay_revision: 0,
            edits_in_flight: 0,
            actions: Vec::<UiPaneAction>::new(),
            on_action: move |_| {},
        }
    }
}

#[story(
    description = "Pending persisted edits: yellow header wash, unsaved/live chips, contextual Save and Revert icons."
)]
pub(crate) fn uncommitted() -> Element {
    rsx! {
        ProjectHeader {
            project_id: "studio-demo",
            dirty: DirtySummary {
                persisted: 2,
                transient: 1,
                failed: 0,
            },
            overlay_revision: 7,
            edits_in_flight: 0,
            actions: header_actions(),
            on_action: move |_| {},
        }
    }
}

#[story(
    description = "Only live (transient) edits: blue header wash and live chip; no persisted edits, so no action icons."
)]
pub(crate) fn live_only() -> Element {
    rsx! {
        ProjectHeader {
            project_id: "studio-demo",
            dirty: DirtySummary {
                persisted: 0,
                transient: 2,
                failed: 0,
            },
            overlay_revision: 4,
            edits_in_flight: 0,
            actions: Vec::<UiPaneAction>::new(),
            on_action: move |_| {},
        }
    }
}

#[story(
    description = "An edit awaiting its server ack: working wash, syncing chip ahead of the unsaved count, in-progress detail trigger."
)]
pub(crate) fn in_progress() -> Element {
    rsx! {
        ProjectHeader {
            project_id: "studio-demo",
            dirty: DirtySummary {
                persisted: 1,
                transient: 0,
                failed: 0,
            },
            overlay_revision: 7,
            edits_in_flight: 1,
            actions: header_actions(),
            on_action: move |_| {},
        }
    }
}

#[story(
    description = "The detail popup: state, overlay revision, and per-kind sections — warning tint for unsaved, live tint for transient."
)]
pub(crate) fn detail_popup() -> Element {
    rsx! {
        div { class: "tw:min-h-80",
            ProjectHeader {
                project_id: "studio-demo",
                dirty: DirtySummary {
                    persisted: 2,
                    transient: 1,
                    failed: 0,
                },
                overlay_revision: 7,
                edits_in_flight: 0,
                actions: header_actions(),
                on_action: move |_| {},
                initially_open: true,
            }
        }
    }
}

/// The same Save / Revert-to-saved pair the project controller produces while
/// persisted edits are pending.
fn header_actions() -> Vec<UiPaneAction> {
    vec![
        UiPaneAction::new("save", project_action(ProjectOp::SaveOverlay)),
        UiPaneAction::new(
            "revert",
            project_action(ProjectOp::RevertAllEdits).with_label("Revert to saved"),
        ),
    ]
}

fn project_action(op: ProjectOp) -> UiAction {
    UiAction::from_op(ControllerId::new(ProjectController::NODE_ID), op)
}
