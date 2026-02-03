use crate::LpPathBuf;
use crate::project::{ProjectHandle, ProjectRequest, api::SerializableProjectResponse};
use crate::server::fs_api::{FsRequest, FsResponse};
use alloc::string::String;
use alloc::vec::Vec;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "requestType", rename_all = "camelCase")]
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
#[serde(tag = "responseType", rename_all = "camelCase")]
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

    Log {
        level: LogLevel,
        message: String,
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
