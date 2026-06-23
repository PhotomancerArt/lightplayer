use crate::{StudioView, UiActivity, UiStatus, UxLogEntry, UxNodeId};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UxUpdate {
    View(StudioView),
    Activity {
        target: UxActivityTarget,
        status: UiStatus,
        activity: UiActivity,
    },
    Log(UxLogEntry),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UxActivityTarget {
    Pane {
        node_id: UxNodeId,
    },
    StackSection {
        pane_node_id: UxNodeId,
        section_id: String,
    },
}

impl UxActivityTarget {
    pub fn pane(node_id: impl Into<UxNodeId>) -> Self {
        Self::Pane {
            node_id: node_id.into(),
        }
    }

    pub fn stack_section(pane_node_id: impl Into<UxNodeId>, section_id: impl Into<String>) -> Self {
        Self::StackSection {
            pane_node_id: pane_node_id.into(),
            section_id: section_id.into(),
        }
    }

    pub fn pane_node_id(&self) -> &UxNodeId {
        match self {
            Self::Pane { node_id } => node_id,
            Self::StackSection { pane_node_id, .. } => pane_node_id,
        }
    }
}
