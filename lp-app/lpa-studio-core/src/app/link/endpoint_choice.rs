use lpa_link::{LinkEndpoint, LinkEndpointId, LinkEndpointStatus, LinkProviderKind};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EndpointChoice {
    pub provider_id: LinkProviderKind,
    pub id: LinkEndpointId,
    pub label: String,
    pub summary: String,
    pub status: LinkEndpointStatus,
}

impl EndpointChoice {
    pub fn from_endpoint(endpoint: LinkEndpoint) -> Self {
        let summary = endpoint_summary(&endpoint);
        Self {
            provider_id: endpoint.provider_kind,
            id: endpoint.id,
            label: endpoint.label,
            summary,
            status: endpoint.status,
        }
    }

    #[cfg(any(test, feature = "browser-worker"))]
    pub fn browser_worker() -> Self {
        Self {
            provider_id: LinkProviderKind::BrowserWorker,
            id: LinkEndpointId::new("browser-worker-worker-1"),
            label: "Browser firmware runtime".to_string(),
            summary: "Spawn a browser-local firmware runtime.".to_string(),
            status: LinkEndpointStatus::Available,
        }
    }
}

fn endpoint_summary(endpoint: &LinkEndpoint) -> String {
    match endpoint.provider_kind {
        LinkProviderKind::BrowserWorker => "Spawn a browser-local firmware runtime.".to_string(),
        LinkProviderKind::HostProcess => "Spawn a host-local firmware runtime.".to_string(),
        LinkProviderKind::BrowserSerialEsp32 | LinkProviderKind::HostSerialEsp32 => {
            "Open this ESP32 endpoint.".to_string()
        }
        LinkProviderKind::Fake => "Open this test endpoint.".to_string(),
    }
}
