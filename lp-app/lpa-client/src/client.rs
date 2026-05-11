//! Standalone LpClient for communicating with LpServer
//!
//! Provides async methods for filesystem and project operations.

use anyhow::{Error, Result};
use lpc_model::{LpPath, LpPathBuf};
use lpc_wire::{
    ResourceSummarySpecifier, RuntimeBufferPayloadSpecifier, VisualProductPayloadRequest,
    VisualProductPayloadSpecifier, WireProjectHandle as ProjectHandle, WireProjectRequest,
    WireServerMessage,
    message::{ClientMessage, ClientRequest},
    server::{AvailableProject, FsResponse, LoadedProject, ServerMsgBody},
};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tokio::time::timeout;

use crate::transport::ClientTransport;

/// Resource interest options reserved for M3 canonical project sync.
///
/// The server does not retain subscription state between calls.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectGetChangesOptions {
    pub resource_summary_specifier: ResourceSummarySpecifier,
    pub runtime_buffer_payload_specifier: RuntimeBufferPayloadSpecifier,
    pub visual_product_payload_request: VisualProductPayloadRequest,
}

impl Default for ProjectGetChangesOptions {
    fn default() -> Self {
        Self {
            resource_summary_specifier: ResourceSummarySpecifier::default(),
            runtime_buffer_payload_specifier: RuntimeBufferPayloadSpecifier::default(),
            visual_product_payload_request: VisualProductPayloadRequest::default(),
        }
    }
}

