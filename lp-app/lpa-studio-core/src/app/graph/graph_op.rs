//! Pressing one of the Graph's verbs: what its offer's action carries.

use core::any::Any;

use lab_mapping_model::GraphVerb;

use crate::{
    ActionClass, ActionMeta, ActionPriority, ControllerId, ControllerOp,
    PROJECT_EDITOR_ACTION_DEADLINE, UiAction,
};

/// Press `verb` on the Graph's editor, as its key does it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GraphOp {
    pub verb: GraphVerb,
}

impl GraphOp {
    /// The verb as an action for the studio actor, addressed to the open
    /// project (the Graph edits it).
    pub fn action(verb: GraphVerb) -> UiAction {
        UiAction::from_op(
            ControllerId::new(crate::ProjectController::NODE_ID),
            GraphOp { verb },
        )
        .with_label(verb.label())
    }
}

impl ControllerOp for GraphOp {
    fn default_action_meta(&self) -> ActionMeta {
        ActionMeta::new(
            self.verb.label(),
            "Do this on the Graph, as its key does.",
            ActionPriority::Secondary,
        )
    }

    fn action_class(&self) -> ActionClass {
        ActionClass::Foreground {
            deadline: PROJECT_EDITOR_ACTION_DEADLINE,
        }
    }

    fn clone_box(&self) -> Box<dyn ControllerOp> {
        Box::new(*self)
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
