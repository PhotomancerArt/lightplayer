use dioxus::prelude::*;
use lpa_studio_core::{UiAction, UiNodeChild, UiNodeView, UiPendingEdit};

use crate::app::node::{NodeDirtyTint, NodePane};

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn NodeChildren(
    items: Vec<UiNodeChild>,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
    /// The editor-level pending-edit list, threaded into every child pane
    /// (each pane's detail popover filters it to its own node).
    #[props(default)]
    pending_edits: Vec<UiPendingEdit>,
    #[props(default)] dirty_tint: NodeDirtyTint,
    /// M2 UX spike: panel gestures raised by a module child's face, passed
    /// down so a child module's panel is as live as its host's.
    #[props(default = None)]
    module_panel: Option<EventHandler<crate::app::module::PanelGesture>>,
) -> Element {
    rsx! {
        div { class: "tw:grid tw:min-w-0 tw:gap-3 tw:border-l tw:border-border-muted tw:pl-4",
            for child in items {
                NodePane {
                    key: "{child.label}",
                    view: child_node_view(child),
                    on_action,
                    pending_edits: pending_edits.clone(),
                    dirty_tint,
                    module_panel,
                }
            }
        }
    }
}

/// Promote an extracted child summary to a full pane view (nested cards are
/// the same card grammar) — the playlist's active child renders below its
/// playlist card through this same path (P2c item 2).
///
/// The mapping lives in core ([`UiNodeChild::into_node_view`]) so the
/// renderer and the core-side workspace scans promote a child card exactly
/// the same way.
fn child_node_view(child: UiNodeChild) -> UiNodeView {
    child.into_node_view()
}
