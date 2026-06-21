use serde::{Deserialize, Serialize};

use crate::{LinkCapabilities, LinkEndpointId, LinkEndpointStatus, LinkProviderId};

/// A provider-visible target that can be connected to.
///
/// An endpoint is a candidate target, not a live connection. It is returned by
/// `LinkProvider::discover()` and describes what can be opened: a serial port,
/// a browser worker runtime, a host process runtime template, or a future
/// websocket target.
///
/// Endpoints are not always physical devices. `host-process`, for example,
/// exposes spawnable host runtime endpoints: connecting to one creates a new
/// in-process `fw-host` runtime session.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct LinkEndpoint {
    pub id: LinkEndpointId,
    pub provider_id: LinkProviderId,
    pub label: String,
    pub status: LinkEndpointStatus,
    pub capabilities: LinkCapabilities,
}

impl LinkEndpoint {
    pub fn new(
        id: impl Into<LinkEndpointId>,
        provider_id: impl Into<LinkProviderId>,
        label: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            provider_id: provider_id.into(),
            label: label.into(),
            status: LinkEndpointStatus::Available,
            capabilities: LinkCapabilities::default(),
        }
    }

    pub fn with_status(mut self, status: LinkEndpointStatus) -> Self {
        self.status = status;
        self
    }

    pub fn with_capabilities(mut self, capabilities: LinkCapabilities) -> Self {
        self.capabilities = capabilities;
        self
    }
}
