use alloc::string::String;

use lpc_model::HardwareEndpointSpec;

use super::{HardwareAddress, HardwareEndpointId, HardwareEndpointKind, HardwareEndpointStatus};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HardwareEndpoint {
    id: HardwareEndpointId,
    spec: HardwareEndpointSpec,
    kind: HardwareEndpointKind,
    driver_id: String,
    address: HardwareAddress,
    display_label: String,
    status: HardwareEndpointStatus,
}

impl HardwareEndpoint {
    pub fn new(
        id: HardwareEndpointId,
        spec: HardwareEndpointSpec,
        kind: HardwareEndpointKind,
        driver_id: impl Into<String>,
        address: HardwareAddress,
        display_label: impl Into<String>,
        status: HardwareEndpointStatus,
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

    pub fn id(&self) -> &HardwareEndpointId {
        &self.id
    }

    pub fn spec(&self) -> &HardwareEndpointSpec {
        &self.spec
    }

    pub fn kind(&self) -> HardwareEndpointKind {
        self.kind
    }

    pub fn driver_id(&self) -> &str {
        &self.driver_id
    }

    pub fn address(&self) -> &HardwareAddress {
        &self.address
    }

    pub fn display_label(&self) -> &str {
        &self.display_label
    }

    pub fn status(&self) -> &HardwareEndpointStatus {
        &self.status
    }

    pub fn is_available(&self) -> bool {
        self.status.is_available()
    }
}
