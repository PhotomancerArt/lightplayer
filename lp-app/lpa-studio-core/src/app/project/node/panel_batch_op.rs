//! Several panel writes and clears pressed as one gesture.

use core::any::Any;

use crate::{
    ActionClass, ActionMeta, ActionPriority, ControllerOp, PROJECT_EDITOR_ACTION_DEADLINE,
    PanelWriteOp,
};

/// Panel writes, then panel clears, sent in order as one action: the
/// Pattern zone's reset (the pick, the cycle and the skip list back to
/// what was authored, vision D13), and turning the cycle off on the
/// playing pattern (the cycle held and the playing pattern picked, so the
/// hold survives a reboot). A scope clear would take every shared knob of
/// the scope with it; a batch names exactly its channels.
#[derive(Clone, Debug, PartialEq)]
pub struct PanelBatchOp {
    /// Writes, sent first.
    pub writes: Vec<PanelWriteOp>,
    /// Clears, sent after the writes.
    pub clears: Vec<lpc_wire::WirePanelClearRequest>,
}

impl ControllerOp for PanelBatchOp {
    fn default_action_meta(&self) -> ActionMeta {
        ActionMeta::new(
            "Set panel controls",
            "Hold these values, or let these controls go, together.",
            ActionPriority::Primary,
        )
    }

    fn action_class(&self) -> ActionClass {
        ActionClass::Foreground {
            deadline: PROJECT_EDITOR_ACTION_DEADLINE,
        }
    }

    fn clone_box(&self) -> Box<dyn ControllerOp> {
        Box::new(self.clone())
    }

    fn eq_op(&self, other: &dyn ControllerOp) -> bool {
        other.as_any().downcast_ref::<Self>() == Some(self)
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn into_any(self: Box<Self>) -> Box<dyn Any> {
        self
    }
}
