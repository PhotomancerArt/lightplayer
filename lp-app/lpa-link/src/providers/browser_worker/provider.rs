use crate::providers::browser_worker::session::BrowserWorkerSession;
use crate::{
    LinkCapabilities, LinkEndpoint, LinkEndpointId, LinkEndpointStatus, LinkError, LinkOperation,
    LinkProvider, LinkProviderId, LinkSessionId,
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
