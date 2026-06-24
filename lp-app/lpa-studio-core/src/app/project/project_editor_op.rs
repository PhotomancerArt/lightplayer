use core::any::Any;

use crate::{ActionMeta, ActionPriority, ControllerOp};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProjectEditorOp {
    Focus,
}

impl ControllerOp for ProjectEditorOp {
    fn default_action_meta(&self) -> ActionMeta {
        match self {
            Self::Focus => ActionMeta::new(
                "Focus",
                "Focus this project editor surface.",
                ActionPriority::Secondary,
            ),
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
