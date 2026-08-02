//! Stories for the project pane states (one `StudioPane` for the whole
//! project card: name title, status chip, contextual actions, detail popup,
//! node-tree body).

use dioxus::prelude::*;
use lpa_studio_core::app::project::node::{add_node_menu, gate_add_node_menu};
use lpa_studio_core::{
    ControllerId, DirtySummary, LpFeature, ProjectController, ProjectNodeAddress,
    ProjectNodeStatusTone, ProjectNodeStatusView, ProjectOp, ProjectSlotAddress, ProjectSlotRoot,
    ProjectSyncPhase, SlotEditOp, SlotPath, UiAction, UiAttachTarget, UiPaneAction, UiPendingEdit,
    UiPendingEditKind, UiPendingEditPhase, UiStatus,
};
use lpa_studio_web_story_macros::story;

use crate::app::project::ProjectPane;
use crate::app::story_fixtures::project_editor_fixture;

#[story(
    description = "Clean project: the project name as title, 'Project' kind label, no chips, no header actions (adding lives in the node list's dashed 'Add node…' row), quiet 'i' detail trigger (the status word lives in the popup); the node tree is the whole pane body — no 'Node tree' heading and no Refresh/Disconnect strip (P6 sidebar tidy)."
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
    description = "Only live (transient) edits: blue header wash; no persisted edits, so no Save/Revert icons and a quiet 'i' trigger."
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

#[story(
    description = "The add-node kind picker pinned open on the node tree's 'Add node…' row: one flat popover (no submenu — the future source dimension grows here), one row per instantiable kind with its glyph; a row click dispatches the ready-made create at the project root and closes the picker. No name field — nodes auto-name."
)]
pub(crate) fn add_node_picker() -> Element {
    rsx! {
        div { class: "tw:min-h-[520px]",
            StoryPane {
                dirty: DirtySummary::default(),
                edits_in_flight: 0,
                actions: false,
                add_picker_open: true,
            }
        }
    }
}

/// The build of a device that carries no fluid and no radio runtime — the
/// shape the picker and the tree are gated against.
fn gapped_device_features() -> Vec<LpFeature> {
    vec![
        LpFeature::NodeButton,
        LpFeature::NodeClock,
        LpFeature::NodeFixture,
        LpFeature::NodePlaylist,
        LpFeature::NodeShader,
        LpFeature::NodeTexture,
        LpFeature::GfxLpvm,
        LpFeature::SvcButton,
    ]
}

#[story(
    description = "The add-node picker against a device whose firmware lacks the fluid and radio runtimes: those rows are DISABLED and annotated 'Not on this device' — never hidden, because a picker that drops entries teaches a false catalog. G1 question 4: is the disabled treatment plus the annotation copy right?"
)]
pub(crate) fn add_node_picker_device_gaps() -> Element {
    let mut view = project_editor_fixture(ProjectSyncPhase::Ready);
    let mut menu = add_node_menu(&UiAttachTarget::ProjectRoot);
    gate_add_node_menu(&mut menu, Some(&gapped_device_features()));
    view.add_node_menu = Some(menu);

    rsx! {
        div { class: "tw:min-h-[520px] tw:max-w-[320px]",
            ProjectPane {
                view,
                status: UiStatus::good("Ready"),
                on_action: move |_| {},
                add_picker_initially_open: true,
            }
        }
    }
}

#[story(
    description = "A project holding a node whose kind this device's firmware does not carry (Clock — a gateable kind; Output is ungated and could never be unsupported). The tree row announces itself with the ordinary attention indicator, no special dimming: rows carry affordances, and the words — 'Not on this device' plus the engine's reason — live in the tooltip and the node's popover. Loud on purpose; an unsupported node usually breaks the show on this device."
)]
pub(crate) fn unsupported_node_in_tree() -> Element {
    let mut view = project_editor_fixture(ProjectSyncPhase::Ready);
    let unsupported = ProjectNodeStatusView::new(
        "Not on this device",
        Some("node kind Clock is not included in this firmware build".to_string()),
        ProjectNodeStatusTone::Disabled,
    );
    // The CLOCK row: a gateable kind. Deliberately not `Output`, which is
    // ungated in the engine and can never be unsupported — a story that
    // showed it would be teaching something untrue.
    if let Some(root) = view.tree.roots.first_mut()
        && let Some(child) = root.children.first_mut()
    {
        child.status = unsupported;
    }

    rsx! {
        div { class: "tw:max-w-[320px]",
            ProjectPane {
                view,
                status: UiStatus::good("Ready"),
                on_action: move |_| {},
            }
        }
    }
}

#[story(
    description = "A freshly created (or emptied) project's pane: the synced-empty state is not a waiting message but the dashed 'Add node…' row — the add affordance lives in the node list, where nodes live (no title-bar '+')."
)]
pub(crate) fn empty_project() -> Element {
    let mut view = project_editor_fixture(ProjectSyncPhase::Ready);
    view.tree.roots = Vec::new();
    view.nodes = Vec::new();
    view.header_actions = Vec::new();
    view.add_node_menu = Some(add_node_menu(&UiAttachTarget::ProjectRoot));

    rsx! {
        div { class: "tw:max-w-[320px]",
            ProjectPane {
                view,
                status: UiStatus::good("Ready"),
                on_action: move |_| {},
            }
        }
    }
}

#[story(
    description = "A staged node removal in the save panel: the NodeRemoved row (removed node's name, its attachment site, 'Restore' revert) plus the staged file deletions as 'file deleted on save' rows with their own reverts — all in the unsaved bucket until save."
)]
pub(crate) fn staged_node_removal() -> Element {
    rsx! {
        div { class: "tw:min-h-[640px]",
            StoryPane {
                dirty: DirtySummary {
                    persisted: 3,
                    transient: 0,
                    failed: 0,
                },
                edits_in_flight: 0,
                actions: true,
                initially_open: true,
                pending_edits: vec![
                    pending_edit(
                        "Orbit shader",
                        "nodes[orbit]",
                        UiPendingEditKind::NodeRemoved,
                        UiPendingEditPhase::Persisted,
                    ),
                    file_deletion_edit("Orbit shader", "/orbit.json"),
                    file_deletion_edit("Orbit shader", "/orbit.glsl"),
                ],
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
    #[props(default = false)] add_picker_open: bool,
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
    // Mirror the controller (review round): no header add action — the
    // picker data rides the view and renders as the tree's add row.
    view.add_node_menu = Some(add_node_menu(&UiAttachTarget::ProjectRoot));

    rsx! {
        div { class: "tw:max-w-[320px]",
            ProjectPane {
                view,
                status: UiStatus::good("Ready"),
                on_action: move |_| {},
                initially_open,
                add_picker_initially_open: add_picker_open,
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
        ProjectNodeAddress::parse("/demo.module/orbit.shader").expect("valid story node address"),
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

/// One staged file-deletion row, as a node removal stages it: an
/// `AssetBody`-kind file row (path display = the artifact path) whose detail
/// reads "deleted"; its revert cancels the staged deletion.
fn file_deletion_edit(node_label: &str, file_path: &str) -> UiPendingEdit {
    let mut edit = pending_edit(
        node_label,
        "nodes[orbit]",
        UiPendingEditKind::AssetBody {
            detail: "deleted".to_string(),
        },
        UiPendingEditPhase::Persisted,
    );
    // File rows carry the artifact path where slot rows carry the slot path.
    edit.slot_path_display = file_path.to_string();
    edit
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
