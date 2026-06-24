use crate::{ControllerId, UiAction, UiStatus, UiViewContent};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiPaneView {
    pub node_id: ControllerId,
    pub title: String,
    pub status: UiStatus,
    pub body: UiViewContent,
    pub actions: Vec<UiAction>,
}

impl UiPaneView {
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
