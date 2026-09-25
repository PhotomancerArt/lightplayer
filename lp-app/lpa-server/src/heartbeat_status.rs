//! The embedder-measured half of a heartbeat.

extern crate alloc;

use alloc::vec::Vec;
use lpc_wire::server::{LinkCounters, MemoryStats, OutputWireStatus, RecoveryStatus, SampleStats};

/// What the embedder's loop measured for one heartbeat — everything the
/// server cannot know itself. The server adds its own half (loaded projects,
/// identity) and decides, per link, how much of either a link may see
/// ([`crate::LpServer::heartbeats`]).
#[derive(Debug, Clone)]
pub struct HeartbeatStatus {
    pub fps: SampleStats,
    pub frame_count: u64,
    pub uptime_ms: u64,
    pub memory: Option<MemoryStats>,
    pub recovery: Option<RecoveryStatus>,
    pub outputs: Option<Vec<OutputWireStatus>>,
    pub link: Option<LinkCounters>,
}
