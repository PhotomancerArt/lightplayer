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

/// Which pane a progressive activity update lands on.
///
/// The `StackSection` variant retired with the step-stack device pane —
/// it was the only surface with addressable sub-sections, and its only
/// producer narrated connect/flash progress into it. That narration lives
/// on the device CARD now (state-flow model §2).
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UxActivityTarget {
    Pane { node_id: ControllerId },
}

impl UxActivityTarget {
    pub fn pane(node_id: impl Into<ControllerId>) -> Self {
        Self::Pane {
            node_id: node_id.into(),
        }
    }

    pub fn pane_node_id(&self) -> &ControllerId {
        match self {
            Self::Pane { node_id } => node_id,
        }
    }
}
