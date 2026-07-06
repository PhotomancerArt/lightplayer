use dioxus::prelude::*;
use lpa_studio_core::{ControllerId, ProjectEditorOp, UiAction};
use lpa_studio_web_story_macros::story;

use crate::app::node::node_story_fixtures::{
    error_node_view, failed_dirty_node_view, live_dirty_node_view, nested_dirty_node_view,
    playlist_node_view, unsaved_dirty_node_view,
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
    description = "D7 variant (a), unsaved: header-only yellow tint plus the unsaved chip (the live default)."
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
    description = "D7 variant (a), live-only: header-only blue tint plus the live chip (the live default)."
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
    description = "D7 variant (a), failed: the error wash dominates the header and the red failed chip joins the unsaved chip (the live default)."
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
    description = "Dirty bubbling: a dirty grandchild's counts show on its own chip and on both ancestors' chips, so a collapsed parent still reveals a dirty descendant."
)]
pub(crate) fn nested_dirty_children() -> Element {
    let mut view = nested_dirty_node_view();
    view.action = Some(story_focus_action());

    rsx! {
        NodePane { view, on_action: move |_| {} }
    }
}

#[story(
    description = "The merged node detail popup open: status content plus the per-kind dirty count sections."
)]
pub(crate) fn dirty_detail_popup() -> Element {
    let mut view = unsaved_dirty_node_view();
    view.header.dirty.transient = 1;

    rsx! {
        div { class: "tw:flex tw:min-h-[620px] tw:justify-end",
            NodeDetailPopover {
                header: view.header,
                initially_open: true,
            }
        }
    }
}
