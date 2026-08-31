use crate::messages::ProjectReadEvent;
use crate::project::WireProjectHandle;
use crate::project_command::WireProjectCommandResponse;
use crate::server::fs_api::FsResponse;
use alloc::string::String;
use alloc::vec::Vec;
use lpc_model::LpPathBuf;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ServerMsgBody {
    /// Wire bootstrap: protocol version + build provenance + device uid.
    ///
    /// Sent unsolicited (id 0) as the first frame when the server loop
    /// starts serving, and as the response to [`crate::ClientRequest::Hello`].
    /// See [`crate::server::hello`] for the contract and version policy.
    Hello(crate::server::hello::ServerHello),
    /// Filesystem operation response
    Filesystem(FsResponse),
    /// Response to LoadProject
    LoadProject {
        handle: WireProjectHandle,
    },
    /// Response to UnloadProject
    UnloadProject,
    /// One batch of ordered project-read events.
    ///
    /// The transport batches events to a budget and the envelope sequences the
    /// batches (`seq`/`fin`). A read may span several `ProjectRead` messages
    /// under the same request id; the final one carries `fin == true` and (for a
    /// successful read) the `End`/`Error` event.
    ProjectRead {
        events: Vec<ProjectReadEvent>,
    },
    /// Response to ProjectCommand
    ProjectCommand {
        response: WireProjectCommandResponse,
    },
    /// Response to ListAvailableProjects
    ListAvailableProjects {
        projects: Vec<AvailableProject>,
    },
    /// Response to ListLoadedProjects
    ListLoadedProjects {
        projects: Vec<LoadedProject>,
    },
    /// Response to StopAllProjects
    StopAllProjects,
    /// Ack for SetLogLevel: the level has been applied globally.
    SetLogLevel,
    /// Ack for [`crate::ClientRequest::Reboot`]: the request was accepted and
    /// the device resets once this frame is on the wire.
    ///
    /// The ack is sent BEFORE the reset, not after it (there is no after):
    /// the embedder's reset hook fires only once the transport reports the
    /// frame written, so a client sees its answer and then the boot banner.
    /// An embedder with no reset hook answers [`ServerMsgBody::Error`]
    /// instead — a reboot that will not happen must never be acked.
    Reboot,

    Log {
        level: LogLevel,
        message: String,
    },
    /// Heartbeat message with server status
    ///
    /// Sent periodically (typically every second) to provide server status information.
    /// These are unsolicited messages (not responses to client requests) and use `id: 0`
    /// to indicate they are not correlated with any specific request.
    ///
    /// Clients can subscribe to these messages to monitor server health, FPS, and loaded
    /// projects, or ignore them if not needed.
    ///
    /// # Prior Art
    ///
    /// This follows the pattern established in `fw-esp32c6/src/tests/test_usb.rs` which sends
    /// heartbeat messages for debugging. This implementation makes heartbeat messages part
    /// of the formal protocol using proper `ServerMessage` types with `M!` prefix.
    ///
    /// # Fields
    ///
    /// * `fps` - FPS statistics (avg, sdev, min, max) over a recent window (e.g. 5s)
    /// * `frame_count` - Total frame count since server startup
    /// * `loaded_projects` - List of currently loaded projects with handles and paths
    /// * `uptime_ms` - Server uptime in milliseconds since startup
    /// * `memory` - Optional memory statistics (platform-dependent; ESP32 reports heap)
    Heartbeat {
        /// FPS statistics over the configured window (e.g. 5 seconds)
        fps: SampleStats,
        /// Total frame count since startup
        frame_count: u64,
        /// List of loaded projects
        loaded_projects: Vec<LoadedProject>,
        /// Uptime in milliseconds since server startup
        uptime_ms: u64,
        /// Optional memory statistics (ESP32 reports heap; absent on other platforms)
        #[serde(default)]
        memory: Option<MemoryStats>,
        /// Crash-recovery state (level, last crash, gated paths); absent on
        /// targets without a recovery region.
        #[serde(default)]
        recovery: Option<crate::server::RecoveryStatus>,
        /// Per-output-wire transmission counters; absent on targets whose
        /// output drivers keep no per-wire attribution (host server,
        /// single-core fallback boots).
        #[serde(default)]
        outputs: Option<Vec<crate::server::OutputWireStatus>>,
        /// Serial-link loss/corruption counters since boot; absent on
        /// targets without a lossy byte-stream link (host server, ws).
        /// Every drop the demux takes is counted here so silent loss has a
        /// wire-visible trace (2026-08-26 inbound-loss defect).
        #[serde(default)]
        link: Option<crate::server::LinkCounters>,
        /// Who this device is, repeated on every heartbeat.
        ///
        /// A client that attaches MID-STREAM never sees the boot hello, so
        /// without this it stays anonymous until its own `Hello` request is
        /// answered. Repeating identity on the unsolicited channel resolves
        /// such an attach passively, within one heartbeat period. Absent
        /// from embedders that know no identity (and from pre-R4 firmware,
        /// which is why it is optional rather than required).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        identity: Option<HeartbeatIdentity>,
    },
    /// Error response for any request type
    Error {
        error: String,
    },
}

