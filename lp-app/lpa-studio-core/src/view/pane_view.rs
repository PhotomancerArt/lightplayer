use crate::{UiAction, UiStatus, UiViewContent, UxNodeId};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiPaneView {
    pub node_id: UxNodeId,
    pub title: String,
    pub status: UiStatus,
    pub body: UiViewContent,
    pub actions: Vec<UiAction>,
}

impl UiPaneView {
    pub fn new(
        node_id: impl Into<UxNodeId>,
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
