//! What one pre-tick residency step did ([`super::Engine::apply_residency`]).

use alloc::string::String;
use alloc::vec::Vec;

use lpc_model::NodeId;

use crate::node::ResidencyRequest;

/// One answer to one half of a [`ResidencyRequest`], in the order the engine
/// applied them (an unload always before its load). Each is also delivered
/// to the owner through the matching [`crate::node::NodeRuntime`] hook.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EntryResidencyEvent {
    /// `entry`'s subtree left the tree ([`crate::node::NodeRuntime::entry_unloaded`]).
    Unloaded { owner: NodeId, entry: u32 },
    /// `entry`'s subtree is attached and bound, rooted at `child`
    /// ([`crate::node::NodeRuntime::entry_loaded`]).
    Loaded {
        owner: NodeId,
        entry: u32,
        child: NodeId,
    },
    /// `entry` could not be loaded; nothing of it is attached
    /// ([`crate::node::NodeRuntime::entry_load_failed`]).
    LoadFailed {
        owner: NodeId,
        entry: u32,
        reason: String,
    },
    /// The whole request was refused and nothing changed
    /// ([`crate::node::NodeRuntime::residency_refused`]).
    Refused {
        owner: NodeId,
        request: ResidencyRequest,
        reason: String,
    },
}

/// Everything one [`super::Engine::apply_residency`] call did. Empty on a
/// tick where no node asked for anything — the steady state, which
/// allocates nothing.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ResidencyApplied {
    pub events: Vec<EntryResidencyEvent>,
}

impl ResidencyApplied {
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }
}
