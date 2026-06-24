use crate::{UiAction, UiBody, UiStatus, UxNodeId};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiPaneView {
    pub node_id: UxNodeId,
    pub title: String,
    pub status: UiStatus,
    pub body: UiBody,
    pub actions: Vec<UiAction>,
}

impl UiPaneView {
    pub fn new(
        node_id: impl Into<UxNodeId>,
        title: impl Into<String>,
        status: UiStatus,
        body: UiBody,
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
