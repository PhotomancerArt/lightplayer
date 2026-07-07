//! Stories for the project pane states (one `StudioPane` for the whole
//! project card: name title, status chip, contextual actions, detail popup,
//! node-tree body).

use dioxus::prelude::*;
use lpa_studio_core::{
    ControllerId, DirtySummary, ProjectController, ProjectNodeAddress, ProjectOp,
    ProjectSlotAddress, ProjectSlotRoot, ProjectSyncPhase, SlotEditOp, SlotPath, UiAction,
    UiPaneAction, UiPendingEdit, UiPendingEditKind, UiPendingEditPhase, UiStatus,
};
use lpa_studio_web_story_macros::story;

use crate::app::project::ProjectPane;
use crate::app::story_fixtures::project_editor_fixture;

#[story(
    description = "Clean project: the project name as title, 'Project' kind label, no chips and no action icons, quiet 'i' detail trigger (the status word lives in the popup); the node tree is the whole pane body — no 'Node tree' heading and no Refresh/Disconnect strip (P6 sidebar tidy)."
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
    description = "The detail popup as the save panel: identity with the status pill, state, overlay revision, and the per-bucket sections as headed change lists (counts in the headers, node label + path + op/value + revert per row), plus the project stats section."
)]
pub(crate) fn detail_popup() -> Element {
    rsx! {
        div { class: "tw:min-h-[560px]",
            StoryPane {
                dirty: DirtySummary {
                    persisted: 2,
                    transient: 1,
                    failed: 0,
                },
                edits_in_flight: 0,
                actions: true,
                initially_open: true,
                pending_edits: vec![
                    with_old_value(
                        assign_edit("Orbit shader", "brightness", "0.85", UiPendingEditPhase::Persisted),
                        "0.5",
                    ),
                    pending_edit(
                        "Sunrise palette",
                        "mapping.PathPoints.paths[0]",
                        UiPendingEditKind::Added,
                        UiPendingEditPhase::Persisted,
                    ),
                    assign_edit("Orbit shader", "controls.rate", "2.0", UiPendingEditPhase::Live),
                ],
            }
        }
    }
}

#[story(
    description = "A mixed change list in the save panel: value assigns (old → new where the saved value is known), a structural add and remove (the remove with its replaced value), a live control, and a failed entry with its reason in the error-tinted section — every row with its own revert."
)]
pub(crate) fn change_list() -> Element {
    rsx! {
        div { class: "tw:min-h-[640px]",
            StoryPane {
                dirty: DirtySummary {
                    persisted: 3,
                    transient: 1,
                    failed: 1,
                },
                edits_in_flight: 0,
                actions: true,
                initially_open: true,
                pending_edits: vec![
                    with_old_value(
                        assign_edit("Orbit shader", "brightness", "0.85", UiPendingEditPhase::Persisted),
                        "0.5",
                    ),
                    pending_edit(
                        "Sunrise palette",
                        "mapping.PathPoints.paths[0]",
                        UiPendingEditKind::Added,
                        UiPendingEditPhase::Persisted,
                    ),
                    with_old_value(
                        pending_edit(
                            "Sunrise palette",
                            "entries[stripe]",
                            UiPendingEditKind::Removed,
                            UiPendingEditPhase::Persisted,
                        ),
                        "{\"shader\":\"stripe.glsl\",\"duration\":2.0}",
                    ),
                    assign_edit("Orbit shader", "controls.rate", "2.0", UiPendingEditPhase::Live),
                    pending_edit(
                        "Sunrise palette",
                        "entries[ghost]",
                        UiPendingEditKind::Added,
                        UiPendingEditPhase::Failed {
                            reason: "entries[ghost] does not resolve".to_string(),
                        },
                    ),
                ],
            }
        }
    }
}

#[story(
    description = "The save panel's empty state: a clean project shows the count rows at zero with no list rows and no failed section."
)]
pub(crate) fn change_list_empty() -> Element {
    rsx! {
        div { class: "tw:min-h-[480px]",
            StoryPane {
                dirty: DirtySummary::default(),
                edits_in_flight: 0,
                actions: false,
                initially_open: true,
            }
        }
    }
}

#[story(
    description = "A long change list stays inside the popover: the unsaved section's list caps its height and scrolls internally instead of growing the card."
)]
pub(crate) fn change_list_overflow() -> Element {
    let pending_edits = (0..14)
        .map(|index| {
            assign_edit(
                "Orbit shader",
                &format!("palette.stops[{index}]"),
                "(0.4, 0.2, 0.9)",
                UiPendingEditPhase::Persisted,
            )
        })
        .collect::<Vec<_>>();
    rsx! {
        div { class: "tw:min-h-[640px]",
            StoryPane {
                dirty: DirtySummary {
                    persisted: 14,
                    transient: 0,
                    failed: 0,
                },
                edits_in_flight: 0,
                actions: true,
                initially_open: true,
                pending_edits,
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
    #[props(default = Vec::new())] pending_edits: Vec<UiPendingEdit>,
) -> Element {
    let mut view = project_editor_fixture(ProjectSyncPhase::Ready);
    view.dirty = dirty;
    view.edits_in_flight = edits_in_flight;
    view.pending_edits = pending_edits;
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

/// One change-list entry with the same per-entry revert action the project
/// controller produces.
fn pending_edit(
    node_label: &str,
    path: &str,
    kind: UiPendingEditKind,
    phase: UiPendingEditPhase,
) -> UiPendingEdit {
    let address = ProjectSlotAddress::new(
        ProjectNodeAddress::parse("/demo.project/orbit.shader").expect("valid story node address"),
        ProjectSlotRoot::def(),
        SlotPath::parse(path).expect("valid story slot path"),
    );
    let node_path = address.node.to_string();
    UiPendingEdit {
        node_label: node_label.to_string(),
        node_path,
        slot_path_display: path.to_string(),
        kind,
        old_value: None,
        phase,
        revert: Some(UiAction::from_op(
            ControllerId::new(ProjectController::NODE_ID),
            SlotEditOp::Revert { address },
        )),
    }
}

/// Attach the saved (base) value an entry replaces, as the mirror's
/// base-value map would.
fn with_old_value(mut edit: UiPendingEdit, old_value: &str) -> UiPendingEdit {
    edit.old_value = Some(old_value.to_string());
    edit
}

fn assign_edit(
    node_label: &str,
    path: &str,
    value_display: &str,
    phase: UiPendingEditPhase,
) -> UiPendingEdit {
    pending_edit(
        node_label,
        path,
        UiPendingEditKind::Assign {
            value_display: value_display.to_string(),
        },
        phase,
    )
}
