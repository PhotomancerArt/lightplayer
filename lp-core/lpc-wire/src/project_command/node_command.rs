//! Runtime node commands: client-initiated pokes at live node runtimes.
//!
//! This is the wire vocabulary for the runtime command channel — the first
//! client→engine write surface that is NOT an overlay mutation (see
//! `docs/adr/2026-07-27-runtime-node-command-channel.md`). A command names a
//! runtime node by its [`NodeId`] (the same runtime id project reads and
//! tree deltas carry) and asks the node's runtime to act NOW; nothing is
//! staged, nothing shows in the Save panel, and nothing persists.
//!
//! Growth policy: ONE externally-tagged enum, one variant per command,
//! payloads inline. Future consumers (sim button presses, debug pokes) add
//! variants here rather than new [`crate::WireProjectCommand`] arms. Every
//! variant change is a breaking wire change — bump
//! [`crate::WIRE_PROTO_VERSION`].

use alloc::string::String;

/// One command addressed to a runtime node.
///
/// Externally tagged (serde-content lint: no `tag`/`untagged`), snake_case
/// like every project-command payload.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WireNodeCommand {
    /// Ask a playlist runtime to make `entry` (an `entries`-map key, the
    /// same u32 `PlaylistState.active_entry` reports) the active entry,
    /// resetting the entry clock exactly as a trigger switch does.
    PlaylistActivateEntry { entry: u32 },
    /// Synthetic button event addressed to a button node runtime.
    ButtonEvent { event: WireButtonEvent },
    /// Put an output node into (or out of) test-pattern mode. Sustained
    /// patterns auto-expire at `ttl_ms` unless renewed by re-sending the
    /// command.
    OutputTestPattern {
        pattern: WireOutputTestPattern,
        ttl_ms: u32,
    },
}

/// A synthetic button event addressed to a button node runtime.
///
/// Externally tagged, snake_case, like every project-command payload.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WireButtonEvent {
    /// Minimal down-then-up pair (down one produce, up the next).
    Click,
    /// Begin/renew a sustained synthetic hold. Auto-releases at `ttl_ms`
    /// unless renewed by re-sending with the same `press_id`.
    Press { press_id: u32, ttl_ms: u32 },
    /// End the synthetic hold started with `press_id`.
    Release { press_id: u32 },
}

/// An output node's test-pattern payload.
///
/// Externally tagged, snake_case, like every project-command payload.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WireOutputTestPattern {
    /// End test-pattern mode immediately (ignores `ttl_ms`).
    Clear,
    /// Fill every pixel with one color. Auto-expires at `ttl_ms` unless
    /// renewed by re-sending the command.
    Solid { r: u8, g: u8, b: u8 },
}

/// Outcome of a node command.
///
/// Rejection is a NORMAL response — an unknown node, a dead runtime, an
/// unsupported command, or an out-of-range payload answers `Rejected` with
/// a reason instead of failing the request envelope, so a stale click never
/// poisons the connection or the node's runtime status.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WireNodeCommandResponse {
    /// The runtime accepted the command; the effect lands on the next
    /// engine frame and is observed through ordinary project reads.
    Accepted,
    /// The runtime (or the engine's dispatch) declined the command.
    Rejected { reason: String },
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;

    #[test]
    fn node_command_round_trips_externally_tagged() {
        let command = WireNodeCommand::PlaylistActivateEntry { entry: 2 };
        let json = serde_json::to_string(&command).unwrap();
        assert!(json.contains("playlist_activate_entry"));
        let back: WireNodeCommand = serde_json::from_str(&json).unwrap();
        assert_eq!(back, command);
    }

    #[test]
    fn button_event_click_round_trips() {
        let command = WireNodeCommand::ButtonEvent {
            event: WireButtonEvent::Click,
        };
        let json = serde_json::to_string(&command).unwrap();
        assert!(json.contains("button_event"));
        assert!(json.contains("click"));
        let back: WireNodeCommand = serde_json::from_str(&json).unwrap();
        assert_eq!(back, command);
    }

    #[test]
    fn button_event_press_round_trips() {
        let command = WireNodeCommand::ButtonEvent {
            event: WireButtonEvent::Press {
                press_id: 7,
                ttl_ms: 500,
            },
        };
        let json = serde_json::to_string(&command).unwrap();
        assert!(json.contains("button_event"));
        assert!(json.contains("press"));
        let back: WireNodeCommand = serde_json::from_str(&json).unwrap();
        assert_eq!(back, command);
    }

    #[test]
    fn button_event_release_round_trips() {
        let command = WireNodeCommand::ButtonEvent {
            event: WireButtonEvent::Release { press_id: 7 },
        };
        let json = serde_json::to_string(&command).unwrap();
        assert!(json.contains("button_event"));
        assert!(json.contains("release"));
        let back: WireNodeCommand = serde_json::from_str(&json).unwrap();
        assert_eq!(back, command);
    }

    #[test]
    fn output_test_pattern_clear_round_trips() {
        let command = WireNodeCommand::OutputTestPattern {
            pattern: WireOutputTestPattern::Clear,
            ttl_ms: 0,
        };
        let json = serde_json::to_string(&command).unwrap();
        assert!(json.contains("output_test_pattern"));
        assert!(json.contains("clear"));
        let back: WireNodeCommand = serde_json::from_str(&json).unwrap();
        assert_eq!(back, command);
    }

    #[test]
    fn output_test_pattern_solid_round_trips() {
        let command = WireNodeCommand::OutputTestPattern {
            pattern: WireOutputTestPattern::Solid {
                r: 255,
                g: 128,
                b: 0,
            },
            ttl_ms: 2000,
        };
        let json = serde_json::to_string(&command).unwrap();
        assert!(json.contains("output_test_pattern"));
        assert!(json.contains("solid"));
        let back: WireNodeCommand = serde_json::from_str(&json).unwrap();
        assert_eq!(back, command);
    }

    #[test]
    fn node_command_response_round_trips_both_outcomes() {
        let accepted = WireNodeCommandResponse::Accepted;
        let json = serde_json::to_string(&accepted).unwrap();
        assert_eq!(json, "\"accepted\"");
        let back: WireNodeCommandResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(back, accepted);

        let rejected = WireNodeCommandResponse::Rejected {
            reason: "playlist has no entry 9".to_string(),
        };
        let json = serde_json::to_string(&rejected).unwrap();
        assert!(json.contains("rejected"));
        let back: WireNodeCommandResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(back, rejected);
    }
}
