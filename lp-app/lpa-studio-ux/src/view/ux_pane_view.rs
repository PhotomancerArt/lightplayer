use crate::{UxAction, UxBody, UxNodeId, UxStatus};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UxPaneView {
    pub node_id: UxNodeId,
    pub title: String,
    pub status: UxStatus,
    pub body: UxBody,
    pub actions: Vec<UxAction>,
}

impl UxPaneView {
    pub fn new(
        node_id: impl Into<UxNodeId>,
        title: impl Into<String>,
        status: UxStatus,
        body: UxBody,
        actions: Vec<UxAction>,
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
