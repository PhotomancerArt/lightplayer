use crate::{ControllerId, UiAction, UiStatus, UiViewContent};

/// Render data for an addressable workspace region.
///
/// A pane is the Studio-level surface with title, status, body content, and
/// pane-level actions. It is not the visual frame itself; web components decide
/// how to render the pane chrome.
#[derive(Clone, Debug, PartialEq)]
pub struct UiPaneView {
    /// Controller id that owns this pane.
    pub node_id: ControllerId,
    /// Visible pane title.
    pub title: String,
    /// Compact state summary for pane chrome.
    pub status: UiStatus,
    /// Main pane body.
    pub body: UiViewContent,
    /// Pane-level actions.
    pub actions: Vec<UiAction>,
}

impl UiPaneView {
    /// Create a pane view from its owner, title, status, body, and actions.
    pub fn new(
        node_id: impl Into<ControllerId>,
        title: impl Into<String>,
        status: UiStatus,
        body: UiViewContent,
        actions: Vec<UiAction>,
    ) -> Self {
        Self {
            node_id: node_id.into(),
            title: title.into(),
            status,
            body,
            actions,
        }
    }
}
