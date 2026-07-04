//! Portable LightPlayer server protocol client.

use lpc_model::{LpPath, LpPathBuf};
use lpc_wire::{
    ClientMessage, ClientRequest, FsRequest, ProjectReadRequest, ProjectReadResponse,
    WireOverlayCommitRequest, WireOverlayCommitResponse, WireOverlayMutationRequest,
    WireOverlayMutationResponse, WireOverlayReadRequest, WireOverlayReadResponse,
    WireProjectCommand, WireProjectCommandResponse, WireProjectHandle,
    WireProjectInventoryReadRequest, WireProjectInventoryReadResponse, WireServerMessage,
    WireServerMsgBody,
    server::{AvailableProject, FsResponse, LoadedProject},
};

use crate::client_error::{ClientError, ClientResult};
use crate::client_event::ClientEvent;
use crate::client_io::ClientIo;
use crate::project_deploy::{
    ProjectDeployFile, project_deploy_requests, project_write_requests,
    validate_project_deploy_response,
};
use crate::project_read_stream::{
    ProjectReadStream, ProjectReadStreamError, ProjectReadStreamStep,
};
use crate::protocol_session::{ProtocolSession, ResponseDisposition};

/// Result value plus protocol events observed while waiting for it.
#[derive(Debug)]
pub struct ClientOutcome<T> {
    pub value: T,
    pub events: Vec<ClientEvent>,
}

impl<T> ClientOutcome<T> {
    pub fn new(value: T, events: Vec<ClientEvent>) -> Self {
        Self { value, events }
    }

    pub fn map<U>(self, f: impl FnOnce(T) -> U) -> ClientOutcome<U> {
        ClientOutcome {
            value: f(self.value),
            events: self.events,
        }
    }

    pub fn into_value(self) -> T {
        self.value
    }
}

/// Runtime-neutral client for communicating with `LpServer`.
///
/// The core client owns request ids, response correlation, server errors, and
/// typed server operations. It does not require Tokio or `Send`; host/native
/// code should use `TokioLpClient` when it wants sharing, timeouts, and current
/// CLI ergonomics.
pub struct LpClient<Io> {
    io: Io,
    protocol: ProtocolSession,
}

