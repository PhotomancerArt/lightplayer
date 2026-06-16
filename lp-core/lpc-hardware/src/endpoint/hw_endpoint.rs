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
