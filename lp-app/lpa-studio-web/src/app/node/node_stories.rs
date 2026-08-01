use dioxus::prelude::*;
use lpa_studio_core::{ControllerId, ProjectEditorOp, UiAction};
use lpa_studio_web_story_macros::story;

use crate::app::node::node_story_fixtures::{
    clock_node_view, error_node_view, failed_dirty_node_view, nested_dirty_node_view,
    node_delete_pane_action, playlist_node_view, playlist_pending_edits, unsaved_dirty_node_view,
};
use crate::app::node::{NodeDetailPopover, NodeDirtyTint, NodePane};

/// Story stand-in for the controller-built focus action so panes render the
/// header select control.
fn story_focus_action() -> UiAction {
    UiAction::from_op(ControllerId::new("story.project"), ProjectEditorOp::Focus)
}

#[story(description = "A composed node pane showing the current node anatomy direction.")]
pub(crate) fn node_pane() -> Element {
    let mut view = playlist_node_view();
    view.action = Some(story_focus_action());

    rsx! {
        NodePane { view, on_action: move |_| {} }
    }
}

#[story(
    description = "A selected node pane collapsed down to its header: accent border and active select control."
)]
pub(crate) fn collapsed_node_pane() -> Element {
    let mut view = playlist_node_view();
    view.action = Some(story_focus_action());
    view.focused = true;
    view.collapsed = true;

    rsx! {
        NodePane { view, on_action: move |_| {} }
    }
}

#[story(description = "Node pane with an error status and projection issues.")]
pub(crate) fn error_node() -> Element {
    rsx! {
        NodePane { view: error_node_view() }
    }
}

#[story(
    description = "Node pane header with the always-available delete action (Trash2, generic pane-action path): pressing it runs the confirmation composed core-side from the removal pre-flight — dependents count, pending edits the sweep discards, and the files staged for deletion on save."
)]
pub(crate) fn header_delete_action() -> Element {
    let mut view = playlist_node_view();
    view.action = Some(story_focus_action());
    view.header_actions = vec![node_delete_pane_action()];

    rsx! {
        NodePane { view, on_action: move |_| {} }
    }
}

#[story(
    description = "D7 variant (a), unsaved: header-only yellow tint; the yellow edit-pencil detail trigger is the whole announcement (no count chips — counts live in the popup)."
)]
pub(crate) fn dirty_unsaved_header_tint() -> Element {
    let mut view = unsaved_dirty_node_view();
    view.action = Some(story_focus_action());

    rsx! {
        NodePane {
            view,
            on_action: move |_| {},
            dirty_tint: NodeDirtyTint::HeaderOnly,
        }
    }
}

#[story(
    description = "D7 variant (b), unsaved: the yellow tint re-mixed into the whole pane surface."
)]
pub(crate) fn dirty_unsaved_surface_tint() -> Element {
    let mut view = unsaved_dirty_node_view();
    view.action = Some(story_focus_action());

    rsx! {
        NodePane {
            view,
            on_action: move |_| {},
            dirty_tint: NodeDirtyTint::FullSurface,
        }
    }
}

#[story(
    description = "D7 variant (a), failed: the error wash dominates the header and the detail trigger wears the red warning glyph (the live default)."
)]
pub(crate) fn dirty_failed_header_tint() -> Element {
    let mut view = failed_dirty_node_view();
    view.action = Some(story_focus_action());

    rsx! {
        NodePane {
            view,
            on_action: move |_| {},
            dirty_tint: NodeDirtyTint::HeaderOnly,
        }
    }
}

#[story(
    description = "D7 variant (b), failed: the error tint re-mixed into the whole pane surface."
)]
pub(crate) fn dirty_failed_surface_tint() -> Element {
    let mut view = failed_dirty_node_view();
    view.action = Some(story_focus_action());

    rsx! {
        NodePane {
            view,
            on_action: move |_| {},
            dirty_tint: NodeDirtyTint::FullSurface,
        }
    }
}

