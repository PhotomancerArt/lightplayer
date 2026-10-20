//! [`PanelStoryLook`]: what a story asks the card's cells and strips to
//! draw that only a hand can make — the hover look, and a control's details
//! open. A story provides it as context around a card; nothing else does,
//! and a cell with none draws as it is.

use dioxus::prelude::*;

/// Story-only looks for the card's panel, by control: its `(scope,
/// channel)`, the panel's identity for a control (two effects' `hue` are
/// two controls).
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct PanelStoryLook {
    /// Controls drawn as if under the pointer: lightened, their ⓘ in view.
    pub hovered: Vec<(String, String)>,
    /// The control whose details are open on first render.
    pub details_open: Option<(String, String)>,
}

impl PanelStoryLook {
    /// Whether the control at `(scope, channel)` is drawn hovered.
    pub(crate) fn hovers(&self, scope: &str, channel: &str) -> bool {
        self.hovered
            .iter()
            .any(|(hovered_scope, hovered)| hovered_scope == scope && hovered == channel)
    }

    /// Whether the control at `(scope, channel)` opens its details on first
    /// render.
    pub(crate) fn opens(&self, scope: &str, channel: &str) -> bool {
        self.details_open
            .as_ref()
            .is_some_and(|(open_scope, open)| open_scope == scope && open == channel)
    }
}

/// The story's look, or none.
pub(crate) fn use_panel_story_look() -> PanelStoryLook {
    use_hook(try_consume_context::<PanelStoryLook>).unwrap_or_default()
}
