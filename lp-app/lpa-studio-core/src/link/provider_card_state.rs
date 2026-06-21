use crate::{ProviderAvailability, ProviderCapability, ProviderIntent};
use lpa_link::LinkEndpoint;
use lpa_link::LinkProviderKind;
use serde::{Deserialize, Serialize};

/// Studio-facing card/profile for a link provider.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct ProviderCardState {
    pub provider_id: LinkProviderKind,
    pub label: String,
    pub intent: ProviderIntent,
    pub availability: ProviderAvailability,
    pub capabilities: Vec<ProviderCapability>,
    pub endpoints: Vec<LinkEndpoint>,
}

impl ProviderCardState {
    pub fn new(
        provider_id: impl Into<LinkProviderKind>,
        label: impl Into<String>,
        intent: ProviderIntent,
    ) -> Self {
        Self {
            provider_id: provider_id.into(),
            label: label.into(),
            intent,
            availability: ProviderAvailability::Available,
            capabilities: Vec::new(),
            endpoints: Vec::new(),
        }
    }

    pub fn with_availability(mut self, availability: ProviderAvailability) -> Self {
        self.availability = availability;
        self
    }

    pub fn with_capabilities(mut self, capabilities: Vec<ProviderCapability>) -> Self {
        self.capabilities = capabilities;
        self
    }

    pub fn with_endpoints(mut self, endpoints: Vec<LinkEndpoint>) -> Self {
        self.endpoints = endpoints;
        self
    }
}
