use crate::messages::{ProjectReadEvent, ProjectReadRequest};
use crate::project::WireProjectHandle;
use crate::project_command::{WireProjectCommand, WireProjectCommandResponse};
use crate::server::fs_api::{FsRequest, FsResponse};
use alloc::string::String;
use alloc::vec::Vec;
use lpc_model::LpPathBuf;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ClientMsgBody {
    /// Filesystem operation request
    Filesystem(FsRequest),
    /// Load a project
    LoadProject { path: LpPathBuf },
    /// Unload a project
    UnloadProject { handle: WireProjectHandle },
    /// Project read request that expects project-read frames.
    ProjectRead {
        handle: WireProjectHandle,
        request: ProjectReadRequest,
    },
    /// Project-specific command request.
    ProjectCommand {
        handle: WireProjectHandle,
        command: WireProjectCommand,
    },
    /// List available projects
    ListAvailableProjects,
    /// List loaded projects
    ListLoadedProjects,
    /// Set the server/device global log level at runtime.
    ///
    /// Applies process-globally via the `log` crate on whichever platform
    /// serves the protocol (ESP32, emulator, browser worker, host). Not
    /// persisted: the device reverts to its logger-init default (Info) on
    /// reboot. There is deliberately no `Off` — the client can never turn
    /// the device fully silent.
    SetLogLevel { level: LogLevel },
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ServerMsgBody {
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
    /// This follows the pattern established in `fw-esp32/src/tests/test_usb.rs` which sends
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
    },
    /// Error response for any request type
    Error {
        error: String,
    },
}

/// Log severity carried by [`ServerMsgBody::Log`] frames and
/// [`ClientMsgBody::SetLogLevel`] requests, lowest to highest.
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
        let request = ClientMsgBody::SetLogLevel {
            level: LogLevel::Debug,
        };
        let json = crate::json::to_string(&request).unwrap();
        let deserialized: ClientMsgBody = crate::json::from_str(&json).unwrap();
        assert!(matches!(
            deserialized,
            ClientMsgBody::SetLogLevel {
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
