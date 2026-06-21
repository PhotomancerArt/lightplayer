use crate::providers::fake::session::FakeSession;
use crate::{
    LinkEndpoint, LinkEndpointId, LinkEndpointStatus, LinkError, LinkProvider, LinkProviderId,
    LinkSessionId,
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
