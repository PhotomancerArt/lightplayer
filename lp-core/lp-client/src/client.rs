//! Standalone LpClient for communicating with LpServer
//!
//! Provides async methods for filesystem and project operations.

use anyhow::{Error, Result};
use lp_model::{
    ClientMessage, ClientRequest, LpPath, LpPathBuf, ServerMessage,
    project::{
        FrameId,
        api::{ApiNodeSpecifier, SerializableProjectResponse},
        handle::ProjectHandle,
    },
    server::{AvailableProject, FsResponse, LoadedProject, ServerMsgBody},
};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::transport::ClientTransport;

/// Standalone client for communicating with LpServer
///
/// Provides typed async methods for filesystem and project operations.
/// Uses an async `ClientTransport` for communication.
pub struct LpClient {
    /// Transport wrapped in Arc<Mutex> for sharing across async tasks
    transport: Arc<tokio::sync::Mutex<Box<dyn ClientTransport>>>,
    /// Next request ID to use
    next_request_id: Arc<AtomicU64>,
}

impl LpClient {
    /// Create a new LpClient with the given transport
    ///
    /// # Arguments
    ///
    /// * `transport` - The client transport (will be wrapped in Arc<Mutex>)
    ///
    /// # Returns
    ///
    /// * `Self` - The client
    #[allow(dead_code, reason = "Will be used in tests and other contexts")]
    pub fn new(transport: Box<dyn ClientTransport>) -> Self {
        Self {
            transport: Arc::new(tokio::sync::Mutex::new(transport)),
            next_request_id: Arc::new(AtomicU64::new(1)),
        }
    }

    /// Create a new LpClient with a shared transport
    ///
    /// # Arguments
    ///
    /// * `transport` - Shared transport (Arc<Mutex<Box<dyn ClientTransport>>>)
    ///
    /// # Returns
    ///
    /// * `Self` - The client
    pub fn new_shared(transport: Arc<tokio::sync::Mutex<Box<dyn ClientTransport>>>) -> Self {
        Self {
            transport,
            next_request_id: Arc::new(AtomicU64::new(1)),
        }
    }

    /// Send a request and wait for the response
    ///
    /// Helper method that generates a request ID, sends the request, and waits for the response.
    /// If the server returns an Error response, converts it to an Err.
    async fn send_request(&self, request: ClientRequest) -> Result<ServerMessage> {
        let id = self.next_request_id.fetch_add(1, Ordering::Relaxed);
        let msg = ClientMessage { id, msg: request };

        // Lock transport and send
        let mut transport = self.transport.lock().await;
        transport
            .send(msg)
            .await
            .map_err(|e| Error::msg(format!("Transport error: {e}")))?;

        // Wait for response
        let response = transport
            .receive()
            .await
            .map_err(|e| Error::msg(format!("Transport error: {e}")))?;

        // Check if server returned an error response
        if let ServerMsgBody::Error { error } = &response.msg {
            return Err(Error::msg(error.clone()));
        }

        Ok(response)
    }

    /// Read a file from the server filesystem
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the file (relative to server root)
    ///
    /// # Returns
    ///
    /// * `Ok(Vec<u8>)` if the file was read successfully
    /// * `Err` if reading failed or transport error occurred
    pub async fn fs_read(&self, path: &LpPath) -> Result<Vec<u8>> {
        let request = ClientRequest::Filesystem(lp_model::server::FsRequest::Read {
            path: path.to_path_buf(),
        });

        let response = self.send_request(request).await?;

        match response.msg {
            ServerMsgBody::Filesystem(FsResponse::Read { data, error, .. }) => {
                if let Some(err) = error {
                    return Err(Error::msg(format!("Server error: {err}")));
                }
                data.ok_or_else(|| Error::msg("No data in read response"))
            }
            _ => Err(Error::msg(format!(
                "Unexpected response type for fs_read: {:?}",
                response.msg
            ))),
        }
    }

