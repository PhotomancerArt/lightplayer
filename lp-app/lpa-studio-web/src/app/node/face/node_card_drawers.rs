//! Expandable drawers under a node card's permanent face.
//!
//! The disclosure grammar settled by the spike, recast on
//! [`NodeCardSection`] (P2b item 1): a collapsed drawer is the section
//! grammar's slim horizontal row (chevron + label + summary hint); an
//! expanded drawer is a full section wearing the left-edge label rail,
//! expanding DOWNWARD under the face — the face never moves. Open state
//! is CORE-OWNED (`NodeCardUiState`, the node arm of the CardUiState
//! re-home): the drawers render the state the DTO carries and dispatch
//! `NodeUiOp::SetDrawer` on toggle, so disclosure survives re-renders and
//! is e2e-drivable. Stories seed the DTO (`UiNodeView::card_ui`) instead
//! of component props.
//!
//! Drawers used today: shader = `code` (the existing [`AssetEditor`]) +
//! `advanced`; fixture/playlist = `advanced` only. The advanced drawer
//! hosts today's generic slot-row sections unchanged.

use dioxus::prelude::*;
use lpa_studio_core::{
    NodeCardDrawer, NodeUiOp, UiAction, UiAssetEditor as UiAssetEditorData, UiNodeSection,
    UiPendingEdit,
};

use crate::app::node::{AssetEditor, NodeDirtyTint, NodeSection};
use crate::base::Platform;

use super::{NodeCardSection, node_ui_action};

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn NodeCardDrawers(
    /// The node's address path — the card UI state key the toggle ops
    /// carry.
    node: String,
    /// Inline GLSL editor content for the `code` drawer; `None` omits the
    /// drawer entirely (fixture/playlist faces).
    #[props(default = None)]
    code: Option<UiAssetEditorData>,
    /// Sections for the `advanced` drawer — today's generic slot-row view.
    sections: Vec<UiNodeSection>,
    /// Whether the code drawer is expanded (core-owned state).
    #[props(default = false)]
    code_open: bool,
    /// Whether the advanced drawer is expanded (core-owned state).
    #[props(default = false)]
    advanced_open: bool,
    /// Platform for the code editor's shortcut hints; stories pin it for
    /// deterministic captures.
    #[props(default = None)]
    platform: Option<Platform>,
    #[props(default)] pending_edits: Vec<UiPendingEdit>,
    #[props(default)] dirty_tint: NodeDirtyTint,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
) -> Element {
    let has_advanced = !sections.is_empty();
    let code_summary = code.as_ref().map(|editor| editor.source.clone());
    let code_node = node.clone();
    let advanced_node = node;

    rsx! {
        if let Some(editor) = code {
            NodeCardSection {
                label: "code",
                summary: code_summary,
                open: Some(code_open),
                on_toggle: move |()| dispatch_drawer(
                    on_action,
                    &code_node,
                    NodeCardDrawer::Code,
                    !code_open,
                ),
                AssetEditor { editor, on_action, platform }
            }
        }
        if has_advanced {
            NodeCardSection {
                label: "advanced",
                summary: Some("slots · bindings".to_string()),
                open: Some(advanced_open),
                on_toggle: move |()| dispatch_drawer(
                    on_action,
                    &advanced_node,
                    NodeCardDrawer::Advanced,
                    !advanced_open,
                ),
                for (index, section) in sections.clone().into_iter().enumerate() {
                    NodeSection {
                        section,
                        first: index == 0,
                        on_action,
                        pending_edits: pending_edits.clone(),
                        dirty_tint,
                    }
                }
            }
        }
    }
}

/// Dispatch one drawer toggle through core (`ProjectEditorOp::NodeUi`),
/// keyed by the node's address.
fn dispatch_drawer(
    on_action: Option<EventHandler<UiAction>>,
    node: &str,
    drawer: NodeCardDrawer,
    open: bool,
) {
    if let Some(handler) = on_action {
        handler.call(node_ui_action(NodeUiOp::SetDrawer {
            node: node.to_string(),
            drawer,
            open,
        }));
    }
}
