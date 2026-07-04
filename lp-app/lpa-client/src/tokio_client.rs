//! Tokio host adapter for the portable LightPlayer client protocol.
//!
//! `TokioLpClient` preserves the existing CLI/native ergonomics: cloneable
//! shared transport, request timeout, and heartbeat/log rendering. Protocol
//! state and deploy ordering still come from the portable modules so host and
//! browser paths do not diverge semantically.

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Error, Result};
use lpc_model::{LpPath, LpPathBuf};
use lpc_wire::server::api::LogLevel;
use lpc_wire::{
    ClientMessage, ClientRequest, FsRequest, ProjectReadEvent, ProjectReadRequest,
    WireOverlayCommitRequest, WireOverlayCommitResponse, WireOverlayMutationRequest,
    WireOverlayMutationResponse, WireOverlayReadRequest, WireOverlayReadResponse,
    WireProjectCommand, WireProjectCommandResponse, WireProjectHandle,
    WireProjectInventoryReadRequest, WireProjectInventoryReadResponse, WireServerMessage,
    WireServerMsgBody,
    server::{AvailableProject, FsResponse, LoadedProject},
};
use tokio::sync::Mutex;
use tokio::time::timeout;

use crate::client::ClientOutcome;
use crate::client_error::ClientError;
use crate::client_event::ClientEvent;
use crate::client_io::ClientIo;
use crate::project_deploy::{
    ProjectDeployFile, project_deploy_requests, project_write_requests,
    validate_project_deploy_response,
};
use crate::protocol_session::{ProtocolSession, ResponseDisposition};
use crate::pull_loop::{NeverCancel, ProgressDeadline, PullOutcome, run_project_read};
use crate::transport::ClientTransport;

pub type SharedClientTransport = Arc<Mutex<Box<dyn ClientTransport>>>;

/// `ClientIo` implementation backed by a shared Tokio transport.
#[derive(Clone)]
pub struct TokioClientIo {
    transport: SharedClientTransport,
}

impl TokioClientIo {
    pub fn new(transport: Box<dyn ClientTransport>) -> Self {
        Self {
            transport: Arc::new(Mutex::new(transport)),
        }
    }

    pub fn new_shared(transport: SharedClientTransport) -> Self {
        Self { transport }
    }

    pub fn shared_transport(&self) -> SharedClientTransport {
        Arc::clone(&self.transport)
    }
}

#[async_trait::async_trait(?Send)]
impl ClientIo for TokioClientIo {
    async fn send(&mut self, msg: lpc_wire::ClientMessage) -> Result<(), lpc_wire::TransportError> {
        self.transport.lock().await.send(msg).await
    }

    async fn receive(&mut self) -> Result<lpc_wire::WireServerMessage, lpc_wire::TransportError> {
        self.transport.lock().await.receive().await
    }

    async fn close(&mut self) -> Result<(), lpc_wire::TransportError> {
        self.transport.lock().await.close().await
    }
}

/// Cloneable host client wrapper with timeouts and optional heartbeat display.
#[derive(Clone)]
pub struct TokioLpClient {
    state: Arc<Mutex<TokioLpClientState>>,
    request_timeout: Duration,
    display_heartbeats: bool,
}

struct TokioLpClientState {
    transport: SharedClientTransport,
    protocol: ProtocolSession,
}

impl TokioLpClient {
    pub fn new(transport: Box<dyn ClientTransport>) -> Self {
        Self::new_shared(Arc::new(Mutex::new(transport)))
    }

    pub fn new_shared(transport: SharedClientTransport) -> Self {
        Self {
            state: Arc::new(Mutex::new(TokioLpClientState {
                transport,
                protocol: ProtocolSession::new(),
            })),
            request_timeout: Duration::from_secs(60),
            display_heartbeats: true,
        }
    }

    pub fn from_io(io: TokioClientIo) -> Self {
        Self::new_shared(io.shared_transport())
    }

    pub fn with_heartbeat_display(mut self, display_heartbeats: bool) -> Self {
        self.display_heartbeats = display_heartbeats;
        self
    }

    pub async fn send_request(
        &self,
        request: ClientRequest,
    ) -> Result<ClientOutcome<WireServerMessage>> {
        let run = self.send_request_inner(request);
        let outcome = match timeout(self.request_timeout, run).await {
            Ok(Ok(outcome)) => outcome,
            Ok(Err(error)) => return Err(error),
            Err(_) => {
                return Err(Error::msg(
                    "Request timed out - server may not be receiving messages (check host->device serial)",
                ));
            }
        };
        self.handle_events(&outcome.events);
        Ok(outcome)
    }