    /// Write a file to the server filesystem
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the file (relative to server root)
    /// * `data` - File contents to write
    ///
    /// # Returns
    ///
    /// * `Ok(())` if the file was written successfully
    /// * `Err` if writing failed or transport error occurred
    pub async fn fs_write(&self, path: &LpPath, data: Vec<u8>) -> Result<()> {
        let request = ClientRequest::Filesystem(lp_model::server::FsRequest::Write {
            path: path.to_path_buf(),
            data,
        });

        let response = self.send_request(request).await?;

        match response.msg {
            ServerMsgBody::Filesystem(FsResponse::Write { error, .. }) => {
                if let Some(err) = error {
                    return Err(Error::msg(format!("Server error: {err}")));
                }
                Ok(())
            }
            _ => Err(Error::msg(format!(
                "Unexpected response type for fs_write: {:?}",
                response.msg
            ))),
        }
    }

    /// Delete a file from the server filesystem
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the file (relative to server root)
    ///
    /// # Returns
    ///
    /// * `Ok(())` if the file was deleted successfully
    /// * `Err` if deletion failed or transport error occurred
    pub async fn fs_delete_file(&self, path: &LpPath) -> Result<()> {
        let request = ClientRequest::Filesystem(lp_model::server::FsRequest::DeleteFile {
            path: path.to_path_buf(),
        });

        let response = self.send_request(request).await?;

        match response.msg {
            ServerMsgBody::Filesystem(FsResponse::DeleteFile { error, .. }) => {
                if let Some(err) = error {
                    return Err(Error::msg(format!("Server error: {err}")));
                }
                Ok(())
            }
            _ => Err(Error::msg(format!(
                "Unexpected response type for fs_delete_file: {:?}",
                response.msg
            ))),
        }
    }

    /// List directory contents from the server filesystem
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the directory (relative to server root)
    /// * `recursive` - Whether to list recursively
    ///
    /// # Returns
    ///
    /// * `Ok(Vec<LpPathBuf>)` - List of file/directory paths
    /// * `Err` if listing failed or transport error occurred
    pub async fn fs_list_dir(&self, path: &LpPath, recursive: bool) -> Result<Vec<LpPathBuf>> {
        let request = ClientRequest::Filesystem(lp_model::server::FsRequest::ListDir {
            path: path.to_path_buf(),
            recursive,
        });

        let response = self.send_request(request).await?;

        match response.msg {
            ServerMsgBody::Filesystem(FsResponse::ListDir { entries, error, .. }) => {
                if let Some(err) = error {
                    return Err(Error::msg(format!("Server error: {err}")));
                }
                Ok(entries)
            }
            _ => Err(Error::msg(format!(
                "Unexpected response type for fs_list_dir: {:?}",
                response.msg
            ))),
        }
    }

    /// Load a project on the server
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the project file (relative to server root)
    ///
    /// # Returns
    ///
    /// * `Ok(ProjectHandle)` if the project was loaded successfully
    /// * `Err` if loading failed or transport error occurred
    pub async fn project_load(&self, path: &str) -> Result<ProjectHandle> {
        let request = ClientRequest::LoadProject {
            path: path.to_string(),
        };

        let response = self.send_request(request).await?;

        match response.msg {
            ServerMsgBody::LoadProject { handle } => Ok(handle),
            _ => Err(Error::msg(format!(
                "Unexpected response type for project_load: {:?}",
                response.msg
            ))),
        }
    }

    /// Unload a project on the server
    ///
    /// # Arguments
    ///
    /// * `handle` - Project handle to unload
    ///
    /// # Returns
    ///
    /// * `Ok(())` if the project was unloaded successfully
    /// * `Err` if unloading failed or transport error occurred
    #[allow(dead_code, reason = "Will be used in future commands")]
    pub async fn project_unload(&self, handle: ProjectHandle) -> Result<()> {
        let request = ClientRequest::UnloadProject { handle };

        let response = self.send_request(request).await?;

        match response.msg {
            ServerMsgBody::UnloadProject => Ok(()),
            _ => Err(Error::msg(format!(
                "Unexpected response type for project_unload: {:?}",
                response.msg
            ))),
        }
    }

