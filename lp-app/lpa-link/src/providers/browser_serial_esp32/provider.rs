use crate::link_endpoint::{LinkEndpointId, LinkEndpointStatus};
use crate::link_provider::LinkProviderId;
use crate::link_session::LinkSessionId;
use crate::providers::browser_serial_esp32::session::BrowserSerialEsp32Session;
use crate::{LinkCapabilities, LinkEndpoint, LinkError, LinkProvider};

#[derive(Clone, Debug)]
pub struct BrowserSerialEsp32Provider {
    id: LinkProviderId,
    endpoints: Vec<LinkEndpoint>,
    next_endpoint_index: u64,
    next_session_index: u64,
}

impl BrowserSerialEsp32Provider {
    pub fn new(id: impl Into<LinkProviderId>) -> Self {
        Self {
            id: id.into(),
            endpoints: Vec::new(),
            next_endpoint_index: 1,
            next_session_index: 1,
        }
    }

    /// Record a browser-granted ESP32 serial endpoint.
    ///
    /// This provider intentionally does not request Web Serial access itself.
    /// The web runtime owns the user-gesture-bound `requestPort()` call and then
    /// records the resulting endpoint here for Studio/link modeling.
    pub fn create_granted_endpoint(&mut self, label: impl Into<String>) -> LinkEndpointId {
        let endpoint_id = LinkEndpointId::new(format!(
            "{}-port-{}",
            self.id.as_str(),
            self.next_endpoint_index
        ));
        self.next_endpoint_index += 1;

        let endpoint = LinkEndpoint::new(endpoint_id.clone(), self.id.clone(), label)
            .with_capabilities(LinkCapabilities::esp32_serial_base().with_flash());
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

impl LinkProvider for BrowserSerialEsp32Provider {
    type Session = BrowserSerialEsp32Session;

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
        Ok(BrowserSerialEsp32Session::new(endpoint.id, session_id))
    }
}