#[story(
    description = "Dirty bubbling: a dirty grandchild's affordance shows on its own detail trigger and on both ancestors' triggers (with the header tint), so a collapsed parent still reveals a dirty descendant; the clean sibling stays silent."
)]
pub(crate) fn nested_dirty_children() -> Element {
    let mut view = nested_dirty_node_view();
    view.action = Some(story_focus_action());

    rsx! {
        NodePane { view, on_action: move |_| {} }
    }
}

#[story(
    description = "The live default: a Clock card whose three `controls.*` fields are Debug-role. The section is COLLAPSED — most of the time those controls are not wanted — but its header is always debug territory: hazard-striped, labelled DEBUG, reading \"session only\". Nothing is overridden, so there is no count, no Clear, and no card marking. The persisted rows sit above under `Settings`."
)]
pub(crate) fn debug_section_idle() -> Element {
    rsx! {
        NodePane { view: clock_node_view(0, false), on_action: move |_| {} }
    }
}

#[story(
    description = "Collapsed with two active overrides: the header reads \"2 active · session only\" and offers Clear WITHOUT expanding, and the card header carries the `debug 2` marking. The header box is the same height as the idle story's — the count and the Clear button are reserved space, so touching a control never reflows the card."
)]
pub(crate) fn debug_section_collapsed_active() -> Element {
    rsx! {
        NodePane { view: clock_node_view(2, false), on_action: move |_| {} }
    }
}

#[story(
    description = "Expanded with two active overrides: the flattened Debug rows (Running / Rate / Scrub offset seconds — no nested `Controls` group), the touched ones wearing the hazard row tint with the inline Clear verb. The header wash stays neutral on purpose — a debug override is NOT unsaved work (D7), so it never borrows the amber dirty treatment."
)]
pub(crate) fn debug_section_active() -> Element {
    rsx! {
        NodePane { view: clock_node_view(2, true), on_action: move |_| {} }
    }
}

#[story(
    description = "Expanded and idle: what the disclosure reveals before anything is touched — three transient controls, each already reading as debug territory. This is the clean-transient case D8c exists for."
)]
pub(crate) fn debug_section_expanded_idle() -> Element {
    rsx! {
        NodePane { view: clock_node_view(0, true), on_action: move |_| {} }
    }
}

#[story(
    description = "The hazard family beside its neighbours, for the G1 distinctness question: collapsed-idle, collapsed-active, and expanded-active Debug sections next to an amber-unsaved card. Debug = attention-orange + diagonal stripes; flat orange stays device health; amber stays unsaved. The three debug cards also show the no-reflow contract — every header strip is the same height."
)]
pub(crate) fn debug_section_vs_unsaved() -> Element {
    let mut unsaved = unsaved_dirty_node_view();
    unsaved.action = Some(story_focus_action());

    rsx! {
        div { class: "tw:grid tw:gap-4",
            NodePane { view: clock_node_view(0, false), on_action: move |_| {} }
            NodePane { view: clock_node_view(2, false), on_action: move |_| {} }
            NodePane { view: clock_node_view(2, true), on_action: move |_| {} }
            NodePane { view: unsaved, on_action: move |_| {} }
        }
    }
}

#[story(
    description = "The node detail popup on an erroring node: the status pill plus the runtime's error text — the popup answers WHY a node is in the error state (the compact status alone doesn't)."
)]
pub(crate) fn error_detail_popup() -> Element {
    let view = error_node_view();

    rsx! {
        div { class: "tw:flex tw:min-h-[320px] tw:justify-end",
            NodeDetailPopover {
                header: view.header,
                pending_edits: vec![],
                on_action: move |_| {},
                initially_open: true,
            }
        }
    }
}

#[story(
    description = "The merged node detail popup open: status content plus the per-bucket dirty sections as tinted-title change lists — the node's OWN pending edits with per-entry reverts (subtree counts ride the title rows; the other node's edit in the threaded list is filtered out)."
)]
pub(crate) fn dirty_detail_popup() -> Element {
    let view = unsaved_dirty_node_view();

    rsx! {
        div { class: "tw:flex tw:min-h-[620px] tw:justify-end",
            NodeDetailPopover {
                header: view.header,
                pending_edits: playlist_pending_edits(),
                on_action: move |_| {},
                initially_open: true,
            }
        }
    }
}
