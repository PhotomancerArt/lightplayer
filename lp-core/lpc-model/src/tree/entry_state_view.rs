//! Wire-shape discriminant for node lifecycle state.
//!
//! This is the client-side / wire view of `EntryState`. The server-side
//! `EntryState<N>` carries an `Alive(N)` payload; the wire only needs the
//! discriminant.
//!
//! See `docs/roadmaps/2026-04-28-node-runtime/design/01-tree.md` §EntryState.

use alloc::string::String;

/// Client-side view of a node's lifecycle state.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum EntryStateView {
    /// Artifact handle resolved + refcounted; node not yet instantiated.
    Pending,
    /// Node instantiated and ticking (or ready to tick).
    Alive,
    /// Instantiation failed; resolution falls through to slot defaults.
    Failed { reason: String },
}

#[cfg(test)]
mod tests {
    use super::EntryStateView;
    use alloc::string::String;

    #[test]
    fn entry_state_view_pending_round_trips() {
        let state = EntryStateView::Pending;
        let json = serde_json::to_string(&state).unwrap();
        let decoded: EntryStateView = serde_json::from_str(&json).unwrap();
        assert_eq!(state, decoded);
    }

    #[test]
    fn entry_state_view_alive_round_trips() {
        let state = EntryStateView::Alive;
        let json = serde_json::to_string(&state).unwrap();
        let decoded: EntryStateView = serde_json::from_str(&json).unwrap();
        assert_eq!(state, decoded);
    }

    #[test]
    fn entry_state_view_failed_round_trips() {
        let state = EntryStateView::Failed {
            reason: String::from("oom during shader compile"),
        };
        let json = serde_json::to_string(&state).unwrap();
        let decoded: EntryStateView = serde_json::from_str(&json).unwrap();
        assert_eq!(state, decoded);
    }
}
