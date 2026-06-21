use crate::link_endpoint::{LinkEndpointId, LinkEndpointStatus};
use crate::link_provider::LinkProviderId;
use crate::link_session::LinkSessionId;
use crate::providers::host_process::session::HostProcessSession;
use crate::{LinkCapabilities, LinkEndpoint, LinkError, LinkOperation, LinkProvider};
use fw_host::HostRuntime;

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
