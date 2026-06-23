use core::any::Any;

use crate::{ActionMeta, ActionPriority, UxOp};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ServerOp {
    DisconnectServer,
}

impl UxOp for ServerOp {
    fn default_action_meta(&self) -> ActionMeta {
        match self {
            Self::DisconnectServer => ActionMeta::new(
                "Disconnect server",
                "Detach Studio from the server protocol while keeping the link session open.",
                ActionPriority::Tertiary,
            ),
        }
    }

    fn clone_box(&self) -> Box<dyn UxOp> {
        Box::new(self.clone())
    }

    fn eq_op(&self, other: &dyn UxOp) -> bool {
        other.as_any().downcast_ref::<Self>() == Some(self)
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn into_any(self: Box<Self>) -> Box<dyn Any> {
        self
    }
}
