use std::collections::BTreeMap;

use crate::provider::endpoint::{LinkEndpointId, LinkEndpointStatus};
use crate::provider::session::LinkSessionId;
use crate::providers::browser_worker::BrowserWorkerOptions;
use crate::providers::browser_worker::{
    BrowserInputEnvelope, BrowserOutputEnvelope, BrowserWorkerHandle,
};
use crate::providers::{LinkProviderDescriptor, LinkProviderKind};
use crate::{
    LinkCapabilities, LinkConnection, LinkConnectionKind, LinkDiagnostic, LinkDiagnosticSeverity,
    LinkEndpoint, LinkError, LinkLogEntry, LinkLogLevel, LinkOperation, LinkProvider, LinkSession,
    LinkSessionStatus,
};

pub fn descriptor() -> LinkProviderDescriptor {
    LinkProviderKind::BrowserWorker.descriptor()
}

pub struct BrowserWorkerProvider {
    endpoints: Vec<LinkEndpoint>,
    sessions: BTreeMap<LinkSessionId, BrowserWorkerSessionState>,
    options: BrowserWorkerOptions,
    next_endpoint_index: u64,
    next_session_index: u64,
}

impl BrowserWorkerProvider {
    pub fn new() -> Self {
        Self::with_options(BrowserWorkerOptions::default())
    }

    pub fn with_options(options: BrowserWorkerOptions) -> Self {
        Self {
            endpoints: Vec::new(),
            sessions: BTreeMap::new(),
            options,
            next_endpoint_index: 1,
            next_session_index: 1,
        }
    }

    pub fn options(&self) -> &BrowserWorkerOptions {
        &self.options
    }

    pub fn create_worker_endpoint(&mut self, label: impl Into<String>) -> LinkEndpointId {
        let endpoint_id = LinkEndpointId::new(format!(
            "{}-worker-{}",
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

    pub fn post(
        &self,
        session_id: &LinkSessionId,
        envelope: &BrowserInputEnvelope,
    ) -> Result<(), LinkError> {
        self.session(session_id)?.handle()?.post(envelope)
    }

    pub fn take_outputs(
        &mut self,
        session_id: &LinkSessionId,
    ) -> Result<Vec<BrowserOutputEnvelope>, LinkError> {
        let state = self.session_mut(session_id)?;
        let mut outputs = state.pending_outputs.split_off(0);
        outputs.extend(state.handle_mut()?.take_outputs());
        Ok(outputs)
    }

    fn session(&self, session_id: &LinkSessionId) -> Result<&BrowserWorkerSessionState, LinkError> {
        self.sessions
            .get(session_id)
            .ok_or_else(|| LinkError::session_not_found(session_id.as_str()))
    }

    fn session_mut(
        &mut self,
        session_id: &LinkSessionId,
    ) -> Result<&mut BrowserWorkerSessionState, LinkError> {
        self.sessions
            .get_mut(session_id)
            .ok_or_else(|| LinkError::session_not_found(session_id.as_str()))
    }
}

impl LinkProvider for BrowserWorkerProvider {
    fn kind(&self) -> LinkProviderKind {
        LinkProviderKind::BrowserWorker
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
            LinkConnectionKind::BrowserWorker {
                protocol: "fw-browser-post-message-v1".to_string(),
            },
            endpoint.capabilities.clone(),
        );
        let mut state = BrowserWorkerSessionState::new(endpoint.id, session.clone());
        let mut handle = BrowserWorkerHandle::new(&self.options.worker_script_path())?;
        state
            .pending_outputs
            .extend(handle.boot("Studio browser runtime", &self.options).await?);
        state.handle = Some(handle);
        self.sessions.insert(session_id, state);
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
        Ok(LinkConnection::browser_worker(
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
        if state.session.status == LinkSessionStatus::Closed {
            return Ok(());
        }
        state.session.status = LinkSessionStatus::Closed;
        if let Some(handle) = &state.handle {
            handle.terminate();
        }
        state.logs.push(LinkLogEntry::new(
            state.endpoint_id.clone(),
            Some(state.session.id.clone()),
            LinkLogLevel::Info,
            "browser worker session closed",
        ));
        Ok(())
    }
}

struct BrowserWorkerSessionState {
    endpoint_id: LinkEndpointId,
    session: LinkSession,
    logs: Vec<LinkLogEntry>,
    diagnostics: Vec<LinkDiagnostic>,
    pending_outputs: Vec<BrowserOutputEnvelope>,
    handle: Option<BrowserWorkerHandle>,
}

impl BrowserWorkerSessionState {
    fn new(endpoint_id: LinkEndpointId, session: LinkSession) -> Self {
        let logs = vec![LinkLogEntry::new(
            endpoint_id.clone(),
            Some(session.id.clone()),
            LinkLogLevel::Info,
            "browser worker session created",
        )];
        let diagnostics = vec![LinkDiagnostic::new(
            endpoint_id.clone(),
            Some(session.id.clone()),
            LinkDiagnosticSeverity::Info,
            "browser worker session owns Worker lifecycle in lpa-link",
        )];
        Self {
            endpoint_id,
            session,
            logs,
            diagnostics,
            pending_outputs: Vec::new(),
            handle: None,
        }
    }

    fn handle(&self) -> Result<&BrowserWorkerHandle, LinkError> {
        self.handle
            .as_ref()
            .ok_or_else(|| LinkError::other("browser worker session has no worker handle"))
    }

    fn handle_mut(&mut self) -> Result<&mut BrowserWorkerHandle, LinkError> {
        self.handle
            .as_mut()
            .ok_or_else(|| LinkError::other("browser worker session has no worker handle"))
    }
}
