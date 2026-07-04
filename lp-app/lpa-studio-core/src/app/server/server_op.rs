use core::any::Any;

use crate::{ActionClass, ActionMeta, ActionPriority, ControllerOp};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ServerOp {
    DisconnectServer,
}

impl ControllerOp for ServerOp {
    fn default_action_meta(&self) -> ActionMeta {
        match self {
            Self::DisconnectServer => ActionMeta::new(
                "Disconnect server",
                "Detach Studio from the server protocol while keeping the link session open.",
                ActionPriority::Tertiary,
            ),
        }
    }

    fn action_class(&self) -> ActionClass {
        // `DisconnectServer` is in the retired web policy's preemption set, so
        // it is recovery-class: it preempts an in-flight refresh / foreground
        // action and carries no deadline.
        match self {
            Self::DisconnectServer => ActionClass::Recovery,
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

#[cfg(test)]
mod tests {
    use crate::{ActionClass, ControllerOp, ServerOp};

    #[test]
    fn disconnect_server_is_recovery_class() {
        assert_eq!(
            ServerOp::DisconnectServer.action_class(),
            ActionClass::Recovery
        );
    }
}
