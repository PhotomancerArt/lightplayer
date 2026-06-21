use crate::{
    LinkConnection, LinkDiagnostic, LinkDiagnosticSeverity, LinkEndpoint, LinkEndpointId,
    LinkEndpointStatus, LinkError, LinkLogEntry, LinkLogLevel, LinkManagement, LinkProvider,
    LinkProviderId, LinkSession, LinkSessionId,
};

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
            .with_management(LinkManagement::esp32_serial_base().with_flash());
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

#[derive(Clone, Debug)]
pub struct BrowserSerialEsp32Session {
    endpoint_id: LinkEndpointId,
    id: LinkSessionId,
    closed: bool,
    logs: Vec<LinkLogEntry>,
    diagnostics: Vec<LinkDiagnostic>,
}

impl BrowserSerialEsp32Session {
    pub fn new(endpoint_id: LinkEndpointId, id: LinkSessionId) -> Self {
        let logs = vec![LinkLogEntry::new(
            endpoint_id.clone(),
            Some(id.clone()),
            LinkLogLevel::Info,
            "browser serial ESP32 session created",
        )];
        let diagnostics = vec![LinkDiagnostic::new(
            endpoint_id.clone(),
            Some(id.clone()),
            LinkDiagnosticSeverity::Info,
            "browser serial session modeled; Studio web owns the Web Serial stream binding",
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

impl LinkSession for BrowserSerialEsp32Session {
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
        Ok(LinkConnection::browser_serial_esp32(
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
            "browser serial ESP32 session closed",
        ));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::{LinkConnectionKind, LinkManagementOperation};

    use super::*;

    #[tokio::test]
    async fn browser_serial_provider_models_granted_ports() {
        let mut provider = BrowserSerialEsp32Provider::new("browser-serial-esp32");
        let endpoint_id = provider.create_granted_endpoint("ESP32-C6");

        let endpoints = provider.discover().await.unwrap();

        assert_eq!(endpoints.len(), 1);
        assert_eq!(endpoints[0].id, endpoint_id);
        assert!(
            endpoints[0]
                .management
                .supports(LinkManagementOperation::Reset)
        );
        assert!(
            endpoints[0]
                .management
                .supports(LinkManagementOperation::ReadLogs)
        );
        assert!(
            endpoints[0]
                .management
                .supports(LinkManagementOperation::FlashFirmware)
        );
    }

    #[tokio::test]
    async fn browser_serial_connection_reports_protocol() {
        let mut provider = BrowserSerialEsp32Provider::new("browser-serial-esp32");
        let endpoint_id = provider.create_granted_endpoint("ESP32-C6");
        let mut session = provider.connect(&endpoint_id).await.unwrap();

        let connection = session.connection().await.unwrap();

        assert!(matches!(
            connection.kind,
            LinkConnectionKind::BrowserSerialEsp32 { ref protocol }
                if protocol == "lp-serial-json-lines-v1"
        ));
    }
}
