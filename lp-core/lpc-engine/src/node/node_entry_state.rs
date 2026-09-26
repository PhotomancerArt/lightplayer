//! Server-side lifecycle state for node entries.
//!
//! The archived design (`docs-archive/roadmaps/2026-04-28-node-runtime/design/01-tree.md`
//! §EntryState) had `Pending` children woken by the resolver and `Failed`
//! reads falling through to slot defaults. Neither holds: the resolver never
//! wakes a node, a parent that wants a child loaded asks for it
//! (`docs/adr/2026-09-25-parent-owned-child-residency.md`), and reading from a
//! node that is not `Alive` is a hard resolve error.

use alloc::string::String;

use super::NodeCallKey;

/// Lifecycle state of a `NodeEntry`.
///
/// Generic over `N` — the payload type when the entry is `Alive`. In M3 this
/// is `()` (no Node trait yet). When the Node trait lands, this becomes
/// `Box<dyn Node>`.
#[derive(Debug)]
pub enum NodeEntryState<N> {
    /// Artifact handle resolved + refcounted; node not yet instantiated.
    /// Nothing wakes a `Pending` node on demand.
    Pending,
    /// Node instantiated and ticking.
    Alive(N),
    /// Node payload is temporarily moved out for an engine-dispatched call.
    Executing { call: NodeCallKey },
    /// Instantiation failed. Reading this node's produced slots is a hard
    /// resolve error ("node … not alive"), like every non-`Alive` state; it
    /// does not fall through to slot defaults.
    Failed { reason: String },
}

impl<N> NodeEntryState<N> {
    /// Returns `true` if this state is `Alive`.
    pub fn is_alive(&self) -> bool {
        matches!(self, NodeEntryState::Alive(_))
    }

    /// Returns `true` if this state is `Pending`.
    pub fn is_pending(&self) -> bool {
        matches!(self, NodeEntryState::Pending)
    }

    /// Returns `true` if this state is `Failed`.
    pub fn is_failed(&self) -> bool {
        matches!(self, NodeEntryState::Failed { .. })
    }

    /// Returns `true` if this state is an active engine-dispatched call.
    pub fn is_executing(&self) -> bool {
        matches!(self, NodeEntryState::Executing { .. })
    }
}

/// Convert server-side `EntryState<N>` to wire-side `WireEntryState`.
impl<N> From<&NodeEntryState<N>> for lpc_wire::WireEntryState {
    fn from(state: &NodeEntryState<N>) -> Self {
        match state {
            NodeEntryState::Pending => lpc_wire::WireEntryState::Pending,
            NodeEntryState::Alive(_) => lpc_wire::WireEntryState::Alive,
            NodeEntryState::Executing { .. } => lpc_wire::WireEntryState::Alive,
            NodeEntryState::Failed { reason } => lpc_wire::WireEntryState::Failed {
                reason: reason.clone(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::NodeEntryState;
    use alloc::string::String;

    #[test]
    fn entry_state_discriminants() {
        let pending: NodeEntryState<()> = NodeEntryState::Pending;
        let alive: NodeEntryState<()> = NodeEntryState::Alive(());
        let failed: NodeEntryState<()> = NodeEntryState::Failed {
            reason: String::from("oom"),
        };

        assert!(pending.is_pending());
        assert!(!pending.is_alive());
        assert!(!pending.is_failed());

        assert!(!alive.is_pending());
        assert!(alive.is_alive());
        assert!(!alive.is_failed());

        assert!(!failed.is_pending());
        assert!(!failed.is_alive());
        assert!(failed.is_failed());
    }
}
