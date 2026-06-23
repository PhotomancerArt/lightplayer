use std::collections::BTreeMap;

use crate::provider::endpoint::{LinkEndpointId, LinkEndpointStatus};
use crate::provider::session::LinkSessionId;
use crate::providers::{LinkProviderDescriptor, LinkProviderKind};
use crate::{
    LinkCapabilities, LinkConnection, LinkConnectionKind, LinkDiagnostic, LinkDiagnosticSeverity,
    LinkEndpoint, LinkError, LinkLogEntry, LinkLogLevel, LinkOperation, LinkProvider, LinkSession,
    LinkSessionStatus,
};
use fw_host::HostRuntime;

pub fn descriptor() -> LinkProviderDescriptor {
    LinkProviderKind::HostProcess.descriptor()
}

pub struct HostProcessProvider {
    endpoints: Vec<LinkEndpoint>,
    sessions: BTreeMap<LinkSessionId, HostProcessSessionState>,
    next_endpoint_index: u64,
    next_session_index: u64,
}

impl HostProcessProvider {
    pub fn new() -> Self {
        Self {
            endpoints: Vec::new(),
            sessions: BTreeMap::new(),
            next_endpoint_index: 1,
            next_session_index: 1,
        }
    }

    /// Create a spawnable in-process `fw-host` memory runtime endpoint.
    ///
    /// The endpoint is not a physical device. Each successful `connect()` call
    /// starts a new `fw-host` runtime instance and returns a session that owns
    /// that runtime lifecycle.
    pub fn create_memory_endpoint(&mut self, label: impl Into<String>) -> LinkEndpointId {
        let endpoint_id = LinkEndpointId::new(format!(
            "{}-memory-{}",
            self.kind().key(),
            self.next_endpoint_index
        ));
        self.next_endpoint_index += 1;

        let endpoint = LinkEndpoint::new(endpoint_id.clone(), self.kind(), label)
            .with_capabilities(
                LinkCapabilities::default()
                    .with(LinkOperation::ReadLogs)
                    .with(LinkOperation::ReadDiagnostics),
            );
        self.endpoints.push(endpoint);
        endpoint_id
    }

    fn endpoint(&self, endpoint_id: &LinkEndpointId) -> Result<&LinkEndpoint, LinkError> {
        self.endpoints
            .iter()
            .find(|endpoint| endpoint.id == *endpoint_id)
            .ok_or_else(|| LinkError::endpoint_not_found(endpoint_id.as_str()))
    }

    fn session(&self, session_id: &LinkSessionId) -> Result<&HostProcessSessionState, LinkError> {
        self.sessions
            .get(session_id)
            .ok_or_else(|| LinkError::session_not_found(session_id.as_str()))
    }

    fn session_mut(
        &mut self,
        session_id: &LinkSessionId,
    ) -> Result<&mut HostProcessSessionState, LinkError> {
        self.sessions
            .get_mut(session_id)
            .ok_or_else(|| LinkError::session_not_found(session_id.as_str()))
    }
}

impl LinkProvider for HostProcessProvider {
    fn kind(&self) -> LinkProviderKind {
        LinkProviderKind::HostProcess
    }

    async fn discover(&mut self) -> Result<Vec<LinkEndpoint>, LinkError> {
        Ok(self.endpoints.clone())
    }

    async fn status(
        &mut self,
        endpoint_id: &LinkEndpointId,
    ) -> Result<LinkEndpointStatus, LinkError> {
        Ok(self.endpoint(endpoint_id)?.status.clone())
    }

    async fn connect(&mut self, endpoint_id: &LinkEndpointId) -> Result<LinkSession, LinkError> {
        let endpoint = self.endpoint(endpoint_id)?.clone();
        let runtime = HostRuntime::start_memory().map_err(|error| LinkError::ConnectionFailed {
            message: error.to_string(),
        })?;
        let session_id = LinkSessionId::new(format!(
            "{}:{}",
            endpoint_id.as_str(),
            self.next_session_index
        ));
        self.next_session_index += 1;

        let session = LinkSession::new(
            session_id.clone(),
            self.kind(),
            endpoint.id.clone(),
            LinkConnectionKind::HostProcess,
            endpoint.capabilities.clone(),
        );
        self.sessions.insert(
            session_id,
            HostProcessSessionState::new(endpoint.id, session.clone(), runtime),
        );
        Ok(session)
    }

    async fn connection(
        &mut self,
        session_id: &LinkSessionId,
    ) -> Result<LinkConnection, LinkError> {
        let state = self.session(session_id)?;
        if state.session.status == LinkSessionStatus::Closed {
            return Err(LinkError::Closed);
        }
        Ok(LinkConnection::host_process(
            state.session.endpoint_id.clone(),
            state.session.id.clone(),
            state.runtime.client_transport(),
        ))
    }

    fn logs(&self, session_id: &LinkSessionId) -> Result<Vec<LinkLogEntry>, LinkError> {
        Ok(self.session(session_id)?.logs.clone())
    }

    fn diagnostics(&self, session_id: &LinkSessionId) -> Result<Vec<LinkDiagnostic>, LinkError> {
        Ok(self.session(session_id)?.diagnostics.clone())
    }

    async fn close(&mut self, session_id: &LinkSessionId) -> Result<(), LinkError> {
        let state = self.session_mut(session_id)?;
        if state.session.status == LinkSessionStatus::Closed {
            return Ok(());
        }
        state.session.status = LinkSessionStatus::Closed;
        state
            .runtime
            .close()
            .await
            .map_err(|error| LinkError::other(error.to_string()))?;
        state.logs.push(LinkLogEntry::new(
            state.endpoint_id.clone(),
            Some(state.session.id.clone()),
            LinkLogLevel::Info,
            "host process runtime stopped",
        ));
        Ok(())
    }
}

struct HostProcessSessionState {
    endpoint_id: LinkEndpointId,
    session: LinkSession,
    runtime: HostRuntime,
    logs: Vec<LinkLogEntry>,
    diagnostics: Vec<LinkDiagnostic>,
}

impl HostProcessSessionState {
    fn new(endpoint_id: LinkEndpointId, session: LinkSession, runtime: HostRuntime) -> Self {
        let logs = vec![LinkLogEntry::new(
            endpoint_id.clone(),
            Some(session.id.clone()),
            LinkLogLevel::Info,
            "host process runtime started",
        )];
        let diagnostics = vec![LinkDiagnostic::new(
            endpoint_id.clone(),
            Some(session.id.clone()),
            LinkDiagnosticSeverity::Info,
            "host process runtime ready",
        )];

        Self {
            endpoint_id,
            session,
            runtime,
            logs,
            diagnostics,
        }
    }
}
