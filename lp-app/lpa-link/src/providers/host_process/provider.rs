use std::collections::BTreeMap;
use std::sync::{Mutex, MutexGuard};

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

/// Host process provider: spawnable in-process `fw-host` memory runtimes.
///
/// Endpoint and session state live behind an internal `Mutex` with lock
/// scopes confined to synchronous sections (never across an `await`), so the
/// provider serves `&self` callers through a shared `LinkConnector` while its
/// futures stay `Send` for host consumers such as `lp-cli`.
pub struct HostProcessProvider {
    state: Mutex<HostProcessState>,
}

struct HostProcessState {
    endpoints: Vec<LinkEndpoint>,
    sessions: BTreeMap<LinkSessionId, HostProcessSessionState>,
    next_endpoint_index: u64,
    next_session_index: u64,
}

impl HostProcessProvider {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(HostProcessState {
                endpoints: Vec::new(),
                sessions: BTreeMap::new(),
                next_endpoint_index: 1,
                next_session_index: 1,
            }),
        }
    }

    /// Create a spawnable in-process `fw-host` memory runtime endpoint.
    ///
    /// The endpoint is not a physical device. Each successful `connect()` call
    /// starts a new `fw-host` runtime instance and returns a session that owns
    /// that runtime lifecycle.
    pub fn create_memory_endpoint(&self, label: impl Into<String>) -> LinkEndpointId {
        let mut state = self.state();
        let endpoint_index = state.next_endpoint_index;
        state.next_endpoint_index += 1;
        let endpoint_id =
            LinkEndpointId::new(format!("{}-memory-{}", self.kind().key(), endpoint_index));

        let endpoint = LinkEndpoint::new(endpoint_id.clone(), self.kind(), label)
            .with_capabilities(
                LinkCapabilities::default()
                    .with(LinkOperation::ReadLogs)
                    .with(LinkOperation::ReadDiagnostics),
            );
        state.endpoints.push(endpoint);
        endpoint_id
    }

    fn state(&self) -> MutexGuard<'_, HostProcessState> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn endpoint(&self, endpoint_id: &LinkEndpointId) -> Result<LinkEndpoint, LinkError> {
        self.state()
            .endpoints
            .iter()
            .find(|endpoint| endpoint.id == *endpoint_id)
            .cloned()
            .ok_or_else(|| LinkError::endpoint_not_found(endpoint_id.as_str()))
    }
}

impl LinkProvider for HostProcessProvider {
    fn kind(&self) -> LinkProviderKind {
        LinkProviderKind::HostProcess
    }

    async fn discover(&self) -> Result<Vec<LinkEndpoint>, LinkError> {
        Ok(self.state().endpoints.clone())
    }

    async fn status(&self, endpoint_id: &LinkEndpointId) -> Result<LinkEndpointStatus, LinkError> {
        Ok(self.endpoint(endpoint_id)?.status)
    }

    async fn connect(&self, endpoint_id: &LinkEndpointId) -> Result<LinkSession, LinkError> {
        let endpoint = self.endpoint(endpoint_id)?;
        let runtime = HostRuntime::start_memory().map_err(|error| LinkError::ConnectionFailed {
            message: error.to_string(),
        })?;
        let mut state = self.state();
        let session_index = state.next_session_index;
        state.next_session_index += 1;
        let session_id = LinkSessionId::new(format!("{}:{}", endpoint_id.as_str(), session_index));

        let session = LinkSession::new(
            session_id.clone(),
            self.kind(),
            endpoint.id.clone(),
            LinkConnectionKind::HostProcess,
            endpoint.capabilities.clone(),
        );
        state.sessions.insert(
            session_id,
            HostProcessSessionState::new(endpoint.id, session.clone(), runtime),
        );
        Ok(session)
    }

    async fn connection(&self, session_id: &LinkSessionId) -> Result<LinkConnection, LinkError> {
        let state = self.state();
        let session = state
            .sessions
            .get(session_id)
            .ok_or_else(|| LinkError::session_not_found(session_id.as_str()))?;
        if session.session.status == LinkSessionStatus::Closed {
            return Err(LinkError::Closed);
        }
        let runtime = session.runtime.as_ref().ok_or(LinkError::Closed)?;
        Ok(LinkConnection::host_process(
            session.session.endpoint_id.clone(),
            session.session.id.clone(),
            runtime.client_transport(),
        ))
    }

    fn logs(&self, session_id: &LinkSessionId) -> Result<Vec<LinkLogEntry>, LinkError> {
        let state = self.state();
        let session = state
            .sessions
            .get(session_id)
            .ok_or_else(|| LinkError::session_not_found(session_id.as_str()))?;
        Ok(session.logs.clone())
    }

    fn diagnostics(&self, session_id: &LinkSessionId) -> Result<Vec<LinkDiagnostic>, LinkError> {
        let state = self.state();
        let session = state
            .sessions
            .get(session_id)
            .ok_or_else(|| LinkError::session_not_found(session_id.as_str()))?;
        Ok(session.diagnostics.clone())
    }

    async fn close(&self, session_id: &LinkSessionId) -> Result<(), LinkError> {
        // Take the runtime out of the session state before awaiting its
        // shutdown: no internal lock may span the await.
        let runtime = {
            let mut state = self.state();
            let session = state
                .sessions
                .get_mut(session_id)
                .ok_or_else(|| LinkError::session_not_found(session_id.as_str()))?;
            if session.session.status == LinkSessionStatus::Closed {
                return Ok(());
            }
            session.session.status = LinkSessionStatus::Closed;
            session.runtime.take()
        };
        if let Some(mut runtime) = runtime {
            runtime
                .close()
                .await
                .map_err(|error| LinkError::other(error.to_string()))?;
        }
        let mut state = self.state();
        let session = state
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| LinkError::session_not_found(session_id.as_str()))?;
        let log = LinkLogEntry::new(
            session.endpoint_id.clone(),
            Some(session.session.id.clone()),
            LinkLogLevel::Info,
            "host process runtime stopped",
        );
        session.logs.push(log);
        Ok(())
    }
}

struct HostProcessSessionState {
    endpoint_id: LinkEndpointId,
    session: LinkSession,
    /// `None` after `close()` took the runtime for shutdown.
    runtime: Option<HostRuntime>,
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
            runtime: Some(runtime),
            logs,
            diagnostics,
        }
    }
}
