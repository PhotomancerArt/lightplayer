//! Protocol events observed while waiting for a request response.
//!
//! `lp-server` can emit heartbeats and logs between correlated responses. The
//! portable client returns those events to callers instead of printing or
//! mutating UI state directly.

use lpc_wire::server::api::LogLevel;
use lpc_wire::server::{LoadedProject, MemoryStats, RecoveryStatus, SampleStats, ServerHello};
use lpc_wire::{WireServerMessage, WireServerMsgBody};

/// Side-channel protocol event surfaced by `LpClient`.
#[derive(Debug)]
pub enum ClientEvent {
    /// Wire bootstrap hello: protocol version + build provenance + device
    /// uid, sent unsolicited (id 0) when the server loop starts serving.
    Hello(ServerHello),
    /// Periodic server health and performance sample.
    Heartbeat {
        fps: SampleStats,
        frame_count: u64,
        loaded_projects: Vec<LoadedProject>,
        uptime_ms: u64,
        memory: Option<MemoryStats>,
        recovery: Option<RecoveryStatus>,
    },
    /// Firmware/server log line carried by the protocol.
    Log { level: LogLevel, message: String },
    /// A genuinely unexpected response id arrived: never issued/abandoned by
    /// this session (from the future, or a duplicate delivery). Consumers
    /// should surface this as a warning.
    UncorrelatedResponse { response_id: u64, expected_id: u64 },
    /// A late response for a request this client itself abandoned (cancelled
    /// or timed-out pull) arrived and was discarded. This is the designed
    /// stale-drop — expected under edit-op preemption during input floods —
    /// so consumers should keep it quiet (at most debug level).
    StaleResponseDropped { response_id: u64 },
}

impl ClientEvent {
    pub fn from_unsolicited_message(message: WireServerMessage) -> Option<Self> {
        match message.msg {
            WireServerMsgBody::Hello(hello) => Some(Self::Hello(hello)),
            WireServerMsgBody::Heartbeat {
                fps,
                frame_count,
                loaded_projects,
                uptime_ms,
                memory,
                recovery,
            } => Some(Self::Heartbeat {
                fps,
                frame_count,
                loaded_projects,
                uptime_ms,
                memory,
                recovery,
            }),
            WireServerMsgBody::Log { level, message } => Some(Self::Log { level, message }),
            _ => None,
        }
    }
}
