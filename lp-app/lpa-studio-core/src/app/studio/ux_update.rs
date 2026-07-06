use crate::{ControllerId, UiActivityView, UiLogDraft, UiStatus, UiStudioView};

#[derive(Clone, Debug, PartialEq)]
pub enum UxUpdate {
    View(UiStudioView),
    Activity {
        target: UxActivityTarget,
        status: UiStatus,
        activity: UiActivityView,
    },
    /// A progressive log line emitted mid-action. Carries an unstamped draft
    /// (producers have no clock); the consumer stamps it — the controller via
    /// `push_log`, the actor with the controller's shared clock.
    Log(UiLogDraft),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UxActivityTarget {
    Pane {
        node_id: ControllerId,
    },
    StackSection {
        pane_node_id: ControllerId,
        section_id: String,
    },
}

impl UxActivityTarget {
    pub fn pane(node_id: impl Into<ControllerId>) -> Self {
        Self::Pane {
            node_id: node_id.into(),
        }
    }

    pub fn stack_section(
        pane_node_id: impl Into<ControllerId>,
        section_id: impl Into<String>,
    ) -> Self {
        Self::StackSection {
            pane_node_id: pane_node_id.into(),
            section_id: section_id.into(),
        }
    }

    pub fn pane_node_id(&self) -> &ControllerId {
        match self {
            Self::Pane { node_id } => node_id,
            Self::StackSection { pane_node_id, .. } => pane_node_id,
        }
    }
}
