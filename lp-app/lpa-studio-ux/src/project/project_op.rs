use core::any::Any;

use crate::{ActionMeta, ActionPriority, UxOp};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProjectOp {
    ConnectRunningProject,
    ConnectLoadedProject { handle_id: u32 },
    LoadDemoProject,
    DisconnectProject,
}

impl UxOp for ProjectOp {
    fn default_action_meta(&self) -> ActionMeta {
        match self {
            Self::ConnectRunningProject => ActionMeta::new(
                "Connect running project",
                "Attach to a project that is already loaded on the connected server.",
                ActionPriority::Primary,
            ),
            Self::ConnectLoadedProject { .. } => ActionMeta::new(
                "Connect project",
                "Attach to this already-loaded project.",
                ActionPriority::Primary,
            ),
            Self::LoadDemoProject => ActionMeta::new(
                "Load demo project",
                "Upload and run the built-in demo project.",
                ActionPriority::Secondary,
            ),
            Self::DisconnectProject => ActionMeta::new(
                "Disconnect project",
                "Detach Studio from the current project without stopping it on the device.",
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