impl ProjectGetChangesOptions {
    /// Local dev UI (`just demo`): summaries plus full buffer and visual-product payloads.
    pub fn dev_demo_full_resources() -> Self {
        Self {
            resource_summary_specifier: ResourceSummarySpecifier::All,
            runtime_buffer_payload_specifier: RuntimeBufferPayloadSpecifier::All,
            visual_product_payload_request: VisualProductPayloadRequest {
                specifier: VisualProductPayloadSpecifier::All,
                ..Default::default()
            },
        }
    }
}

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
    /// Correlates messages by ID to handle heartbeats and other interstitial messages.
    /// If the server returns an Error response, converts it to an Err.
    async fn send_request(&self, request: ClientRequest) -> Result<WireServerMessage> {
        let id = self.next_request_id.fetch_add(1, Ordering::Relaxed);
        let msg = ClientMessage { id, msg: request };

        // Lock transport and send
        let mut transport = self.transport.lock().await;
        transport
            .send(msg)
            .await
            .map_err(|e| Error::msg(format!("Transport error: {e}")))?;

        // Wait for response with matching ID (with timeout to avoid deadlock if server
        // never receives our request, e.g. host->device serial direction broken)
        // ESP32 compile and other heavy ops can take 30s+, so allow 60s
        const REQUEST_TIMEOUT: Duration = Duration::from_secs(60);

        let wait_response = async {
            loop {
                let response = transport
                    .receive()
                    .await
                    .map_err(|e| Error::msg(format!("Transport error: {e}")))?;

                if response.id == id {
                    if let ServerMsgBody::Error { error } = &response.msg {
                        return Err(Error::msg(error.clone()));
                    }
                    return Ok(response);
                }

                if response.id == 0 {
                    if let ServerMsgBody::Heartbeat {
                        fps,
                        frame_count,
                        loaded_projects,
                        uptime_ms,
                        memory,
                    } = &response.msg
                    {
                        Self::display_heartbeat(
                            fps,
                            *frame_count,
                            loaded_projects.as_slice(),
                            *uptime_ms,
                            memory,
                        );
                    }
                    continue;
                }

                log::warn!(
                    "Received non-correlated message (id: {}, expected: {})",
                    response.id,
                    id
                );
            }
        };

        match timeout(REQUEST_TIMEOUT, wait_response).await {
            Ok(Ok(response)) => Ok(response),
            Ok(Err(e)) => Err(e),
            Err(_) => Err(Error::msg(
                "Request timed out - server may not be receiving messages (check host->device serial)",
            )),
        }
    }

    /// Display heartbeat with colors and memory bar chart
    fn display_heartbeat(
        fps: &lpc_wire::server::SampleStats,
        _frame_count: u64,
        loaded_projects: &[lpc_wire::server::LoadedProject],
        uptime_ms: u64,
        memory: &Option<lpc_wire::server::MemoryStats>,
    ) {
        const BOLD: &str = "\x1b[1m";
        const DIM: &str = "\x1b[90m";
        const CYAN: &str = "\x1b[36m";
        const GREEN: &str = "\x1b[32m";
        const YELLOW: &str = "\x1b[33m";
        const RED: &str = "\x1b[31m";
        const RESET: &str = "\x1b[0m";

        let uptime_secs = uptime_ms as f64 / 1000.0;
        let _projects_str = if loaded_projects.is_empty() {
            format!("{DIM}none{RESET}")
        } else {
            loaded_projects
                .iter()
                .map(|p| {
                    p.path
                        .file_name()
                        .map(|n| n.to_string())
                        .unwrap_or_else(|| p.path.as_str().to_string())
                })
                .collect::<Vec<_>>()
                .join(", ")
        };

        let fps_color = if fps.avg >= 50.0 {
            GREEN
        } else if fps.avg >= 20.0 {
            YELLOW
        } else {
            RED
        };

        let mut line = format!(
            "{BOLD}{CYAN}[server]{RESET} {fps_color}FPS {:.0}{RESET} avg (σ{:.1} {:.0}-{:.0}) {DIM}|{RESET} \
             {DIM}Uptime {uptime_secs:.1}s{RESET}",
            fps.avg, fps.sdev, fps.min, fps.max
        );

        if let Some(mem) = memory {
            let total = mem.total_bytes as f32;
            let used_pct = if total > 0.0 {
                (mem.used_bytes as f32 / total) * 100.0
            } else {
                0.0
            };
            let free_pct = 100.0 - used_pct;

            const BAR_WIDTH: usize = 16;
            let filled = if total > 0.0 {
                ((mem.used_bytes as f32 / total) * BAR_WIDTH as f32).round() as usize
            } else {
                0
            };
            let filled = filled.min(BAR_WIDTH);

            let (bar_fill_color, bar_empty_color) = if free_pct >= 40.0 {
                (GREEN, DIM)
            } else if free_pct >= 15.0 {
                (YELLOW, DIM)
            } else {
                (RED, DIM)
            };

            let bar: String = (0..BAR_WIDTH)
                .map(|i| {
                    if i < filled {
                        format!("{bar_fill_color}█{RESET}")
                    } else {
                        format!("{bar_empty_color}░{RESET}")
                    }
                })
                .collect();

            let free_kb = mem.free_bytes / 1024;
            let total_kb = mem.total_bytes / 1024;

            line.push_str(&format!(
                " {DIM}|{RESET} [{bar}] {bar_fill_color}{used_pct:.0}%{RESET} ({free_kb}k free / {total_kb}k total)"
            ));
        }

        eprintln!("{line}");
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
        let request = ClientRequest::Filesystem(lpc_wire::server::FsRequest::Read {
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
        let request = ClientRequest::Filesystem(lpc_wire::server::FsRequest::Write {
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
        let request = ClientRequest::Filesystem(lpc_wire::server::FsRequest::DeleteFile {
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
        let request = ClientRequest::Filesystem(lpc_wire::server::FsRequest::ListDir {
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

    /// Project sync is disabled until M3 canonical project sync is rebuilt.
    pub async fn project_sync_disabled(&self, handle: ProjectHandle) -> Result<()> {
        let request = ClientRequest::ProjectRequest {
            handle,
            request: WireProjectRequest::SyncDisabled,
        };

        let _ = self.send_request(request).await?;
        Err(Error::msg(
            "project sync is disabled until M3 canonical project sync",
        ))
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

    /// Stop all loaded projects on the server
    ///
    /// # Returns
    ///
    /// * `Ok(())` if all projects were stopped successfully
    /// * `Err` if the request failed or transport error occurred
    pub async fn stop_all_projects(&self) -> Result<()> {
        let request = ClientRequest::StopAllProjects;

        let response = self.send_request(request).await?;

        match response.msg {
            ServerMsgBody::StopAllProjects => Ok(()),
            _ => Err(Error::msg(format!(
                "Unexpected response type for stop_all_projects: {:?}",
                response.msg
            ))),
        }
    }
}
