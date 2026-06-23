//! Protocol events observed while waiting for a request response.
//!
//! `lp-server` can emit heartbeats and logs between correlated responses. The
//! portable client returns those events to callers instead of printing or
//! mutating UI state directly.

use lpc_wire::server::api::LogLevel;
use lpc_wire::server::{LoadedProject, MemoryStats, SampleStats};
use lpc_wire::{WireServerMessage, WireServerMsgBody};

/// Side-channel protocol event surfaced by `LpClient`.
#[derive(Debug)]
pub enum ClientEvent {
    /// Periodic server health and performance sample.
    Heartbeat {
        fps: SampleStats,
        frame_count: u64,
        loaded_projects: Vec<LoadedProject>,
        uptime_ms: u64,
        memory: Option<MemoryStats>,
    },
    /// Firmware/server log line carried by the protocol.
    Log { level: LogLevel, message: String },
    /// A response id arrived while another request id was expected.
    UncorrelatedResponse { response_id: u64, expected_id: u64 },
}

impl ClientEvent {
    pub fn from_unsolicited_message(message: WireServerMessage) -> Option<Self> {
        match message.msg {
            WireServerMsgBody::Heartbeat {
                fps,
                frame_count,
                loaded_projects,
                uptime_ms,
                memory,
            } => Some(Self::Heartbeat {
                fps,
                frame_count,
                loaded_projects,
                uptime_ms,
                memory,
            }),
            WireServerMsgBody::Log { level, message } => Some(Self::Log { level, message }),
            _ => None,
        }
    }
}
