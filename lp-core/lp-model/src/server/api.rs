use crate::LpPathBuf;
use crate::project::{ProjectHandle, ProjectRequest, api::SerializableProjectResponse};
use crate::server::fs_api::{FsRequest, FsResponse};
use alloc::string::String;
use alloc::vec::Vec;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ClientMsgBody {
    /// Filesystem operation request
    Filesystem(FsRequest),
    /// Load a project
    LoadProject { path: LpPathBuf },
    /// Unload a project
    UnloadProject { handle: ProjectHandle },
    /// Project-specific request
    ProjectRequest {
        handle: ProjectHandle,
        request: ProjectRequest,
    },
    /// List available projects
    ListAvailableProjects,
    /// List loaded projects
    ListLoadedProjects,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ServerMsgBody {
    /// Filesystem operation response
    Filesystem(FsResponse),
    /// Response to LoadProject
    LoadProject {
        handle: ProjectHandle,
    },
    /// Response to UnloadProject
    UnloadProject,
    /// Response to ProjectRequest
    ///
    /// Uses SerializableProjectResponse which wraps NodeDetail in SerializableNodeDetail
    /// to enable serialization of trait objects.
    ProjectRequest {
        response: SerializableProjectResponse,
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
    /// * `fps` - Current frames per second (calculated over recent frames)
    /// * `frame_count` - Total frame count since server startup
    /// * `loaded_projects` - List of currently loaded projects with handles and paths
    /// * `uptime_ms` - Server uptime in milliseconds since startup
    Heartbeat {
        /// Current FPS (frames per second)
        fps: u32,
        /// Total frame count since startup
        frame_count: u64,
        /// List of loaded projects
        loaded_projects: Vec<LoadedProject>,
        /// Uptime in milliseconds since server startup
        uptime_ms: u64,
    },
    /// Error response for any request type
    Error {
        error: String,
    },
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AvailableProject {
    pub path: LpPathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadedProject {
    pub handle: ProjectHandle,
    pub path: LpPathBuf,
}
