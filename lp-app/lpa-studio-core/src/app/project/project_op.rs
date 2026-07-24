use core::any::Any;

use crate::{
    ActionClass, ActionMeta, ActionPriority, ControllerOp, PROJECT_ACTION_DEADLINE,
    PROJECT_EDITOR_ACTION_DEADLINE, PROJECT_LOAD_DEADLINE,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProjectOp {
    ConnectRunningProject,
    ConnectLoadedProject {
        handle_id: u32,
    },
    LoadDemoProject,
    RefreshProject,
    DisconnectProject,
    /// Detach the editor lens (runtime-pool P3): the mirror drops, every
    /// runtime session KEEPS running — worker alive, wire client attached,
    /// device reconcile state intact. The gallery-return route policy
    /// dispatches this (the retired policy tore the whole pool down).
    ///
    /// Quiesce: the actor's serialized dispatch is the quiesce — every
    /// edit action is fully awaited (its ack landed) before the next
    /// queued command runs, and an in-flight passive pull is cancelled at
    /// a frame boundary by this op's Foreground class before it executes.
    /// Nothing acked is ever lost; the acked overlay lives server-side and
    /// a re-attach rebuilds the mirror over it.
    DetachLens,
    /// The D29 click (runtime-pool P3) and the `#/device/<uid>` route
    /// (D37, M5): move the editor lens onto the DEVICE session and open
    /// its running project in the editor — connect against the device's
    /// own wire client (the device reports its loaded handle), then sync
    /// the mirror. A project open on the sim quiesces first; the sim
    /// session stays in the pool.
    ///
    /// `uid: None` (the card click) targets the ≤1 attached device
    /// session. `uid: Some(dev_…)` (route reload/navigation) attaches the
    /// existing session when its identity matches; otherwise it runs the
    /// M1 granted-port connect first — connecting/failed states render
    /// honestly on the gallery's device card (its connect evidence), never
    /// as new UI.
    OpenDeviceProject {
        uid: Option<String>,
    },
    /// The sim-card click (runtime-pool P4): re-attach the editor lens to
    /// THE sim session and open what it is running — the D29 grammar's sim
    /// arm, mirroring [`Self::OpenDeviceProject`]. The mirror rebuilds
    /// over the session's server-side overlay; a lens on the device
    /// quiesces first and that session stays in the pool.
    OpenSimProject,
    /// Commit the pending-edit overlay: persisted edits are written back to
    /// def artifacts; transient edits stay pending (live-only).
    SaveOverlay,
    /// Discard every pending edit — the local edit buffer and the server
    /// overlay both clear.
    RevertAllEdits,
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
            Self::DetachLens => ActionMeta::new(
                "Close editor",
                "Close the editor; every runtime keeps running.",
                ActionPriority::Tertiary,
            ),
            Self::OpenDeviceProject { .. } => ActionMeta::new(
                "Open in editor",
                "Edit the project this device is running.",
                ActionPriority::Primary,
            ),
            Self::OpenSimProject => ActionMeta::new(
                "Open in editor",
                "Edit the project running in the simulator.",
                ActionPriority::Primary,
            ),
            Self::SaveOverlay => ActionMeta::new(
                "Save",
                "Write pending persisted edits back to the project files.",
                ActionPriority::Primary,
            ),
            Self::RevertAllEdits => ActionMeta::new(
                "Revert all",
                "Discard every pending edit on this project.",
                ActionPriority::Secondary,
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
            | Self::DisconnectProject
            // Lens moves preempt an in-flight passive pull (clean cancel at
            // a frame boundary) — that preemption plus the actor's action
            // serialization IS the detach quiesce.
            | Self::DetachLens
            | Self::OpenSimProject => ActionClass::Foreground {
                deadline: PROJECT_ACTION_DEADLINE,
            },
            // The route-driven arm may run a full granted-port connect +
            // attach before the mirror opens — the load budget fits.
            Self::OpenDeviceProject { .. } => ActionClass::Foreground {
                deadline: PROJECT_LOAD_DEADLINE,
            },
            Self::LoadDemoProject => ActionClass::Foreground {
                deadline: PROJECT_LOAD_DEADLINE,
            },
            // Editing ops share the project-editor quiet-gap budget (D5:
            // all edit ops are Foreground/6 s).
            Self::SaveOverlay | Self::RevertAllEdits => ActionClass::Foreground {
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
            ProjectOp::DetachLens,
            ProjectOp::OpenSimProject,
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
    fn load_demo_and_device_open_use_the_project_load_deadline() {
        // the device open's route arm may run a full granted-port connect
        // + attach before the mirror opens — the load budget fits
        for op in [
            ProjectOp::LoadDemoProject,
            ProjectOp::OpenDeviceProject { uid: None },
            ProjectOp::OpenDeviceProject {
                uid: Some("dev_aaaaaaaaaaaaaaaa".to_string()),
            },
        ] {
            assert_eq!(
                op.action_class(),
                ActionClass::Foreground {
                    deadline: PROJECT_LOAD_DEADLINE,
                },
                "{op:?}"
            );
        }
    }

    #[test]
    fn overlay_edit_ops_use_the_editor_deadline() {
        for op in [ProjectOp::SaveOverlay, ProjectOp::RevertAllEdits] {
            assert_eq!(
                op.action_class(),
                ActionClass::Foreground {
                    deadline: crate::PROJECT_EDITOR_ACTION_DEADLINE,
                },
                "{op:?}"
            );
        }
    }
}
