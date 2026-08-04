//! Panel commands: the `(scope, channel)` write surface for panel
//! controls (panel.md P8).
//!
//! Panel writes are runtime state, never authoring: like the node command
//! channel (`docs/adr/2026-07-27-runtime-node-command-channel.md`) they
//! touch no overlay, no pending edit, and no dirty flag — which is also
//! what sidesteps `PendingEdit` value-shadowing and lets multiple clients
//! converge through ordinary probe pulls (last writer wins, panel.md P9).
//! Unlike node commands they address a SCOPE, not a node, so they are
//! project-level arms.

use alloc::string::String;
use lpc_model::LpValue;

use crate::WireScopeRef;

/// Engage (or update) the panel writer for `(scope, channel)`.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct WirePanelWriteRequest {
    /// The scope the write lands in (the control's home scope).
    pub scope: WireScopeRef,
    /// The channel the control drives.
    pub channel: String,
    /// The value to hold.
    pub value: LpValue,
    /// Momentary liveness (panel.md P14): when present, the writer
    /// DESPAWNS unless renewed within this many milliseconds — renewal is
    /// simply another write. Absent = latching writer (holds until an
    /// explicit clear). The TTL/renewal shape follows the design reference
    /// in the punted node-actions direction, not its wire form.
    pub ttl_ms: Option<u32>,
}

/// Clear engaged panel writers (panel.md P3).
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WirePanelClearRequest {
    /// Clear one control.
    Channel {
        scope: WireScopeRef,
        channel: String,
    },
    /// Clear every engaged control in one scope.
    Scope { scope: WireScopeRef },
    /// Clear everything — sink scopes included (settled P-Q4: a playlist
    /// entry's latched value clears too).
    All,
}

/// Turn panel-state auto-save on or off (panel.md P11 — on by default).
///
/// Project-level, not scoped: panel state persists per project folder
/// (`.lp/state.json`), so the switch belongs to the project as a whole and
/// the request carries no scope. The current value rides back on every
/// project read as [`crate::ServerRuntimeStatus::panel_auto_save`].
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct WirePanelAutoSaveRequest {
    /// Whether panel state keeps being written.
    pub enabled: bool,
}

/// Outcome of a panel command. Rejection is a NORMAL response (unknown
/// scope, dead runtime): a stale gesture must never poison the connection.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WirePanelCommandResponse {
    /// Accepted; `engaged` is the store's total engaged-writer count after
    /// the command (drives the ↺ superscript without another read).
    Accepted {
        engaged: u32,
    },
    Rejected {
        reason: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;
    use lpc_model::NodeId;

    #[test]
    fn panel_write_round_trips() {
        let request = WirePanelWriteRequest {
            scope: WireScopeRef::Module {
                owner: NodeId::new(4),
            },
            channel: "speed".to_string(),
            value: LpValue::F32(0.5),
            ttl_ms: Some(1000),
        };
        let json = serde_json::to_string(&request).unwrap();
        let back: WirePanelWriteRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(back, request);
    }

    #[test]
    fn panel_clear_variants_round_trip() {
        for request in [
            WirePanelClearRequest::Channel {
                scope: WireScopeRef::Sink {
                    owner: NodeId::new(2),
                    entry: 7,
                },
                channel: "speed".to_string(),
            },
            WirePanelClearRequest::Scope {
                scope: WireScopeRef::Module {
                    owner: NodeId::new(0),
                },
            },
            WirePanelClearRequest::All,
        ] {
            let json = serde_json::to_string(&request).unwrap();
            let back: WirePanelClearRequest = serde_json::from_str(&json).unwrap();
            assert_eq!(back, request);
        }
    }

    #[test]
    fn panel_auto_save_round_trips() {
        for enabled in [true, false] {
            let request = WirePanelAutoSaveRequest { enabled };
            let json = serde_json::to_string(&request).unwrap();
            assert!(json.contains("enabled"));
            let back: WirePanelAutoSaveRequest = serde_json::from_str(&json).unwrap();
            assert_eq!(back, request);
        }
    }
}
