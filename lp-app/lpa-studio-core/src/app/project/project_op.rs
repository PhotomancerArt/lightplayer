use core::any::Any;

use crate::{
    ActionClass, ActionMeta, ActionPriority, ControllerOp, PROJECT_ACTION_DEADLINE,
    PROJECT_LOAD_DEADLINE,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProjectOp {
    ConnectRunningProject,
    ConnectLoadedProject { handle_id: u32 },
    LoadDemoProject,
    RefreshProject,
    DisconnectProject,
}

impl ControllerOp for ProjectOp {
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
            Self::RefreshProject => ActionMeta::new(
                "Refresh project",
                "Refresh Studio's synced project view.",
                ActionPriority::Secondary,
            ),
            Self::DisconnectProject => ActionMeta::new(
                "Disconnect project",
                "Detach Studio from the current project without stopping it on the device.",
                ActionPriority::Tertiary,
            ),
        }
    }

    fn action_class(&self) -> ActionClass {
        // Project ops are foreground-class: they preempt a passive refresh but
        // not each other, and carry a quiet-gap deadline. Seeded from the
        // retired web policy's `foreground_action_timeout_ms`:
        //   - connect / attach / refresh → `PROJECT_ACTION_TIMEOUT_MS` (8 s)
        //   - demo-project load          → `PROJECT_LOAD_TIMEOUT_MS`   (20 s)
        // `DisconnectProject` had `None` there (no wall-clock cap), but a
        // foreground op needs a deadline; the standard project budget is a safe
        // quiet-gap bound for it (a disconnect that never makes progress should
        // not hang the loop indefinitely).
        match self {
            Self::ConnectRunningProject
            | Self::ConnectLoadedProject { .. }
            | Self::RefreshProject
            | Self::DisconnectProject => ActionClass::Foreground {
                deadline: PROJECT_ACTION_DEADLINE,
            },
            Self::LoadDemoProject => ActionClass::Foreground {
                deadline: PROJECT_LOAD_DEADLINE,
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
    use crate::{
        ActionClass, ControllerOp, PROJECT_ACTION_DEADLINE, PROJECT_LOAD_DEADLINE, ProjectOp,
    };

    #[test]
    fn connect_attach_and_refresh_use_the_project_action_deadline() {
        for op in [
            ProjectOp::ConnectRunningProject,
            ProjectOp::ConnectLoadedProject { handle_id: 1 },
            ProjectOp::RefreshProject,
            ProjectOp::DisconnectProject,
        ] {
            assert_eq!(
                op.action_class(),
                ActionClass::Foreground {
                    deadline: PROJECT_ACTION_DEADLINE,
                },
                "{op:?}"
            );
        }
    }

    #[test]
    fn load_demo_uses_the_project_load_deadline() {
        assert_eq!(
            ProjectOp::LoadDemoProject.action_class(),
            ActionClass::Foreground {
                deadline: PROJECT_LOAD_DEADLINE,
            }
        );
    }
}
