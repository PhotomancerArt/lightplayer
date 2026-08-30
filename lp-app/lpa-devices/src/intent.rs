//! Intent: the prescriptive half of a device's state.
//!
//! Intent is plain state written by [`Action`](crate::Action)s and by
//! nothing else. It is deliberately NOT re-derived from an event log — an
//! "overly pure" event-sourced design was considered and rejected: a user
//! saying "stay connected" is a standing instruction, not an observation.
//!
//! Intent is never journal-retained. Pruning applies to evidence; intent is
//! state.

use serde::{Deserialize, Serialize};

/// What the user has asked of this device.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct Intent {
    pub connection: ConnectionIntent,
    /// The user's chosen name. The device's own provisioned name lives on
    /// [`IdentityChain`](crate::IdentityChain) — a user rename must not
    /// masquerade as evidence.
    pub name: Option<String>,
    pub autoconnect: bool,
    /// The user asked to set this device up (round 2 turns this into a Setup
    /// activity; M1 records the wish and renders it).
    pub setup_requested: bool,
}

/// Whether the user wants this device connected.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub enum ConnectionIntent {
    /// No standing instruction: reachable, but nobody asked for it.
    #[default]
    Idle,
    Connected,
    Disconnected,
}

impl ConnectionIntent {
    pub fn wants_connection(self) -> bool {
        matches!(self, Self::Connected)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fresh_intent_asks_for_nothing() {
        let intent = Intent::default();

        assert_eq!(intent.connection, ConnectionIntent::Idle);
        assert!(!intent.connection.wants_connection());
        assert!(!intent.autoconnect);
        assert!(!intent.setup_requested);
        assert!(intent.name.is_none());
    }
}
