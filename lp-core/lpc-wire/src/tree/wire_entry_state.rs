//! Wire-shape lifecycle state for a node entry (`WireEntryState`).

use alloc::string::String;

/// Client-side view of a node's lifecycle state.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum WireEntryState {
    Pending,
    Alive,
    Failed { reason: String },
}

#[cfg(test)]
mod tests {
    use super::WireEntryState;
    use alloc::string::String;

    #[test]
    fn wire_entry_state_pending_round_trips() {
        let state = WireEntryState::Pending;
        let json = serde_json::to_string(&state).unwrap();
        let decoded: WireEntryState = serde_json::from_str(&json).unwrap();
        assert_eq!(state, decoded);
    }

    #[test]
    fn wire_entry_state_alive_round_trips() {
        let state = WireEntryState::Alive;
        let json = serde_json::to_string(&state).unwrap();
        let decoded: WireEntryState = serde_json::from_str(&json).unwrap();
        assert_eq!(state, decoded);
    }

    #[test]
    fn wire_entry_state_failed_round_trips() {
        let state = WireEntryState::Failed {
            reason: String::from("oom during shader compile"),
        };
        let json = serde_json::to_string(&state).unwrap();
        let decoded: WireEntryState = serde_json::from_str(&json).unwrap();
        assert_eq!(state, decoded);
    }
}
