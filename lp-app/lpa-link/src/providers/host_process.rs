use fw_host::HostRuntime;

use crate::{
    LinkConnection, LinkDiagnostic, LinkDiagnosticSeverity, LinkEndpoint, LinkEndpointId,
    LinkEndpointStatus, LinkError, LinkLogEntry, LinkLogLevel, LinkManagement, LinkProvider,
    LinkProviderId, LinkSession, LinkSessionId,
};

#[derive(Clone, Debug)]
pub struct HostProcessProvider {
    id: LinkProviderId,
    endpoints: Vec<LinkEndpoint>,
    next_endpoint_index: u64,
    next_session_index: u64,
}

impl HostProcessProvider {
    pub fn new(id: impl Into<LinkProviderId>) -> Self {
        Self {
            id: id.into(),
            endpoints: Vec::new(),
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
            self.id.as_str(),
            self.next_endpoint_index
        ));
        self.next_endpoint_index += 1;

        let endpoint = LinkEndpoint::new(endpoint_id.clone(), self.id.clone(), label)
            .with_management(LinkManagement {
                can_read_logs: true,
                can_read_diagnostics: true,
                ..LinkManagement::default()
            });
        self.endpoints.push(endpoint);
        endpoint_id
    }

    fn endpoint(&self, endpoint_id: &LinkEndpointId) -> Result<&LinkEndpoint, LinkError> {
        self.endpoints
            .iter()
            .find(|endpoint| endpoint.id == *endpoint_id)
            .ok_or_else(|| LinkError::endpoint_not_found(endpoint_id.as_str()))
    }
}

impl LinkProvider for HostProcessProvider {
    type Session = HostProcessSession;

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

    async fn connect(&mut self, endpoint_id: &LinkEndpointId) -> Result<Self::Session, LinkError> {
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

        Ok(HostProcessSession::new(endpoint.id, session_id, runtime))
    }
}

pub struct HostProcessSession {
    endpoint_id: LinkEndpointId,
    id: LinkSessionId,
    runtime: HostRuntime,
    logs: Vec<LinkLogEntry>,
    diagnostics: Vec<LinkDiagnostic>,
}

impl HostProcessSession {
    pub fn new(endpoint_id: LinkEndpointId, id: LinkSessionId, runtime: HostRuntime) -> Self {
        let logs = vec![LinkLogEntry::new(
            endpoint_id.clone(),
            Some(id.clone()),
            LinkLogLevel::Info,
            "host process runtime started",
        )];
        let diagnostics = vec![LinkDiagnostic::new(
            endpoint_id.clone(),
            Some(id.clone()),
            LinkDiagnosticSeverity::Info,
            "host process runtime ready",
        )];

        Self {
            endpoint_id,
            id,
            runtime,
            logs,
            diagnostics,
        }
    }
}

impl LinkSession for HostProcessSession {
    fn id(&self) -> &LinkSessionId {
        &self.id
    }

    fn endpoint_id(&self) -> &LinkEndpointId {
        &self.endpoint_id
    }

    fn logs(&self) -> Vec<LinkLogEntry> {
        self.logs.clone()
    }

    fn diagnostics(&self) -> Vec<LinkDiagnostic> {
        self.diagnostics.clone()
    }

    async fn connection(&mut self) -> Result<LinkConnection, LinkError> {
        Ok(LinkConnection::host_process(
            self.endpoint_id.clone(),
            self.id.clone(),
            self.runtime.client_transport(),
        ))
    }

    async fn close(&mut self) -> Result<(), LinkError> {
        self.runtime
            .close()
            .await
            .map_err(|error| LinkError::Other {
                message: error.to_string(),
            })?;
        self.logs.push(LinkLogEntry::new(
            self.endpoint_id.clone(),
            Some(self.id.clone()),
            LinkLogLevel::Info,
            "host process runtime stopped",
        ));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use lpa_client::LpClient;

    use super::*;

    #[tokio::test]
    async fn host_process_connection_serves_client_requests() {
        let mut provider = provider_with_two_endpoints();
        let endpoint_id = LinkEndpointId::new("host-process-memory-1");
        let mut session = provider.connect(&endpoint_id).await.unwrap();

        let connection = session.connection().await.unwrap();
        assert!(matches!(
            connection.kind,
            crate::LinkConnectionKind::HostProcess
        ));
        let transport = connection.client_transport().unwrap();
        let client = LpClient::new_shared(transport);
        let projects = client.project_list_available().await.unwrap();

        assert!(projects.is_empty());
        session.close().await.unwrap();
    }

    #[tokio::test]
    async fn host_process_provider_supports_multiple_endpoints() {
        let mut provider = provider_with_two_endpoints();
        let endpoints = provider.discover().await.unwrap();

        assert_eq!(endpoints.len(), 2);

        let mut session_a = provider.connect(&endpoints[0].id).await.unwrap();
        let mut session_b = provider.connect(&endpoints[1].id).await.unwrap();

        assert_ne!(session_a.id(), session_b.id());
        assert_ne!(session_a.endpoint_id(), session_b.endpoint_id());

        session_a.close().await.unwrap();
        session_b.close().await.unwrap();
    }

    fn provider_with_two_endpoints() -> HostProcessProvider {
        let mut provider = HostProcessProvider::new("host-process");
        provider.create_memory_endpoint("Host Process A");
        provider.create_memory_endpoint("Host Process B");
        provider
    }
}