    /// Get project changes since a specific frame
    ///
    /// # Arguments
    ///
    /// * `handle` - Project handle
    /// * `since_frame` - Frame ID to get changes since (None for all changes)
    /// * `detail_specifier` - Which nodes to include in the response
    ///
    /// # Returns
    ///
    /// * `Ok(SerializableProjectResponse)` if the request succeeded
    /// * `Err` if the request failed or transport error occurred
    pub async fn project_sync_internal(
        &self,
        handle: ProjectHandle,
        since_frame: Option<FrameId>,
        detail_specifier: ApiNodeSpecifier,
    ) -> Result<SerializableProjectResponse> {
        // Use FrameId::default() if since_frame is None (get all changes)
        let since_frame = since_frame.unwrap_or_default();

        let request = ClientRequest::ProjectRequest {
            handle,
            request: lp_model::project::api::ProjectRequest::GetChanges {
                since_frame,
                detail_specifier,
            },
        };

        let response = self.send_request(request).await?;

        match response.msg {
            ServerMsgBody::ProjectRequest { response } => Ok(response),
            _ => Err(Error::msg(format!(
                "Unexpected response type for project_sync_internal: {:?}",
                response.msg
            ))),
        }
    }

    /// List available projects on the server filesystem
    ///
    /// # Returns
    ///
    /// * `Ok(Vec<AvailableProject>)` - List of available projects
    /// * `Err` if listing failed or transport error occurred
    #[allow(dead_code, reason = "Will be used in future commands")]
    pub async fn project_list_available(&self) -> Result<Vec<AvailableProject>> {
        let request = ClientRequest::ListAvailableProjects;

        let response = self.send_request(request).await?;

        match response.msg {
            ServerMsgBody::ListAvailableProjects { projects } => Ok(projects),
            _ => Err(Error::msg(format!(
                "Unexpected response type for project_list_available: {:?}",
                response.msg
            ))),
        }
    }

    /// List loaded projects on the server
    ///
    /// # Returns
    ///
    /// * `Ok(Vec<LoadedProject>)` - List of loaded projects
    /// * `Err` if listing failed or transport error occurred
    #[allow(dead_code, reason = "Will be used in future commands")]
    pub async fn project_list_loaded(&self) -> Result<Vec<LoadedProject>> {
        let request = ClientRequest::ListLoadedProjects;

        let response = self.send_request(request).await?;

        match response.msg {
            ServerMsgBody::ListLoadedProjects { projects } => Ok(projects),
            _ => Err(Error::msg(format!(
                "Unexpected response type for project_list_loaded: {:?}",
                response.msg
            ))),
        }
    }
}

/// Convert SerializableProjectResponse to ProjectResponse
///
/// This is a helper function for converting the serializable response
/// to the engine client's ProjectResponse type.
pub fn serializable_response_to_project_response(
    response: SerializableProjectResponse,
) -> Result<lp_model::project::api::ProjectResponse, Error> {
    match response {
        SerializableProjectResponse::GetChanges {
            current_frame,
            node_handles,
            node_changes,
            node_details,
            theoretical_fps,
        } => {
            use lp_model::project::api::{NodeDetail, ProjectResponse};
            use std::collections::BTreeMap;

            // Convert Vec<(NodeHandle, SerializableNodeDetail)> to BTreeMap<NodeHandle, NodeDetail>
            let mut node_details_map = BTreeMap::new();
            for (handle, serializable_detail) in node_details {
                let detail = match serializable_detail {
                    lp_model::project::api::SerializableNodeDetail::Texture {
                        path,
                        config,
                        state,
                    } => NodeDetail {
                        path,
                        config: Box::new(config),
                        state,
                    },
                    lp_model::project::api::SerializableNodeDetail::Shader {
                        path,
                        config,
                        state,
                    } => NodeDetail {
                        path,
                        config: Box::new(config),
                        state,
                    },
                    lp_model::project::api::SerializableNodeDetail::Output {
                        path,
                        config,
                        state,
                    } => NodeDetail {
                        path,
                        config: Box::new(config),
                        state,
                    },
                    lp_model::project::api::SerializableNodeDetail::Fixture {
                        path,
                        config,
                        state,
                    } => NodeDetail {
                        path,
                        config: Box::new(config),
                        state,
                    },
                };
                node_details_map.insert(handle, detail);
            }

            Ok(ProjectResponse::GetChanges {
                current_frame,
                node_handles,
                node_changes,
                node_details: node_details_map,
                theoretical_fps,
            })
        }
    }
}
