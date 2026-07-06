//! Stories for the project pane states (one `StudioPane` for the whole
//! project card: name title, status chip, contextual actions, detail popup,
//! node-tree body).

use dioxus::prelude::*;
use lpa_studio_core::{
    ControllerId, DirtySummary, ProjectController, ProjectOp, ProjectSyncPhase, UiAction,
    UiPaneAction, UiStatus,
};
use lpa_studio_web_story_macros::story;

use crate::app::project::ProjectPane;
use crate::app::story_fixtures::{project_editor_fixture, project_ready_actions};

#[story(
    description = "Clean project: the project name as title, 'Project' kind label, no chips and no action icons, quiet 'i' detail trigger (the status word lives in the popup); node tree as the pane body with pane actions at its foot."
)]
pub(crate) fn unchanged() -> Element {
    rsx! {
        StoryPane {
            dirty: DirtySummary::default(),
            edits_in_flight: 0,
            actions: false,
        }
    }
}

#[story(
    description = "Pending persisted edits: yellow header wash, contextual Save and Revert icons, edited detail trigger — no count chips in the header (counts live in the popup)."
)]
pub(crate) fn uncommitted() -> Element {
    rsx! {
        StoryPane {
            dirty: DirtySummary {
                persisted: 2,
                transient: 1,
                failed: 0,
            },
            edits_in_flight: 0,
            actions: true,
        }
    }
}

#[story(
    description = "Only live (transient) edits: blue header wash; no persisted edits, so no action icons and a quiet 'i' trigger."
)]
pub(crate) fn live_only() -> Element {
    rsx! {
        StoryPane {
            dirty: DirtySummary {
                persisted: 0,
                transient: 2,
                failed: 0,
            },
            edits_in_flight: 0,
            actions: false,
        }
    }
}

#[story(
    description = "An edit awaiting its server ack while persisted edits are pending: Unsaved outranks Busy in the shared priority, so the pencil trigger and yellow wash win; the awaiting-ack count is in the popup."
)]
pub(crate) fn in_progress() -> Element {
    rsx! {
        StoryPane {
            dirty: DirtySummary {
                persisted: 1,
                transient: 0,
                failed: 0,
            },
            edits_in_flight: 1,
            actions: true,
        }
    }
}

#[story(
    description = "The detail popup: project identity with the status pill, state, overlay revision, per-kind dirty counts with their tints, and the project stats section (moved from the old sidebar card)."
)]
pub(crate) fn detail_popup() -> Element {
    rsx! {
        div { class: "tw:min-h-[480px]",
            StoryPane {
                dirty: DirtySummary {
                    persisted: 2,
                    transient: 1,
                    failed: 0,
                },
                edits_in_flight: 0,
                actions: true,
                initially_open: true,
            }
        }
    }
}

/// One project pane at sidebar width over the shared synced-project fixture.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn StoryPane(
    dirty: DirtySummary,
    edits_in_flight: usize,
    actions: bool,
    #[props(default = false)] initially_open: bool,
) -> Element {
    let mut view = project_editor_fixture(ProjectSyncPhase::Ready);
    view.dirty = dirty;
    view.edits_in_flight = edits_in_flight;
    view.header_actions = if actions {
        header_actions()
    } else {
        Vec::new()
    };

    rsx! {
        div { class: "tw:max-w-[320px]",
            ProjectPane {
                view,
                status: UiStatus::good("Ready"),
                pane_actions: project_ready_actions(),
                on_action: move |_| {},
                initially_open,
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
