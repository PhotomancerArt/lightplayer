//! Playlist pick operation: Play mode's tap, next and previous.

use core::any::Any;

use crate::{
    ActionClass, ActionMeta, ActionPriority, ControllerOp, PROJECT_EDITOR_ACTION_DEADLINE,
    ProjectNodeAddress,
};

/// Pick one playlist entry: a panel write of its key on the playlist's
/// `playlist.entry` channel (playlists-on-the-bus vision D1). The pick
/// holds, survives a reboot, and Clear returns the authored entry.
///
/// A panel write with one thing more than [`crate::PanelWriteOp`]: the
/// editor opens the picked entry's card once it lands (load-to-edit,
/// multi-pattern D11), which is why it names the playlist.
#[derive(Clone, Debug, PartialEq)]
pub struct PlaylistPickOp {
    /// Stable address of the playlist node whose entry is picked.
    pub node: ProjectNodeAddress,
    /// The `entries`-map key to pick (what `PlaylistState.active_entry`
    /// reports once it plays).
    pub entry: u32,
    /// The scope the playlist reads `playlist.entry` in: where the write
    /// lands.
    pub scope: lpc_wire::WireScopeRef,
}

impl ControllerOp for PlaylistPickOp {
    fn default_action_meta(&self) -> ActionMeta {
        ActionMeta::new(
            "Play pattern",
            "Switch the playlist to this pattern and stay on it.",
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pick_is_editor_foreground_class() {
        let op = PlaylistPickOp {
            node: ProjectNodeAddress::parse("/demo.module/show.playlist").unwrap(),
            entry: 2,
            scope: lpc_wire::WireScopeRef::Module {
                owner: lpc_model::NodeId::new(1),
            },
        };

        assert_eq!(
            op.action_class(),
            ActionClass::Foreground {
                deadline: PROJECT_EDITOR_ACTION_DEADLINE,
            }
        );
        assert_eq!(op.default_action_meta().priority, ActionPriority::Primary);
    }
}
