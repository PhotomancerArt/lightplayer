//! Server-side lazy lifecycle state for node entries.
//!
//! See `docs/roadmaps/2026-04-28-node-runtime/design/01-tree.md` §EntryState.

use alloc::string::String;

/// Server-side lifecycle state of a `NodeEntry`.
///
/// Generic over `N` — the payload type when the entry is `Alive`. In M3 this
/// is `()` (no Node trait yet). When the Node trait lands, this becomes
/// `Box<dyn Node>`.
#[derive(Clone, Debug, PartialEq)]
pub enum EntryState<N> {
    /// Artifact handle resolved + refcounted; node not yet instantiated.
    Pending,
    /// Node instantiated and ticking.
    Alive(N),
    /// Instantiation failed; resolution falls through to slot defaults.
    Failed { reason: String },
}

impl<N> EntryState<N> {
    /// Returns `true` if this state is `Alive`.
    pub fn is_alive(&self) -> bool {
        matches!(self, EntryState::Alive(_))
    }

    /// Returns `true` if this state is `Pending`.
    pub fn is_pending(&self) -> bool {
        matches!(self, EntryState::Pending)
    }

    /// Returns `true` if this state is `Failed`.
    pub fn is_failed(&self) -> bool {
        matches!(self, EntryState::Failed { .. })
    }
}

/// Convert server-side `EntryState<N>` to wire-side `EntryStateView`.
#[cfg(feature = "std")]
impl<N> From<&EntryState<N>> for lpc_model::EntryStateView {
    fn from(state: &EntryState<N>) -> Self {
        match state {
            EntryState::Pending => lpc_model::EntryStateView::Pending,
            EntryState::Alive(_) => lpc_model::EntryStateView::Alive,
            EntryState::Failed { reason } => lpc_model::EntryStateView::Failed {
                reason: reason.clone(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::EntryState;
    use alloc::string::String;

    #[test]
    fn entry_state_discriminants() {
        let pending: EntryState<()> = EntryState::Pending;
        let alive: EntryState<()> = EntryState::Alive(());
        let failed: EntryState<()> = EntryState::Failed {
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