    async fn send_request_inner(
        &self,
        request: ClientRequest,
    ) -> Result<ClientOutcome<WireServerMessage>> {
        let mut state = self.state.lock().await;
        let request_id = state.protocol.next_request_id();
        let mut transport = state.transport.lock().await;
        transport
            .send(ClientMessage {
                id: request_id,
                msg: request,
            })
            .await
            .map_err(|error| Error::msg(format!("Transport error: {error}")))?;

        let mut events = Vec::new();
        loop {
            let response = transport
                .receive()
                .await
                .map_err(|error| Error::msg(format!("Transport error: {error}")))?;
            match state.protocol.response_disposition(&response, request_id) {
                ResponseDisposition::Matched => {
                    if let WireServerMsgBody::Error { error } = &response.msg {
                        return Err(Error::msg(error.clone()));
                    }
                    return Ok(ClientOutcome::new(response, events));
                }
                ResponseDisposition::Unsolicited => {
                    if let Some(event) = ClientEvent::from_unsolicited_message(response) {
                        events.push(event);
                    }
                }
                ResponseDisposition::Uncorrelated {
                    response_id,
                    expected_id,
                } => events.push(ClientEvent::UncorrelatedResponse {
                    response_id,
                    expected_id,
                }),
            }
        }
    }

    pub async fn fs_read(&self, path: &LpPath) -> Result<Vec<u8>> {
        let response = self
            .send_request(ClientRequest::Filesystem(FsRequest::Read {
                path: path.to_path_buf(),
            }))
            .await?;
        match response.value.msg {
            WireServerMsgBody::Filesystem(FsResponse::Read { data, error, .. }) => {
                if let Some(error) = error {
                    return Err(Error::msg(format!("Server error: {error}")));
                }
                data.ok_or_else(|| Error::msg("No data in read response"))
            }
            other => Err(unexpected_response("fs_read", other)),
        }
    }

    pub async fn fs_write(&self, path: &LpPath, data: Vec<u8>) -> Result<()> {
        let response = self
            .send_request(ClientRequest::Filesystem(FsRequest::Write {
                path: path.to_path_buf(),
                data,
            }))
            .await?;
        match response.value.msg {
            WireServerMsgBody::Filesystem(FsResponse::Write { error, .. }) => {
                if let Some(error) = error {
                    return Err(Error::msg(format!("Server error: {error}")));
                }
                Ok(())
            }
            other => Err(unexpected_response("fs_write", other)),
        }
    }

    pub async fn fs_delete_file(&self, path: &LpPath) -> Result<()> {
        let response = self
            .send_request(ClientRequest::Filesystem(FsRequest::DeleteFile {
                path: path.to_path_buf(),
            }))
            .await?;
        match response.value.msg {
            WireServerMsgBody::Filesystem(FsResponse::DeleteFile { error, .. }) => {
                if let Some(error) = error {
                    return Err(Error::msg(format!("Server error: {error}")));
                }
                Ok(())
            }
            other => Err(unexpected_response("fs_delete_file", other)),
        }
    }

    pub async fn fs_list_dir(&self, path: &LpPath, recursive: bool) -> Result<Vec<LpPathBuf>> {
        let response = self
            .send_request(ClientRequest::Filesystem(FsRequest::ListDir {
                path: path.to_path_buf(),
                recursive,
            }))
            .await?;
        match response.value.msg {
            WireServerMsgBody::Filesystem(FsResponse::ListDir { entries, error, .. }) => {
                if let Some(error) = error {
                    return Err(Error::msg(format!("Server error: {error}")));
                }
                Ok(entries)
            }
            other => Err(unexpected_response("fs_list_dir", other)),
        }
    }

    pub async fn project_load(&self, path: &str) -> Result<WireProjectHandle> {
        let response = self
            .send_request(ClientRequest::LoadProject {
                path: path.to_string(),
            })
            .await?;
        match response.value.msg {
            WireServerMsgBody::LoadProject { handle } => Ok(handle),
            other => Err(unexpected_response("project_load", other)),
        }
    }

    pub async fn project_unload(&self, handle: WireProjectHandle) -> Result<()> {
        let response = self
            .send_request(ClientRequest::UnloadProject { handle })
            .await?;
        match response.value.msg {
            WireServerMsgBody::UnloadProject => Ok(()),
            other => Err(unexpected_response("project_unload", other)),
        }
    }

