use crate::{
    LinkConnection, LinkDiagnostic, LinkDiagnosticSeverity, LinkEndpoint, LinkEndpointId,
    LinkEndpointStatus, LinkError, LinkLogEntry, LinkLogLevel, LinkManagement, LinkProvider,
    LinkProviderId, LinkSession, LinkSessionId,
};

#[derive(Clone, Debug)]
pub struct BrowserWorkerProvider {
    id: LinkProviderId,
    endpoints: Vec<LinkEndpoint>,
    next_endpoint_index: u64,
    next_session_index: u64,
}

impl BrowserWorkerProvider {
    pub fn new(id: impl Into<LinkProviderId>) -> Self {
        Self {
            id: id.into(),
            endpoints: Vec::new(),
            next_endpoint_index: 1,
            next_session_index: 1,
        }
    }

    pub fn create_worker_endpoint(&mut self, label: impl Into<String>) -> LinkEndpointId {
        let endpoint_id = LinkEndpointId::new(format!(
            "{}-worker-{}",
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

impl LinkProvider for BrowserWorkerProvider {
    type Session = BrowserWorkerSession;

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
        let session_id = LinkSessionId::new(format!(
            "{}:{}",
            endpoint_id.as_str(),
            self.next_session_index
        ));
        self.next_session_index += 1;
        Ok(BrowserWorkerSession::new(endpoint.id, session_id))
    }
}

#[derive(Clone, Debug)]
pub struct BrowserWorkerSession {
    endpoint_id: LinkEndpointId,
    id: LinkSessionId,
    closed: bool,
    logs: Vec<LinkLogEntry>,
    diagnostics: Vec<LinkDiagnostic>,
}

impl BrowserWorkerSession {
    pub fn new(endpoint_id: LinkEndpointId, id: LinkSessionId) -> Self {
        let logs = vec![LinkLogEntry::new(
            endpoint_id.clone(),
            Some(id.clone()),
            LinkLogLevel::Info,
            "browser worker session created",
        )];
        let diagnostics = vec![LinkDiagnostic::new(
            endpoint_id.clone(),
            Some(id.clone()),
            LinkDiagnosticSeverity::Info,
            "browser worker session ready; Studio web owns Worker postMessage binding",
        )];
        Self {
            endpoint_id,
            id,
            closed: false,
            logs,
            diagnostics,
        }
    }
}

impl LinkSession for BrowserWorkerSession {
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
        if self.closed {
            return Err(LinkError::Closed);
        }
        Ok(LinkConnection::browser_worker(
            self.endpoint_id.clone(),
            self.id.clone(),
        ))
    }

    async fn close(&mut self) -> Result<(), LinkError> {
        self.closed = true;
        self.logs.push(LinkLogEntry::new(
            self.endpoint_id.clone(),
            Some(self.id.clone()),
            LinkLogLevel::Info,
            "browser worker session closed",
        ));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::LinkConnectionKind;

    use super::*;

    #[tokio::test]
    async fn browser_worker_provider_supports_multiple_worker_endpoints() {
        let mut provider = BrowserWorkerProvider::new("browser-worker");
        provider.create_worker_endpoint("Browser A");
        provider.create_worker_endpoint("Browser B");

        let endpoints = provider.discover().await.unwrap();
        assert_eq!(endpoints.len(), 2);

        let session_a = provider.connect(&endpoints[0].id).await.unwrap();
        let session_b = provider.connect(&endpoints[1].id).await.unwrap();

        assert_ne!(session_a.id(), session_b.id());
        assert_ne!(session_a.endpoint_id(), session_b.endpoint_id());
    }

    #[tokio::test]
    async fn browser_worker_connection_reports_worker_protocol() {
        let mut provider = BrowserWorkerProvider::new("browser-worker");
        let endpoint_id = provider.create_worker_endpoint("Browser A");
        let mut session = provider.connect(&endpoint_id).await.unwrap();

        let connection = session.connection().await.unwrap();

        assert_eq!(connection.endpoint_id, endpoint_id);
        assert!(matches!(
            connection.kind,
            LinkConnectionKind::BrowserWorker { ref protocol }
                if protocol == "fw-browser-post-message-v1"
        ));
    }
}
