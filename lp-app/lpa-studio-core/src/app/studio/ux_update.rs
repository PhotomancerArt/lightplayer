use crate::{CardOp, ControllerId, UiActivityView, UiLogDraft, UiStatus, UiStudioView};

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
    /// The CARD-OWNED op flow's live state (state-flow model §2), emitted
    /// as the op ticks so the card's in-place overlay — its progress bar
    /// and its narration — tracks the work instead of freezing at the
    /// dispatch-time label.
    ///
    /// The controller's own `device_card_op` slot is the authority (it is
    /// what a full view build reads); this is the delta that carries the
    /// same value to the live view BETWEEN full snapshots, because a
    /// management op holds `&mut controller` for its whole duration and
    /// cannot rebuild a view mid-flight.
    CardOp {
        /// The managed device's stamped uid, or `None` for an
        /// identity-less board — matched by
        /// [`UiDeviceCard::takes_card_op`](crate::UiDeviceCard::takes_card_op).
        uid: Option<String>,
        op: CardOp,
    },
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