    pub async fn project_read(
        &self,
        handle: WireProjectHandle,
        read: ProjectReadRequest,
    ) -> Result<Vec<ProjectReadEvent>> {
        let run = self.project_read_inner(handle, read);
        let outcome = match timeout(self.request_timeout, run).await {
            Ok(Ok(outcome)) => outcome,
            Ok(Err(error)) => return Err(error),
            Err(_) => {
                return Err(Error::msg(
                    "Request timed out - server may not be receiving messages (check host->device serial)",
                ));
            }
        };
        self.handle_events(&outcome.events);
        Ok(outcome.value)
    }

    async fn project_read_inner(
        &self,
        handle: WireProjectHandle,
        read: ProjectReadRequest,
    ) -> Result<ClientOutcome<Vec<ProjectReadEvent>>> {
        let mut state = self.state.lock().await;
        let state = &mut *state;
        let mut transport = state.transport.lock().await;

        // The shared pull loop owns the send/receive/collect state machine; the
        // Tokio wrapper only supplies the locked transport (wrapped as a
        // `ClientIo`) and its protocol session. The native request timeout is
        // still applied by the outer `timeout(...)` in `project_read`, so the
        // pull loop's own deadline never fires here and cancellation is never
        // requested — a single timeout owner on the native path.
        let mut io = LockedTransportIo {
            transport: &mut **transport,
        };
        let deadline =
            ProgressDeadline::new(Duration::MAX, |_budget| core::future::pending::<()>());

        match run_project_read(
            &mut io,
            &mut state.protocol,
            handle,
            read,
            deadline,
            &NeverCancel,
        )
        .await
        {
            PullOutcome::Completed { events, observed } => Ok(ClientOutcome::new(events, observed)),
            PullOutcome::Failed(error) => Err(client_error_to_anyhow(error)),
            PullOutcome::TimedOut | PullOutcome::Cancelled => {
                Err(Error::msg("project read ended without completing"))
            }
        }
    }

    pub async fn project_read_default_debug(
        &self,
        handle: WireProjectHandle,
    ) -> Result<Vec<ProjectReadEvent>> {
        self.project_read(handle, ProjectReadRequest::default_debug(None))
            .await
    }

    pub async fn project_command(
        &self,
        handle: WireProjectHandle,
        command: WireProjectCommand,
    ) -> Result<WireProjectCommandResponse> {
        let response = self
            .send_request(ClientRequest::ProjectCommand { handle, command })
            .await?;
        match response.value.msg {
            WireServerMsgBody::ProjectCommand { response } => Ok(response),
            other => Err(unexpected_response("project_command", other)),
        }
    }

    pub async fn project_overlay_read(
        &self,
        handle: WireProjectHandle,
    ) -> Result<WireOverlayReadResponse> {
        match self
            .project_command(
                handle,
                WireProjectCommand::ReadOverlay {
                    request: WireOverlayReadRequest,
                },
            )
            .await?
        {
            WireProjectCommandResponse::ReadOverlay { response } => Ok(response),
            other => Err(unexpected_response("project_overlay_read", other)),
        }
    }

    pub async fn project_overlay_mutate(
        &self,
        handle: WireProjectHandle,
        request: WireOverlayMutationRequest,
    ) -> Result<WireOverlayMutationResponse> {
        match self
            .project_command(handle, WireProjectCommand::MutateOverlay { request })
            .await?
        {
            WireProjectCommandResponse::MutateOverlay { response } => Ok(response),
            other => Err(unexpected_response("project_overlay_mutate", other)),
        }
    }

    pub async fn project_overlay_commit(
        &self,
        handle: WireProjectHandle,
    ) -> Result<WireOverlayCommitResponse> {
        match self
            .project_command(
                handle,
                WireProjectCommand::CommitOverlay {
                    request: WireOverlayCommitRequest,
                },
            )
            .await?
        {
            WireProjectCommandResponse::CommitOverlay { response } => Ok(response),
            other => Err(unexpected_response("project_overlay_commit", other)),
        }
    }

    pub async fn project_inventory_read(
        &self,
        handle: WireProjectHandle,
    ) -> Result<WireProjectInventoryReadResponse> {
        match self
            .project_command(
                handle,
                WireProjectCommand::ReadInventory {
                    request: WireProjectInventoryReadRequest,
                },
            )
            .await?
        {
            WireProjectCommandResponse::ReadInventory { response } => Ok(response),
            other => Err(unexpected_response("project_inventory_read", other)),
        }
    }

    pub async fn project_list_available(&self) -> Result<Vec<AvailableProject>> {
        let response = self
            .send_request(ClientRequest::ListAvailableProjects)
            .await?;
        match response.value.msg {
            WireServerMsgBody::ListAvailableProjects { projects } => Ok(projects),
            other => Err(unexpected_response("project_list_available", other)),
        }
    }

