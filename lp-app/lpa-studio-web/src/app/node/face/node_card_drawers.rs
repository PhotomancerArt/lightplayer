//! Expandable drawers under a node card's permanent face.
//!
//! The disclosure grammar settled by the spike, recast on
//! [`NodeCardSection`] (P2b item 1): a collapsed drawer is the section
//! grammar's slim horizontal row (chevron + label + summary hint); an
//! expanded drawer is a full section wearing the left-edge label rail,
//! expanding DOWNWARD under the face — the face never moves. Open
//! state is view-local (`use_signal`, node-card Q4); the seed props exist
//! for stories and for a later CardUiState re-home.
//!
//! Drawers used today: shader = `code` (the existing [`AssetEditor`]) +
//! `advanced`; fixture/playlist = `advanced` only. The advanced drawer
//! hosts today's generic slot-row sections unchanged.

use dioxus::prelude::*;
use lpa_studio_core::{UiAction, UiAssetEditor as UiAssetEditorData, UiNodeSection, UiPendingEdit};

use crate::app::node::{AssetEditor, NodeDirtyTint, NodeSection};
use crate::base::Platform;

use super::NodeCardSection;

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn NodeCardDrawers(
    /// Inline GLSL editor content for the `code` drawer; `None` omits the
    /// drawer entirely (fixture/playlist faces).
    #[props(default = None)]
    code: Option<UiAssetEditorData>,
    /// Sections for the `advanced` drawer — today's generic slot-row view.
    sections: Vec<UiNodeSection>,
    /// Open the code drawer on first render (stories).
    #[props(default = false)]
    code_initially_open: bool,
    /// Open the advanced drawer on first render (stories).
    #[props(default = false)]
    advanced_initially_open: bool,
    /// Platform for the code editor's shortcut hints; stories pin it for
    /// deterministic captures.
    #[props(default = None)]
    platform: Option<Platform>,
    #[props(default)] pending_edits: Vec<UiPendingEdit>,
    #[props(default)] dirty_tint: NodeDirtyTint,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
) -> Element {
    let mut code_open = use_signal(|| code_initially_open);
    let mut advanced_open = use_signal(|| advanced_initially_open);
    let has_advanced = !sections.is_empty();
    let code_summary = code.as_ref().map(|editor| editor.source.clone());

    rsx! {
        if let Some(editor) = code {
            NodeCardSection {
                label: "code",
                summary: code_summary,
                open: Some(code_open()),
                on_toggle: move |()| code_open.set(!code_open()),
                AssetEditor { editor, on_action, platform }
            }
        }
        if has_advanced {
            NodeCardSection {
                label: "advanced",
                summary: Some("slots · bindings".to_string()),
                open: Some(advanced_open()),
                on_toggle: move |()| advanced_open.set(!advanced_open()),
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
