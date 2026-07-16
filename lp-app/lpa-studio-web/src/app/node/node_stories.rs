use dioxus::prelude::*;
use lpa_studio_core::{ControllerId, ProjectEditorOp, UiAction};
use lpa_studio_web_story_macros::story;

use crate::app::node::node_story_fixtures::{
    error_node_view, failed_dirty_node_view, live_dirty_node_view, nested_dirty_node_view,
    playlist_node_view, playlist_pending_edits, unsaved_dirty_node_view,
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
    description = "D7 variant (a), live-only: header-only blue tint with the blue (live) pencil detail trigger (the live default)."
)]
pub(crate) fn dirty_live_header_tint() -> Element {
    let mut view = live_dirty_node_view();
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
    description = "D7 variant (b), live-only: the blue tint re-mixed into the whole pane surface."
)]
pub(crate) fn dirty_live_surface_tint() -> Element {
    let mut view = live_dirty_node_view();
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
    let mut view = unsaved_dirty_node_view();
    view.header.dirty.transient = 1;

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