    pub async fn project_list_loaded(&self) -> Result<Vec<LoadedProject>> {
        let response = self.send_request(ClientRequest::ListLoadedProjects).await?;
        match response.value.msg {
            WireServerMsgBody::ListLoadedProjects { projects } => Ok(projects),
            other => Err(unexpected_response("project_list_loaded", other)),
        }
    }

    pub async fn stop_all_projects(&self) -> Result<()> {
        let response = self.send_request(ClientRequest::StopAllProjects).await?;
        match response.value.msg {
            WireServerMsgBody::StopAllProjects => Ok(()),
            other => Err(unexpected_response("stop_all_projects", other)),
        }
    }

    pub async fn push_project_files(
        &self,
        project_id: &str,
        files: impl IntoIterator<Item = ProjectDeployFile>,
    ) -> Result<()> {
        for request in project_write_requests(project_id, files) {
            let response = self.send_request(request.clone()).await?;
            validate_project_deploy_response(&request, &response.value.msg)
                .map_err(|error| Error::msg(error.to_string()))?;
        }
        Ok(())
    }

    pub async fn deploy_project_files(
        &self,
        project_id: &str,
        files: impl IntoIterator<Item = ProjectDeployFile>,
    ) -> Result<WireProjectHandle> {
        let mut handle = None;
        for request in project_deploy_requests(project_id, files) {
            let response = self.send_request(request.clone()).await?;
            handle = validate_project_deploy_response(&request, &response.value.msg)
                .map_err(|error| Error::msg(error.to_string()))?
                .or(handle);
        }
        handle.ok_or_else(|| Error::msg("project deploy did not return a project handle"))
    }

    pub async fn close(&self) -> Result<()> {
        let state = self.state.lock().await;
        let mut transport = state.transport.lock().await;
        transport
            .close()
            .await
            .map_err(|error| Error::msg(error.to_string()))
    }

    fn handle_events(&self, events: &[ClientEvent]) {
        for event in events {
            match event {
                ClientEvent::Heartbeat {
                    fps,
                    frame_count,
                    loaded_projects,
                    uptime_ms,
                    memory,
                } if self.display_heartbeats => {
                    display_heartbeat(
                        fps,
                        *frame_count,
                        loaded_projects.as_slice(),
                        *uptime_ms,
                        memory,
                    );
                }
                ClientEvent::Log { level, message } => {
                    log::log!(server_log_level(level), "[server] {message}");
                }
                ClientEvent::UncorrelatedResponse {
                    response_id,
                    expected_id,
                } => {
                    log::warn!(
                        "Received non-correlated message (id: {response_id}, expected: {expected_id})"
                    );
                }
                _ => {}
            }
        }
    }
}

fn unexpected_response(operation: &'static str, response: impl std::fmt::Debug) -> Error {
    Error::msg(format!(
        "Unexpected response type for {operation}: {response:?}"
    ))
}

/// Render a pull-loop [`ClientError`] into the anyhow surface the Tokio wrapper
/// exposes, preserving the message shapes the open-coded loop produced.
fn client_error_to_anyhow(error: ClientError) -> Error {
    match error {
        ClientError::Transport(message) => Error::msg(format!("Transport error: {message}")),
        ClientError::Server(message) => Error::msg(message),
        ClientError::Protocol(message) => {
            Error::msg(format!("Project read protocol error: {message}"))
        }
        ClientError::UnexpectedResponse { response, .. } => Error::msg(format!(
            "Unexpected response type for project_read: {response}"
        )),
    }
}

/// Adapts a locked [`ClientTransport`] guard into a [`ClientIo`] so the shared
/// pull loop can drive it. Native transports are `Send`, but the pull loop only
/// needs `?Send`, so this stays a thin forwarder over the borrowed transport.
struct LockedTransportIo<'a> {
    transport: &'a mut dyn ClientTransport,
}

#[async_trait::async_trait(?Send)]
impl ClientIo for LockedTransportIo<'_> {
    async fn send(&mut self, msg: ClientMessage) -> Result<(), lpc_wire::TransportError> {
        self.transport.send(msg).await
    }

    async fn receive(&mut self) -> Result<WireServerMessage, lpc_wire::TransportError> {
        self.transport.receive().await
    }

    async fn close(&mut self) -> Result<(), lpc_wire::TransportError> {
        self.transport.close().await
    }
}

fn server_log_level(level: &LogLevel) -> log::Level {
    match level {
        LogLevel::Debug => log::Level::Debug,
        LogLevel::Info => log::Level::Info,
        LogLevel::Warn => log::Level::Warn,
        LogLevel::Error => log::Level::Error,
    }
}

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
