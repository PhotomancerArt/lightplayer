use core::any::Any;

use crate::{
    ActionClass, ActionMeta, ActionPriority, ControllerOp, PROJECT_EDITOR_ACTION_DEADLINE,
};

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

    fn action_class(&self) -> ActionClass {
        // Project-editor ops are foreground-class, seeded from the retired web
        // policy's `PROJECT_EDITOR_ACTION_TIMEOUT_MS` (6 s). `Focus` becomes a
        // local mutation in P3 (no network refresh), but the class it declares
        // here is the deadline the actor would apply were it to drive a pull.
        match self {
            Self::Focus => ActionClass::Foreground {
                deadline: PROJECT_EDITOR_ACTION_DEADLINE,
            },
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
    use crate::{ActionClass, ControllerOp, PROJECT_EDITOR_ACTION_DEADLINE, ProjectEditorOp};

    #[test]
    fn focus_uses_the_project_editor_deadline() {
        assert_eq!(
            ProjectEditorOp::Focus.action_class(),
            ActionClass::Foreground {
                deadline: PROJECT_EDITOR_ACTION_DEADLINE,
            }
        );
    }
}
