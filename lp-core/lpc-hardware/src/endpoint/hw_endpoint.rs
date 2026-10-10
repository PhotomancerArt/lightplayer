use alloc::string::String;

use lpc_model::HwEndpointSpec;

use crate::{HwAddress, HwEndpointId, HwEndpointKind, HwEndpointStatus};

/// Openable hardware surface reported by a driver.
///
/// An endpoint binds an authored [`HwEndpointSpec`] to a concrete
/// [`HwAddress`], a driver, and a current [`HwEndpointStatus`]. Callers open
/// endpoints through [`crate::HardwareSystem`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HwEndpoint {
    id: HwEndpointId,
    spec: HwEndpointSpec,
    kind: HwEndpointKind,
    driver_id: String,
    address: HwAddress,
    display_label: String,
    status: HwEndpointStatus,
}

impl HwEndpoint {
    pub fn new(
        id: HwEndpointId,
        spec: HwEndpointSpec,
        kind: HwEndpointKind,
        driver_id: impl Into<String>,
        address: HwAddress,
        display_label: impl Into<String>,
        status: HwEndpointStatus,
    ) -> Self {
        Self {
            id,
            spec,
            kind,
            driver_id: driver_id.into(),
            address,
            display_label: display_label.into(),
            status,
        }
    }

    pub fn id(&self) -> &HwEndpointId {
        &self.id
    }

    pub fn spec(&self) -> &HwEndpointSpec {
        &self.spec
    }

    pub fn kind(&self) -> HwEndpointKind {
        self.kind
    }

    pub fn driver_id(&self) -> &str {
        &self.driver_id
    }

    pub fn address(&self) -> &HwAddress {
        &self.address
    }

    pub fn display_label(&self) -> &str {
        &self.display_label
    }

    pub fn status(&self) -> &HwEndpointStatus {
        &self.status
    }

    pub fn is_available(&self) -> bool {
        self.status.is_available()
    }
}

/// The endpoint a lookup should use out of a stream of candidates: the first
/// available one that `matches`, else the first one that `matches` at all.
///
/// An endpoint that exists but is claimed still has to reach its driver, so it
/// fails there with the driver's own account of why. The stream is consumed
/// lazily and the walk stops at the first available match, so a driver that
/// yields endpoints one at a time never builds the ones after it.
pub fn preferred_endpoint(
    candidates: impl IntoIterator<Item = HwEndpoint>,
    matches: &dyn Fn(&HwEndpoint) -> bool,
) -> Option<HwEndpoint> {
    let mut first_match: Option<HwEndpoint> = None;
    for endpoint in candidates {
        if !matches(&endpoint) {
            continue;
        }
        if endpoint.is_available() {
            return Some(endpoint);
        }
        if first_match.is_none() {
            first_match = Some(endpoint);
        }
    }
    first_match
}
