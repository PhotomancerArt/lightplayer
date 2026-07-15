use std::cell::{Cell, RefCell};
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

/// Browser worker provider backed by `fw-browser`.
///
/// Endpoint and session state live behind internal `RefCell`s with borrows
/// scoped to synchronous sections — in particular, `connect()` boots the
/// worker on a LOCAL handle and only inserts it into the session map after
/// the boot await resolves, so no borrow spans a JS future.
pub struct BrowserWorkerProvider {
    endpoints: RefCell<Vec<LinkEndpoint>>,
    sessions: RefCell<BTreeMap<LinkSessionId, BrowserWorkerSessionState>>,
    options: BrowserWorkerOptions,
    next_endpoint_index: Cell<u64>,
    next_session_index: Cell<u64>,
}

impl BrowserWorkerProvider {
    pub fn new() -> Self {
        Self::with_options(BrowserWorkerOptions::default())
    }

    pub fn with_options(options: BrowserWorkerOptions) -> Self {
        Self {
            endpoints: RefCell::new(Vec::new()),
            sessions: RefCell::new(BTreeMap::new()),
            options,
            next_endpoint_index: Cell::new(1),
            next_session_index: Cell::new(1),
        }
    }

    pub fn options(&self) -> &BrowserWorkerOptions {
        &self.options
    }

    pub fn create_worker_endpoint(&self, label: impl Into<String>) -> LinkEndpointId {
        let endpoint_index = self.next_endpoint_index.get();
        self.next_endpoint_index.set(endpoint_index + 1);
        let endpoint_id =
            LinkEndpointId::new(format!("{}-worker-{}", self.kind().key(), endpoint_index));

        let endpoint = LinkEndpoint::new(endpoint_id.clone(), self.kind(), label)
            .with_capabilities(
                LinkCapabilities::default()
                    .with(LinkOperation::ReadLogs)
                    .with(LinkOperation::ReadDiagnostics),
            );
        self.endpoints.borrow_mut().push(endpoint);
        endpoint_id
    }

    fn endpoint(&self, endpoint_id: &LinkEndpointId) -> Result<LinkEndpoint, LinkError> {
        self.endpoints
            .borrow()
            .iter()
            .find(|endpoint| endpoint.id == *endpoint_id)
            .cloned()
            .ok_or_else(|| LinkError::endpoint_not_found(endpoint_id.as_str()))
    }

    pub fn post(
        &self,
        session_id: &LinkSessionId,
        envelope: &BrowserInputEnvelope,
    ) -> Result<(), LinkError> {
        let sessions = self.sessions.borrow();
        let state = sessions
            .get(session_id)
            .ok_or_else(|| LinkError::session_not_found(session_id.as_str()))?;
        state.handle()?.post(envelope)
    }

    pub fn take_outputs(
        &self,
        session_id: &LinkSessionId,
    ) -> Result<Vec<BrowserOutputEnvelope>, LinkError> {
        let mut sessions = self.sessions.borrow_mut();
        let state = sessions
            .get_mut(session_id)
            .ok_or_else(|| LinkError::session_not_found(session_id.as_str()))?;
        let mut outputs = state.pending_outputs.split_off(0);
        outputs.extend(state.handle_mut()?.take_outputs());
        Ok(outputs)
    }
}

impl LinkProvider for BrowserWorkerProvider {
    fn kind(&self) -> LinkProviderKind {
        LinkProviderKind::BrowserWorker
    }

    async fn discover(&self) -> Result<Vec<LinkEndpoint>, LinkError> {
        Ok(self.endpoints.borrow().clone())
    }

    async fn status(&self, endpoint_id: &LinkEndpointId) -> Result<LinkEndpointStatus, LinkError> {
        Ok(self.endpoint(endpoint_id)?.status)
    }

    async fn connect(&self, endpoint_id: &LinkEndpointId) -> Result<LinkSession, LinkError> {
        let endpoint = self.endpoint(endpoint_id)?;
        let session_index = self.next_session_index.get();
        self.next_session_index.set(session_index + 1);
        let session_id = LinkSessionId::new(format!("{}:{}", endpoint_id.as_str(), session_index));

        let session = LinkSession::new(
            session_id.clone(),
            self.kind(),
            endpoint.id.clone(),
            LinkConnectionKind::BrowserWorker {
                protocol: "fw-browser-post-message-v1".to_string(),
            },
            endpoint.capabilities.clone(),
        );
        // Boot the worker on locals so the boot await runs with no session
        // borrow held; only the finished state enters the map.
        let mut state = BrowserWorkerSessionState::new(endpoint.id, session.clone());
        let mut handle = BrowserWorkerHandle::new(&self.options.worker_script_path())?;
        state
            .pending_outputs
            .extend(handle.boot("Studio browser runtime", &self.options).await?);
        state.handle = Some(handle);
        self.sessions.borrow_mut().insert(session_id, state);
        Ok(session)
    }

    async fn connection(&self, session_id: &LinkSessionId) -> Result<LinkConnection, LinkError> {
        let sessions = self.sessions.borrow();
        let state = sessions
            .get(session_id)
            .ok_or_else(|| LinkError::session_not_found(session_id.as_str()))?;
        if state.session.status == LinkSessionStatus::Closed {
            return Err(LinkError::Closed);
        }
        Ok(LinkConnection::browser_worker(
            state.session.endpoint_id.clone(),
            state.session.id.clone(),
        ))
    }

    fn logs(&self, session_id: &LinkSessionId) -> Result<Vec<LinkLogEntry>, LinkError> {
        let sessions = self.sessions.borrow();
        let state = sessions
            .get(session_id)
            .ok_or_else(|| LinkError::session_not_found(session_id.as_str()))?;
        Ok(state.logs.clone())
    }

    fn diagnostics(&self, session_id: &LinkSessionId) -> Result<Vec<LinkDiagnostic>, LinkError> {
        let sessions = self.sessions.borrow();
        let state = sessions
            .get(session_id)
            .ok_or_else(|| LinkError::session_not_found(session_id.as_str()))?;
        Ok(state.diagnostics.clone())
    }

    async fn close(&self, session_id: &LinkSessionId) -> Result<(), LinkError> {
        // Worker termination is synchronous: the whole close runs under one
        // scoped borrow with no await.
        let mut sessions = self.sessions.borrow_mut();
        let state = sessions
            .get_mut(session_id)
            .ok_or_else(|| LinkError::session_not_found(session_id.as_str()))?;
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
