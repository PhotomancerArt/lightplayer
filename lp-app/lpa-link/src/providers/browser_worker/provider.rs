use std::collections::BTreeMap;

use crate::link_endpoint::{LinkEndpointId, LinkEndpointStatus};
use crate::link_provider::LinkProviderId;
use crate::link_session::LinkSessionId;
use crate::providers::browser_worker::BrowserWorkerOptions;
#[cfg(target_arch = "wasm32")]
use crate::providers::browser_worker::{
    BrowserInputEnvelope, BrowserOutputEnvelope, BrowserWorkerHandle,
};
use crate::{
    LinkCapabilities, LinkConnection, LinkConnectionKind, LinkDiagnostic, LinkDiagnosticSeverity,
    LinkEndpoint, LinkError, LinkLogEntry, LinkLogLevel, LinkOperation, LinkProvider, LinkSession,
    LinkSessionStatus,
};

pub struct BrowserWorkerProvider {
    id: LinkProviderId,
    endpoints: Vec<LinkEndpoint>,
    sessions: BTreeMap<LinkSessionId, BrowserWorkerSessionState>,
    options: BrowserWorkerOptions,
    next_endpoint_index: u64,
    next_session_index: u64,
}

impl BrowserWorkerProvider {
    pub fn new(id: impl Into<LinkProviderId>) -> Self {
        Self::with_options(id, BrowserWorkerOptions::default())
    }

    pub fn with_options(id: impl Into<LinkProviderId>, options: BrowserWorkerOptions) -> Self {
        Self {
            id: id.into(),
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
            self.id.as_str(),
            self.next_endpoint_index
        ));
        self.next_endpoint_index += 1;

        let endpoint = LinkEndpoint::new(endpoint_id.clone(), self.id.clone(), label)
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

    #[cfg(target_arch = "wasm32")]
    pub fn post(
        &self,
        session_id: &LinkSessionId,
        envelope: &BrowserInputEnvelope,
    ) -> Result<(), LinkError> {
        self.session(session_id)?.handle()?.post(envelope)
    }

    #[cfg(target_arch = "wasm32")]
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
    fn id(&self) -> &LinkProviderId {
        &self.id
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
            self.id.clone(),
            endpoint.id.clone(),
            LinkConnectionKind::BrowserWorker {
                protocol: "fw-browser-post-message-v1".to_string(),
            },
            endpoint.capabilities.clone(),
        );
        #[cfg(target_arch = "wasm32")]
        let state = {
            let mut state = BrowserWorkerSessionState::new(endpoint.id.clone(), session.clone());
            let mut handle = BrowserWorkerHandle::new(&self.options.worker_script_path())?;
            state
                .pending_outputs
                .extend(handle.boot("Studio browser runtime", &self.options).await?);
            state.handle = Some(handle);
            state
        };
        #[cfg(not(target_arch = "wasm32"))]
        let state = BrowserWorkerSessionState::new(endpoint.id, session.clone());
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
        #[cfg(target_arch = "wasm32")]
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
    #[cfg(target_arch = "wasm32")]
    pending_outputs: Vec<BrowserOutputEnvelope>,
    #[cfg(target_arch = "wasm32")]
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
            #[cfg(target_arch = "wasm32")]
            pending_outputs: Vec::new(),
            #[cfg(target_arch = "wasm32")]
            handle: None,
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn handle(&self) -> Result<&BrowserWorkerHandle, LinkError> {
        self.handle
            .as_ref()
            .ok_or_else(|| LinkError::other("browser worker session has no worker handle"))
    }

    #[cfg(target_arch = "wasm32")]
    fn handle_mut(&mut self) -> Result<&mut BrowserWorkerHandle, LinkError> {
        self.handle
            .as_mut()
            .ok_or_else(|| LinkError::other("browser worker session has no worker handle"))
    }
}
