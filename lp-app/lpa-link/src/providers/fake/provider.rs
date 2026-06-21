use std::collections::BTreeMap;

use crate::provider::endpoint::{LinkEndpointId, LinkEndpointStatus};
use crate::provider::session::LinkSessionId;
use crate::providers::{LinkProviderDescriptor, LinkProviderKind};
use crate::{
    LinkConnection, LinkConnectionKind, LinkDiagnostic, LinkDiagnosticSeverity, LinkEndpoint,
    LinkError, LinkLogEntry, LinkLogLevel, LinkProvider, LinkSession, LinkSessionStatus,
};

pub fn descriptor() -> LinkProviderDescriptor {
    LinkProviderKind::Fake.descriptor()
}

#[derive(Clone, Debug)]
pub struct FakeProvider {
    endpoints: Vec<LinkEndpoint>,
    sessions: BTreeMap<LinkSessionId, FakeSessionState>,
    next_session_index: u64,
}

impl FakeProvider {
    pub fn new() -> Self {
        Self {
            endpoints: Vec::new(),
            sessions: BTreeMap::new(),
            next_session_index: 1,
        }
    }

    pub fn with_endpoint(mut self, endpoint: LinkEndpoint) -> Self {
        self.endpoints.push(endpoint);
        self
    }

    fn endpoint(&self, endpoint_id: &LinkEndpointId) -> Result<&LinkEndpoint, LinkError> {
        self.endpoints
            .iter()
            .find(|endpoint| endpoint.id == *endpoint_id)
            .ok_or_else(|| LinkError::endpoint_not_found(endpoint_id.as_str()))
    }

    fn session(&self, session_id: &LinkSessionId) -> Result<&FakeSessionState, LinkError> {
        self.sessions
            .get(session_id)
            .ok_or_else(|| LinkError::session_not_found(session_id.as_str()))
    }

    fn session_mut(
        &mut self,
        session_id: &LinkSessionId,
    ) -> Result<&mut FakeSessionState, LinkError> {
        self.sessions
            .get_mut(session_id)
            .ok_or_else(|| LinkError::session_not_found(session_id.as_str()))
    }
}

impl LinkProvider for FakeProvider {
    fn kind(&self) -> LinkProviderKind {
        LinkProviderKind::Fake
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
            LinkConnectionKind::Fake,
            endpoint.capabilities.clone(),
        );
        self.sessions.insert(
            session_id,
            FakeSessionState::new(endpoint.id, session.clone()),
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
        Ok(LinkConnection::fake(
            state.session.endpoint_id.clone(),
            state.session.id.clone(),
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
        state.session.status = LinkSessionStatus::Closed;
        state.logs.push(LinkLogEntry::new(
            state.endpoint_id.clone(),
            Some(state.session.id.clone()),
            LinkLogLevel::Info,
            "fake link session closed",
        ));
        Ok(())
    }
}

#[derive(Clone, Debug)]
struct FakeSessionState {
    endpoint_id: LinkEndpointId,
    session: LinkSession,
    logs: Vec<LinkLogEntry>,
    diagnostics: Vec<LinkDiagnostic>,
}

impl FakeSessionState {
    fn new(endpoint_id: LinkEndpointId, session: LinkSession) -> Self {
        let logs = vec![LinkLogEntry::new(
            endpoint_id.clone(),
            Some(session.id.clone()),
            LinkLogLevel::Info,
            "fake link session opened",
        )];
        let diagnostics = vec![LinkDiagnostic::new(
            endpoint_id.clone(),
            Some(session.id.clone()),
            LinkDiagnosticSeverity::Info,
            "fake link session ready",
        )];
        Self {
            endpoint_id,
            session,
            logs,
            diagnostics,
        }
    }
}
