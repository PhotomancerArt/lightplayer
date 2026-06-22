use lpa_link::LinkProviderKind;
use lpa_link::providers::{LinkProviderAvailability, LinkProviderDescriptor};

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
            summary: provider_summary(descriptor.kind, descriptor.availability),
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

fn provider_label(kind: LinkProviderKind, fallback: &str) -> String {
    match kind {
        LinkProviderKind::BrowserWorker => "Simulator".to_string(),
        LinkProviderKind::HostProcess => "Host runtime".to_string(),
        _ => fallback.to_string(),
    }
}

fn provider_summary(kind: LinkProviderKind, availability: LinkProviderAvailability) -> String {
    if let LinkProviderAvailability::Unavailable { reason } = availability {
        return reason.to_string();
    }
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