impl<Io> LpClient<Io>
where
    Io: ClientIo,
{
    pub fn new(io: Io) -> Self {
        Self {
            io,
            protocol: ProtocolSession::new(),
        }
    }

    pub fn into_io(self) -> Io {
        self.io
    }

    pub async fn close(&mut self) -> ClientResult<ClientOutcome<()>> {
        self.io.close().await.map_err(ClientError::from)?;
        Ok(ClientOutcome::new((), Vec::new()))
    }

    pub async fn send_request(
        &mut self,
        request: ClientRequest,
    ) -> ClientResult<ClientOutcome<WireServerMessage>> {
        let request_id = self.protocol.next_request_id();
        self.io
            .send(ClientMessage {
                id: request_id,
                msg: request,
            })
            .await
            .map_err(ClientError::from)?;

        let mut events = Vec::new();
        loop {
            let response = self.io.receive().await.map_err(ClientError::from)?;
            match self.protocol.response_disposition(&response, request_id) {
                ResponseDisposition::Matched => {
                    if let WireServerMsgBody::Error { error } = &response.msg {
                        return Err(ClientError::Server(error.clone()));
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

    pub async fn fs_read(&mut self, path: &LpPath) -> ClientResult<ClientOutcome<Vec<u8>>> {
        let response = self
            .send_request(ClientRequest::Filesystem(FsRequest::Read {
                path: path.to_path_buf(),
            }))
            .await?;
        let events = response.events;
        match response.value.msg {
            WireServerMsgBody::Filesystem(FsResponse::Read { data, error, .. }) => {
                if let Some(error) = error {
                    return Err(ClientError::Server(error));
                }
                data.map(|data| ClientOutcome::new(data, events))
                    .ok_or_else(|| ClientError::Protocol("no data in read response".to_string()))
            }
            other => Err(ClientError::unexpected_response("fs.read", other)),
        }
    }

    pub async fn fs_write(
        &mut self,
        path: &LpPath,
        data: Vec<u8>,
    ) -> ClientResult<ClientOutcome<()>> {
        let response = self
            .send_request(ClientRequest::Filesystem(FsRequest::Write {
                path: path.to_path_buf(),
                data,
            }))
            .await?;
        let events = response.events;
        match response.value.msg {
            WireServerMsgBody::Filesystem(FsResponse::Write { error, .. }) => {
                if let Some(error) = error {
                    return Err(ClientError::Server(error));
                }
                Ok(ClientOutcome::new((), events))
            }
            other => Err(ClientError::unexpected_response("fs.write", other)),
        }
    }

    pub async fn fs_delete_file(&mut self, path: &LpPath) -> ClientResult<ClientOutcome<()>> {
        let response = self
            .send_request(ClientRequest::Filesystem(FsRequest::DeleteFile {
                path: path.to_path_buf(),
            }))
            .await?;
        let events = response.events;
        match response.value.msg {
            WireServerMsgBody::Filesystem(FsResponse::DeleteFile { error, .. }) => {
                if let Some(error) = error {
                    return Err(ClientError::Server(error));
                }
                Ok(ClientOutcome::new((), events))
            }
            other => Err(ClientError::unexpected_response("fs.delete_file", other)),
        }
    }

    pub async fn fs_list_dir(
        &mut self,
        path: &LpPath,
        recursive: bool,
    ) -> ClientResult<ClientOutcome<Vec<LpPathBuf>>> {
        let response = self
            .send_request(ClientRequest::Filesystem(FsRequest::ListDir {
                path: path.to_path_buf(),
                recursive,
            }))
            .await?;
        let events = response.events;
        match response.value.msg {
            WireServerMsgBody::Filesystem(FsResponse::ListDir { entries, error, .. }) => {
                if let Some(error) = error {
                    return Err(ClientError::Server(error));
                }
                Ok(ClientOutcome::new(entries, events))
            }
            other => Err(ClientError::unexpected_response("fs.list_dir", other)),
        }
    }

    pub async fn project_load(
        &mut self,
        path: &str,
    ) -> ClientResult<ClientOutcome<WireProjectHandle>> {
        let response = self
            .send_request(ClientRequest::LoadProject {
                path: path.to_string(),
            })
            .await?;
        let events = response.events;
        match response.value.msg {
            WireServerMsgBody::LoadProject { handle } => Ok(ClientOutcome::new(handle, events)),
            other => Err(ClientError::unexpected_response("project.load", other)),
        }
    }

    pub async fn project_unload(
        &mut self,
        handle: WireProjectHandle,
    ) -> ClientResult<ClientOutcome<()>> {
        let response = self
            .send_request(ClientRequest::UnloadProject { handle })
            .await?;
        let events = response.events;
        match response.value.msg {
            WireServerMsgBody::UnloadProject => Ok(ClientOutcome::new((), events)),
            other => Err(ClientError::unexpected_response("project.unload", other)),
        }
    }

    pub async fn project_read(
        &mut self,
        handle: WireProjectHandle,
        read: ProjectReadRequest,
    ) -> ClientResult<ClientOutcome<ProjectReadResponse>> {
        let request_id = self.protocol.next_request_id();
        self.io
            .send(ClientMessage {
                id: request_id,
                msg: ClientRequest::ProjectRead {
                    handle,
                    request: read,
                },
            })
            .await
            .map_err(ClientError::from)?;

        let mut stream = ProjectReadStream::new(request_id);
        let mut events = Vec::new();
        loop {
            let response = self.io.receive().await.map_err(ClientError::from)?;
            match stream
                .accept(&self.protocol, response)
                .map_err(project_read_stream_error)?
            {
                ProjectReadStreamStep::Continue => {}
                ProjectReadStreamStep::Event(event) => events.push(event),
                ProjectReadStreamStep::Complete(response) => {
                    return Ok(ClientOutcome::new(response, events));
                }
            }
        }
    }

    pub async fn project_read_default_debug(
        &mut self,
        handle: WireProjectHandle,
    ) -> ClientResult<ClientOutcome<ProjectReadResponse>> {
        self.project_read(handle, ProjectReadRequest::default_debug(None))
            .await
    }

    pub async fn project_command(
        &mut self,
        handle: WireProjectHandle,
        command: WireProjectCommand,
    ) -> ClientResult<ClientOutcome<WireProjectCommandResponse>> {
        let response = self
            .send_request(ClientRequest::ProjectCommand { handle, command })
            .await?;
        let events = response.events;
        match response.value.msg {
            WireServerMsgBody::ProjectCommand { response } => {
                Ok(ClientOutcome::new(response, events))
            }
            other => Err(ClientError::unexpected_response("project.command", other)),
        }
    }

    pub async fn project_overlay_read(
        &mut self,
        handle: WireProjectHandle,
    ) -> ClientResult<ClientOutcome<WireOverlayReadResponse>> {
        let response = self
            .project_command(
                handle,
                WireProjectCommand::ReadOverlay {
                    request: WireOverlayReadRequest,
                },
            )
            .await?;
        match response.value {
            WireProjectCommandResponse::ReadOverlay { response: value } => {
                Ok(ClientOutcome::new(value, response.events))
            }
            other => Err(ClientError::unexpected_response(
                "project.overlay_read",
                other,
            )),
        }
    }

    pub async fn project_overlay_mutate(
        &mut self,
        handle: WireProjectHandle,
        request: WireOverlayMutationRequest,
    ) -> ClientResult<ClientOutcome<WireOverlayMutationResponse>> {
        let response = self
            .project_command(handle, WireProjectCommand::MutateOverlay { request })
            .await?;
        match response.value {
            WireProjectCommandResponse::MutateOverlay { response: value } => {
                Ok(ClientOutcome::new(value, response.events))
            }
            other => Err(ClientError::unexpected_response(
                "project.overlay_mutate",
                other,
            )),
        }
    }

    pub async fn project_overlay_commit(
        &mut self,
        handle: WireProjectHandle,
    ) -> ClientResult<ClientOutcome<WireOverlayCommitResponse>> {
        let response = self
            .project_command(
                handle,
                WireProjectCommand::CommitOverlay {
                    request: WireOverlayCommitRequest,
                },
            )
            .await?;
        match response.value {
            WireProjectCommandResponse::CommitOverlay { response: value } => {
                Ok(ClientOutcome::new(value, response.events))
            }
            other => Err(ClientError::unexpected_response(
                "project.overlay_commit",
                other,
            )),
        }
    }

    pub async fn project_inventory_read(
        &mut self,
        handle: WireProjectHandle,
    ) -> ClientResult<ClientOutcome<WireProjectInventoryReadResponse>> {
        let response = self
            .project_command(
                handle,
                WireProjectCommand::ReadInventory {
                    request: WireProjectInventoryReadRequest,
                },
            )
            .await?;
        match response.value {
            WireProjectCommandResponse::ReadInventory { response: value } => {
                Ok(ClientOutcome::new(value, response.events))
            }
            other => Err(ClientError::unexpected_response(
                "project.inventory_read",
                other,
            )),
        }
    }

    pub async fn project_list_available(
        &mut self,
    ) -> ClientResult<ClientOutcome<Vec<AvailableProject>>> {
        let response = self
            .send_request(ClientRequest::ListAvailableProjects)
            .await?;
        let events = response.events;
        match response.value.msg {
            WireServerMsgBody::ListAvailableProjects { projects } => {
                Ok(ClientOutcome::new(projects, events))
            }
            other => Err(ClientError::unexpected_response(
                "project.list_available",
                other,
            )),
        }
    }

    pub async fn project_list_loaded(&mut self) -> ClientResult<ClientOutcome<Vec<LoadedProject>>> {
        let response = self.send_request(ClientRequest::ListLoadedProjects).await?;
        let events = response.events;
        match response.value.msg {
            WireServerMsgBody::ListLoadedProjects { projects } => {
                Ok(ClientOutcome::new(projects, events))
            }
            other => Err(ClientError::unexpected_response(
                "project.list_loaded",
                other,
            )),
        }
    }

    pub async fn stop_all_projects(&mut self) -> ClientResult<ClientOutcome<()>> {
        let response = self.send_request(ClientRequest::StopAllProjects).await?;
        let events = response.events;
        match response.value.msg {
            WireServerMsgBody::StopAllProjects => Ok(ClientOutcome::new((), events)),
            other => Err(ClientError::unexpected_response("project.stop_all", other)),
        }
    }

    pub async fn push_project_files(
        &mut self,
        project_id: &str,
        files: impl IntoIterator<Item = ProjectDeployFile>,
    ) -> ClientResult<ClientOutcome<()>> {
        let mut events = Vec::new();
        for request in project_write_requests(project_id, files) {
            let outcome = self.send_request(request.clone()).await?;
            events.extend(outcome.events);
            validate_project_deploy_response(&request, &outcome.value.msg)?;
        }
        Ok(ClientOutcome::new((), events))
    }

    pub async fn deploy_project_files(
        &mut self,
        project_id: &str,
        files: impl IntoIterator<Item = ProjectDeployFile>,
    ) -> ClientResult<ClientOutcome<WireProjectHandle>> {
        let mut events = Vec::new();
        let mut handle = None;
        for request in project_deploy_requests(project_id, files) {
            let outcome = self.send_request(request.clone()).await?;
            events.extend(outcome.events);
            handle = validate_project_deploy_response(&request, &outcome.value.msg)?.or(handle);
        }
        handle
            .map(|handle| ClientOutcome::new(handle, events))
            .ok_or_else(|| ClientError::Protocol("project deploy did not load project".into()))
    }
}

fn project_read_stream_error(error: ProjectReadStreamError) -> ClientError {
    match error {
        ProjectReadStreamError::Server(message) => ClientError::Server(message),
        ProjectReadStreamError::Protocol(message) => ClientError::Protocol(message),
        ProjectReadStreamError::Unexpected(response) => ClientError::UnexpectedResponse {
            operation: "project.read",
            response,
        },
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

    use async_trait::async_trait;
    use lpc_model::Revision;
    use lpc_wire::{
        ProjectReadEvent, ProjectReadRequest, TransportError, WireProjectHandle, WireServerMessage,
    };

    use super::*;

    #[tokio::test]
    async fn project_read_collects_multiframe_response() {
        let io = ScriptedClientIo::new([
            project_read_frame(
                1,
                0,
                false,
                [ProjectReadEvent::Begin {
                    revision: Revision::new(7),
                }],
            ),
            project_read_frame(
                1,
                1,
                true,
                [ProjectReadEvent::End {
                    revision: Revision::new(7),
                }],
            ),
        ]);
        let mut client = LpClient::new(io);

        let outcome = client
            .project_read(WireProjectHandle::new(3), empty_project_read_request())
            .await
            .expect("project read");

        assert_eq!(outcome.value.revision, Revision::new(7));
        assert!(outcome.value.results.is_empty());
        assert!(outcome.value.probes.is_empty());

        let io = client.into_io();
        assert_eq!(io.sent.len(), 1);
        let ClientRequest::ProjectRead { handle, .. } = &io.sent[0].msg else {
            panic!("project read should use frame-backed request variant");
        };
        assert_eq!(handle.id(), 3);
    }

    #[tokio::test]
    async fn project_read_top_level_server_error_is_terminal() {
        let io = ScriptedClientIo::new([WireServerMessage::new(
            1,
            WireServerMsgBody::Error {
                error: "bad read".into(),
            },
        )]);
        let mut client = LpClient::new(io);

        let error = client
            .project_read(WireProjectHandle::new(3), empty_project_read_request())
            .await
            .unwrap_err();

        assert_eq!(error, ClientError::Server("bad read".into()));
    }

    #[tokio::test]
    async fn project_read_unexpected_same_id_message_is_protocol_error() {
        let io = ScriptedClientIo::new([WireServerMessage::new(
            1,
            WireServerMsgBody::StopAllProjects,
        )]);
        let mut client = LpClient::new(io);

        let error = client
            .project_read(WireProjectHandle::new(3), empty_project_read_request())
            .await
            .unwrap_err();

        assert!(matches!(
            error,
            ClientError::UnexpectedResponse {
                operation: "project.read",
                ..
            }
        ));
    }

    struct ScriptedClientIo {
        sent: Vec<ClientMessage>,
        responses: VecDeque<WireServerMessage>,
    }

    impl ScriptedClientIo {
        fn new(responses: impl IntoIterator<Item = WireServerMessage>) -> Self {
            Self {
                sent: Vec::new(),
                responses: responses.into_iter().collect(),
            }
        }
    }

    #[async_trait(?Send)]
    impl ClientIo for ScriptedClientIo {
        async fn send(&mut self, msg: ClientMessage) -> Result<(), TransportError> {
            self.sent.push(msg);
            Ok(())
        }

        async fn receive(&mut self) -> Result<WireServerMessage, TransportError> {
            self.responses
                .pop_front()
                .ok_or(TransportError::ConnectionLost)
        }

        async fn close(&mut self) -> Result<(), TransportError> {
            Ok(())
        }
    }

    fn project_read_frame(
        id: u64,
        sequence: u32,
        fin: bool,
        events: impl IntoIterator<Item = ProjectReadEvent>,
    ) -> WireServerMessage {
        WireServerMessage::stream_frame(
            id,
            sequence,
            fin,
            WireServerMsgBody::ProjectRead {
                events: events.into_iter().collect(),
            },
        )
    }

    fn empty_project_read_request() -> ProjectReadRequest {
        ProjectReadRequest {
            since: None,
            queries: Vec::new(),
            probes: Vec::new(),
        }
    }
}
