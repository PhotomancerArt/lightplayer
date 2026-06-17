use crate::{
    LinkConnection, LinkDiagnostic, LinkDiagnosticSeverity, LinkEndpoint, LinkEndpointId,
    LinkEndpointStatus, LinkError, LinkLogEntry, LinkLogLevel, LinkProvider, LinkProviderId,
    LinkSession, LinkSessionId,
};

#[derive(Clone, Debug)]
pub struct FakeProvider {
    id: LinkProviderId,
    endpoints: Vec<LinkEndpoint>,
    next_session_index: u64,
}

impl FakeProvider {
    pub fn new(id: impl Into<LinkProviderId>) -> Self {
        Self {
            id: id.into(),
            endpoints: Vec::new(),
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
}

impl LinkProvider for FakeProvider {
    type Session = FakeSession;

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

        Ok(FakeSession::new(endpoint.id, session_id))
    }
}

#[derive(Clone, Debug)]
pub struct FakeSession {
    endpoint_id: LinkEndpointId,
    id: LinkSessionId,
    closed: bool,
    logs: Vec<LinkLogEntry>,
    diagnostics: Vec<LinkDiagnostic>,
}

impl FakeSession {
    pub fn new(endpoint_id: LinkEndpointId, id: LinkSessionId) -> Self {
        let logs = vec![LinkLogEntry::new(
            endpoint_id.clone(),
            Some(id.clone()),
            LinkLogLevel::Info,
            "fake link session opened",
        )];
        let diagnostics = vec![LinkDiagnostic::new(
            endpoint_id.clone(),
            Some(id.clone()),
            LinkDiagnosticSeverity::Info,
            "fake link session ready",
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

impl LinkSession for FakeSession {
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

        Ok(LinkConnection::fake(
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
            "fake link session closed",
        ));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::LinkManagement;

    #[tokio::test]
    async fn discover_returns_all_fake_endpoints() {
        let mut provider = fake_provider();

        let endpoints = provider.discover().await.unwrap();

        assert_eq!(endpoints.len(), 2);
        assert_eq!(endpoints[0].id.as_str(), "fake-a");
        assert_eq!(endpoints[1].id.as_str(), "fake-b");
    }

    #[tokio::test]
    async fn sessions_are_scoped_to_endpoint_and_have_stable_ids() {
        let mut provider = fake_provider();
        let endpoint_a = LinkEndpointId::new("fake-a");
        let endpoint_b = LinkEndpointId::new("fake-b");

        let mut session_a = provider.connect(&endpoint_a).await.unwrap();
        let session_b = provider.connect(&endpoint_b).await.unwrap();

        assert_eq!(session_a.endpoint_id().as_str(), "fake-a");
        assert_eq!(session_b.endpoint_id().as_str(), "fake-b");
        assert_ne!(session_a.id(), session_b.id());

        let connection = session_a.connection().await.unwrap();
        assert_eq!(connection.endpoint_id.as_str(), "fake-a");
        assert_eq!(connection.session_id, session_a.id().clone());
    }

    #[tokio::test]
    async fn logs_and_diagnostics_are_scoped_to_session() {
        let mut provider = fake_provider();
        let mut session = provider
            .connect(&LinkEndpointId::new("fake-a"))
            .await
            .unwrap();

        let logs = session.logs();
        let diagnostics = session.diagnostics();

        assert_eq!(logs[0].endpoint_id.as_str(), "fake-a");
        assert_eq!(logs[0].session_id, Some(session.id().clone()));
        assert_eq!(diagnostics[0].endpoint_id.as_str(), "fake-a");
        assert_eq!(diagnostics[0].session_id, Some(session.id().clone()));

        session.close().await.unwrap();
        assert!(session.connection().await.is_err());
    }

    fn fake_provider() -> FakeProvider {
        let provider_id = LinkProviderId::new("fake");
        FakeProvider::new(provider_id.clone())
            .with_endpoint(
                LinkEndpoint::new("fake-a", provider_id.clone(), "Fake A")
                    .with_management(LinkManagement::diagnostics_only()),
            )
            .with_endpoint(LinkEndpoint::new("fake-b", provider_id, "Fake B"))
    }
}
