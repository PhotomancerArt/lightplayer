use alloc::string::String;
use alloc::vec::Vec;

use super::{HardwareAddress, HardwareCapability, HardwareResource};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HardwareManifest {
    board_id: String,
    board_name: String,
    resources: Vec<HardwareResource>,
}

impl HardwareManifest {
    pub fn new(
        board_id: impl Into<String>,
        board_name: impl Into<String>,
        resources: impl Into<Vec<HardwareResource>>,
    ) -> Self {
        Self {
            board_id: board_id.into(),
            board_name: board_name.into(),
            resources: resources.into(),
        }
    }

    pub fn virtual_single_rmt_gpio_board() -> Self {
        let mut resources = Vec::new();
        for pin in 0..=255 {
            resources.push(HardwareResource::new(
                HardwareAddress::gpio(pin),
                [
                    HardwareCapability::GpioOutput,
                    HardwareCapability::GpioInput,
                ],
                alloc::format!("GPIO{pin}"),
            ));
        }
        resources.push(HardwareResource::new(
            HardwareAddress::rmt_ws281x(0),
            [HardwareCapability::Rmt, HardwareCapability::Ws281xOutput],
            "RMT WS281x 0",
        ));
        Self::new("virtual-single-rmt", "Virtual Single-RMT Board", resources)
    }

    pub fn board_id(&self) -> &str {
        &self.board_id
    }

    pub fn board_name(&self) -> &str {
        &self.board_name
    }

    pub fn resources(&self) -> &[HardwareResource] {
        &self.resources
    }

    pub fn resource(&self, address: &HardwareAddress) -> Option<&HardwareResource> {
        self.resources
            .iter()
            .find(|resource| resource.address() == address)
    }

    pub fn with_reserved(mut self, address: HardwareAddress, reason: impl Into<String>) -> Self {
        let reason = reason.into();
        if let Some(resource) = self
            .resources
            .iter_mut()
            .find(|resource| resource.address() == &address)
        {
            *resource = resource.clone().reserved(reason);
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_resource_by_internal_address_not_label() {
        let manifest = HardwareManifest::new(
            "board",
            "Board",
            [HardwareResource::new(
                HardwareAddress::gpio(18),
                [HardwareCapability::GpioOutput],
                "D6",
            )],
        );

        let resource = manifest.resource(&HardwareAddress::gpio(18)).unwrap();
        assert_eq!(resource.display_label(), "D6");
    }
}