/// Log severity carried by [`ServerMsgBody::Log`] frames and
/// [`crate::ClientRequest::SetLogLevel`] requests, lowest to highest.
///
/// There is deliberately no `Off` variant: the runtime log-level command can
/// lower output to `Error` but never fully silence the device.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AvailableProject {
    pub path: LpPathBuf,
}

/// Sample statistics over a time window (e.g. FPS over 5s).
///
/// Reusable for any scalar metric: avg, population standard deviation, min, max.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SampleStats {
    pub avg: f32,
    pub sdev: f32,
    pub min: f32,
    pub max: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadedProject {
    pub handle: WireProjectHandle,
    pub path: LpPathBuf,
}

/// Optional memory statistics (platform-dependent; ESP32 reports heap).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct MemoryStats {
    pub free_bytes: u32,
    pub used_bytes: u32,
    pub total_bytes: u32,
    /// Largest single allocatable block — the number that matters on a
    /// small fragmented arena (total-free can look healthy while every
    /// allocation over a few hundred bytes fails). Absent on targets that
    /// cannot probe it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub largest_free_block: Option<u32>,
    /// Times the retrying allocator saved an allocation that first failed
    /// (fragmentation pressure evidence). Absent where unsupported.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub oom_retry_saves: Option<u32>,
}

/// The identity a heartbeat announces: the same two facts the hello carries,
/// and no more.
///
/// Deliberately a SUBSET of [`crate::ServerHello`] rather than a second
/// identity vocabulary — `device_uid` is the stamped `dev…` uid
/// ([`crate::ServerHello::device_uid`]) and `base_mac` is the efuse address
/// ([`crate::HardwareFacts::base_mac`]). Build provenance and capabilities
/// stay off the heartbeat: they never change while a device runs, so
/// repeating them every second would only cost frame bytes.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct HeartbeatIdentity {
    /// See [`crate::ServerHello::device_uid`]. `None` = unstamped.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device_uid: Option<String>,
    /// See [`crate::HardwareFacts::base_mac`]. `None` from embedders with
    /// no efuse to read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_mac: Option<String>,
}

impl HeartbeatIdentity {
    /// Whether this announcement carries anything at all — an embedder that
    /// knows neither fact sends no identity rather than an empty record.
    pub fn is_empty(&self) -> bool {
        self.device_uid.is_none() && self.base_mac.is_none()
    }
}

/// Serial-link loss/corruption counters, monotonic since boot.
///
/// Attached to [`ServerMsgBody::Heartbeat`] so a lost or corrupted inbound
/// frame is never silent: the drop is counted at the site that takes it and
/// surfaces on the next heartbeat.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct LinkCounters {
    /// Newline-terminated `M!` lines whose JSON failed to parse (torn or
    /// spliced frames).
    #[serde(default)]
    pub parse_failures: u32,
    /// Hardware RX errors (overflow/parity/framing) that dropped a partial
    /// line.
    #[serde(default)]
    pub rx_errors: u32,
    /// Parsed `M!` lines dropped because the inbound queue was full.
    #[serde(default)]
    pub queue_full_drops: u32,
    /// Stale partial lines discarded at a session boundary (dead session
    /// remnants).
    #[serde(default)]
    pub stale_partial_flushes: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_level_trace_round_trips() {
        let json = crate::json::to_string(&LogLevel::Trace).unwrap();
        assert_eq!(json, "\"Trace\"");
        let level: LogLevel = crate::json::from_str(&json).unwrap();
        assert_eq!(level, LogLevel::Trace);
    }

    #[test]
    fn set_log_level_request_round_trips() {
        let request = crate::ClientRequest::SetLogLevel {
            level: LogLevel::Debug,
        };
        let json = crate::json::to_string(&request).unwrap();
        let deserialized: crate::ClientRequest = crate::json::from_str(&json).unwrap();
        assert!(matches!(
            deserialized,
            crate::ClientRequest::SetLogLevel {
                level: LogLevel::Debug
            }
        ));
    }

    #[test]
    fn set_log_level_ack_round_trips() {
        let json = crate::json::to_string(&ServerMsgBody::SetLogLevel).unwrap();
        let deserialized: ServerMsgBody = crate::json::from_str(&json).unwrap();
        assert!(matches!(deserialized, ServerMsgBody::SetLogLevel));
    }
}
