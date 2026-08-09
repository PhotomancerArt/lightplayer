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
//! `dimensionality` (the producer-side space section — G1b ruling 1 moved
//! it here from below settings, between `code` and `advanced`) +
//! `advanced`; fixture/playlist = `advanced` only (the fixture face keeps
//! its own below-settings space drawer, ruled good as shipped). The
//! advanced drawer hosts today's generic slot-row sections unchanged —
//! with ONE exception: the **Debug** section is lifted out and rendered
//! permanently above the drawers. D8 tier (c) requires debug territory to
//! be visible even when idle, and a section behind a closed lid is not
//! visible.

use dioxus::prelude::*;
use lpa_studio_core::{
    NodeCardDrawer, NodeUiOp, UiAction, UiAssetEditor as UiAssetEditorData, UiNodeSection,
    UiPendingEdit, UiSpaceCellRole, UiSpaceSection,
};

use crate::app::node::{AssetEditor, NodeDirtyTint, NodeSection};
use crate::base::Platform;

use super::space_section::{SPACE_SECTION_LABEL, SpaceSection, space_section_summary};
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
    /// The shader face's producer-side space section for the
    /// `dimensionality` drawer, between `code` and `advanced` (G1b ruling
    /// 1); `None` omits the drawer (every non-shader face — the fixture's
    /// consumer section stays on its own face, ruled good as shipped).
    #[props(default = None)]
    space: Option<UiSpaceSection>,
    /// Sections for the `advanced` drawer — today's generic slot-row view.
    sections: Vec<UiNodeSection>,
    /// Whether the code drawer is expanded (core-owned state).
    #[props(default = false)]
    code_open: bool,
    /// Whether the dimensionality drawer is expanded (core-owned state,
    /// `NodeCardUiState::space_open`). A D1 mismatch or an open picker
    /// forces the drawer open regardless.
    #[props(default = false)]
    space_open: bool,
    /// Open this space cell's tile picker on first render (stories).
    #[props(default = None)]
    space_picker_open_cell: Option<UiSpaceCellRole>,
    /// Whether the advanced drawer is expanded (core-owned state).
    #[props(default = false)]
    advanced_open: bool,
    /// Whether the Debug section's rows are expanded (core-owned state).
    #[props(default = false)]
    debug_open: bool,
    /// Platform for the code editor's shortcut hints; stories pin it for
    /// deterministic captures.
    #[props(default = None)]
    platform: Option<Platform>,
    #[props(default)] pending_edits: Vec<UiPendingEdit>,
    #[props(default)] dirty_tint: NodeDirtyTint,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
) -> Element {
    // The Debug section never hides behind the advanced lid (D8 tier c).
    let (debug_sections, drawer_sections): (Vec<_>, Vec<_>) = sections
        .into_iter()
        .partition(|section| matches!(section, UiNodeSection::DebugSlots(_)));
    let has_advanced = !drawer_sections.is_empty();
    let code_summary = code.as_ref().map(|editor| editor.source.clone());
    let code_node = node.clone();
    let space_node = node.clone();
    let section_node = Some(node.clone());
    let advanced_node = node;
    // The dimensionality drawer opens on its core-owned bit — and is FORCED
    // open by a D1 mismatch (an error folded away is an error hidden) or an
    // open tile picker (a popover anchored inside a closed lid is nothing).
    let space_summary = space.as_ref().map(space_section_summary);
    let space_forced = space
        .as_ref()
        .is_some_and(|section| section.mismatch.is_some() || space_picker_open_cell.is_some());
    let space_effective_open = space_open || space_forced;

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
        if let Some(section) = space {
            NodeCardSection {
                label: SPACE_SECTION_LABEL,
                summary: space_summary,
                open: Some(space_effective_open),
                on_toggle: move |()| dispatch_drawer(
                    on_action,
                    &space_node,
                    NodeCardDrawer::Space,
                    !space_effective_open,
                ),
                SpaceSection {
                    section,
                    picker_open_cell: space_picker_open_cell,
                    on_action,
                }
            }
        }
        for section in debug_sections {
            NodeSection {
                section,
                node: section_node.clone(),
                debug_open,
                on_action,
                pending_edits: pending_edits.clone(),
                dirty_tint,
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
                for (index, section) in drawer_sections.clone().into_iter().enumerate() {
                    NodeSection {
                        section,
                        first: index == 0,
                        node: section_node.clone(),
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
