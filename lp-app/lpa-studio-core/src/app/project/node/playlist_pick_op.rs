//! Playlist activate-entry operation (the runtime command channel's first
//! UI consumer).

use core::any::Any;

use crate::{
    ActionClass, ActionMeta, ActionPriority, ControllerOp, PROJECT_EDITOR_ACTION_DEADLINE,
    ProjectNodeAddress,
};

/// Make one playlist entry active NOW, via
/// `WireProjectCommand::NodeCommand` → `PlaylistNode` (entry-strip click,
/// node-card P4 follow-up). A runtime poke, not an edit: nothing is staged
/// in the overlay, nothing shows in the Save panel, and the effect lands on
/// the engine's next frame — the strip's ACTIVE placard follows through
/// ordinary project reads.
///
/// Dispatched to `ProjectController::NODE_ID` like [`crate::SlotEditOp`];
/// the controller resolves the playlist's CURRENT runtime `NodeId` from the
/// stable authored address at dispatch time, so a queued click can never
/// address a stale runtime id across a project reload.
#[derive(Clone, Debug, PartialEq)]
pub struct PlaylistActivateOp {
    /// Stable address of the playlist node whose entry is activated.
    pub node: ProjectNodeAddress,
    /// The `entries`-map key to activate (the u32
    /// `PlaylistState.active_entry` reports).
    pub entry: u32,
}

impl ControllerOp for PlaylistActivateOp {
    fn default_action_meta(&self) -> ActionMeta {
        ActionMeta::new(
            "Activate entry",
            "Switch the playlist to this entry now.",
            ActionPriority::Primary,
        )
    }

    fn action_class(&self) -> ActionClass {
        // Same editor foreground class as the slot-level edit ops: a click
        // should feel immediate, and a rejection surfaces as a notice.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn activate_is_editor_foreground_class_with_activate_meta() {
        let op = PlaylistActivateOp {
            node: ProjectNodeAddress::parse("/demo.module/show.playlist").unwrap(),
            entry: 2,
        };

        assert_eq!(
            op.action_class(),
            ActionClass::Foreground {
                deadline: PROJECT_EDITOR_ACTION_DEADLINE,
            }
        );
        assert_eq!(op.default_action_meta().label, "Activate entry");
        assert_eq!(op.default_action_meta().priority, ActionPriority::Primary);
    }
}
