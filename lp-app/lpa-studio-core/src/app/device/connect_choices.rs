//! Picker view models derived from the link CATALOG.
//!
//! "What can I connect to?" — the deploy dialog and the connect flow read
//! these choices until M7 redesigns connect UX. Everything here derives
//! from [`LinkProviderRegistry`] descriptors; nothing holds live provider
//! state.

use lpa_link::providers::{LinkProviderDescriptor, LinkProviderRegistry};
use lpa_link::{LinkEndpoint, LinkEndpointId, LinkEndpointStatus, LinkOperation, LinkProviderKind};

/// One provider the picker can open.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderChoice {
    pub id: LinkProviderKind,
    pub label: String,
    pub summary: String,
}

impl ProviderChoice {
    pub fn from_descriptor(descriptor: LinkProviderDescriptor) -> Self {
        Self {
            id: descriptor.kind,
            label: provider_label(descriptor.kind, descriptor.label),
            summary: provider_summary(descriptor.kind),
        }
    }

    #[cfg(any(test, feature = "browser-worker"))]
    pub fn browser_worker() -> Self {
        Self {
            id: LinkProviderKind::BrowserWorker,
            label: "Simulator".to_string(),
            summary: "Run LightPlayer locally in a browser worker.".to_string(),
        }
    }
}

/// One endpoint a provider discovered.
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

/// The provider choices the picker shows for this build's catalog.
pub(crate) fn provider_choices(registry: &LinkProviderRegistry) -> Vec<ProviderChoice> {
    let descriptors = registry.descriptors();
    let server_descriptors = descriptors
        .iter()
        .filter(|descriptor| provider_can_open_server(descriptor.kind))
        .cloned()
        .collect::<Vec<_>>();
    let visible_descriptors = if server_descriptors.is_empty() {
        descriptors
    } else {
        server_descriptors
    };
    visible_descriptors
        .into_iter()
        .map(ProviderChoice::from_descriptor)
        .collect()
}

/// Catalog descriptors for HARDWARE device classes: providers whose class
/// can flash firmware (the deploy dialog's connect + recovery affordances
/// are flash-shaped). Simulators and diagnostics-only classes never
/// surface here.
pub(crate) fn hardware_device_descriptors(
    registry: &LinkProviderRegistry,
) -> Vec<LinkProviderDescriptor> {
    registry
        .descriptors()
        .into_iter()
        .filter(|descriptor| {
            descriptor
                .capabilities
                .supports(LinkOperation::FlashFirmware)
        })
        .collect()
}

/// Whether the provider class can hand a server protocol connection to the
/// studio (everything but the record-level fake today).
fn provider_can_open_server(kind: LinkProviderKind) -> bool {
    matches!(
        kind,
        LinkProviderKind::BrowserWorker
            | LinkProviderKind::HostProcess
            | LinkProviderKind::BrowserSerialEsp32
            | LinkProviderKind::HostSerialEsp32
    )
}

/// Whether `open_provider` auto-connects a single discovered endpoint.
pub(crate) fn provider_auto_connects(kind: LinkProviderKind) -> bool {
    matches!(
        kind,
        LinkProviderKind::BrowserWorker | LinkProviderKind::HostProcess
    )
}

fn provider_label(kind: LinkProviderKind, fallback: &str) -> String {
    match kind {
        LinkProviderKind::BrowserWorker => "Simulator".to_string(),
        LinkProviderKind::HostProcess => "Host runtime".to_string(),
        _ => fallback.to_string(),
    }
}

fn provider_summary(kind: LinkProviderKind) -> String {
    match kind {
        LinkProviderKind::BrowserWorker => {
            "Run LightPlayer locally in a browser worker.".to_string()
        }
        LinkProviderKind::HostProcess => "Run LightPlayer locally in a host process.".to_string(),
        LinkProviderKind::BrowserSerialEsp32 => {
            "Connect to ESP32 hardware through browser Web Serial.".to_string()
        }
        LinkProviderKind::HostSerialEsp32 => {
            "Connect to ESP32 hardware through a host serial port.".to_string()
        }
        LinkProviderKind::Fake => "Use an in-memory test link provider.".to_string(),
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
