use alloc::string::String;
use alloc::vec::Vec;

use super::{HardwareAddress, HardwareCapability, HardwareResource, HardwareTarget};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HardwareManifest {
    board_id: String,
    board_name: String,
    target: Option<HardwareTarget>,
    vendor: Option<String>,
    product: Option<String>,
    description: Option<String>,
    url: Option<String>,
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
            target: None,
            vendor: None,
            product: None,
            description: None,
            url: None,
            resources: resources.into(),
        }
    }

    pub fn virtual_single_rmt_gpio_board() -> Self {
        let mut resources = Vec::new();
        for pin in 0..=255 {
            let display_label = if pin == 18 {
                alloc::format!("D10")
            } else {
                alloc::format!("GPIO{pin}")
            };
            resources.push(HardwareResource::new(
                HardwareAddress::gpio(pin),
                [
                    HardwareCapability::GpioOutput,
                    HardwareCapability::GpioInput,
                ],
                display_label,
            ));
        }
        resources.push(HardwareResource::new(
            HardwareAddress::rmt_ws281x(0),
            [HardwareCapability::Rmt, HardwareCapability::Ws281xOutput],
            "RMT WS281x 0",
        ));
        resources.push(HardwareResource::new(
            HardwareAddress::radio(0),
            [HardwareCapability::Radio],
            "Virtual Radio 0",
        ));
        Self::new("virtual-single-rmt", "Virtual Single-RMT Board", resources)
            .with_target(HardwareTarget::Rv32imacEmu)
            .with_description("Virtual board profile for tests and emulation with GPIO resources, one shared WS281x/RMT resource, and one radio endpoint.")
    }

    pub fn board_id(&self) -> &str {
        &self.board_id
    }

    pub fn board_name(&self) -> &str {
        &self.board_name
    }

    pub fn target(&self) -> Option<HardwareTarget> {
        self.target
    }

    pub fn vendor(&self) -> Option<&str> {
        self.vendor.as_deref()
    }

    pub fn product(&self) -> Option<&str> {
        self.product.as_deref()
    }

    pub fn description(&self) -> Option<&str> {
        self.description.as_deref()
    }

    pub fn url(&self) -> Option<&str> {
        self.url.as_deref()
    }

    pub fn resources(&self) -> &[HardwareResource] {
        &self.resources
    }

    pub fn with_target(mut self, target: HardwareTarget) -> Self {
        self.target = Some(target);
        self
    }

    pub fn with_vendor(mut self, vendor: impl Into<String>) -> Self {
        self.vendor = Some(vendor.into());
        self
    }

    pub fn with_product(mut self, product: impl Into<String>) -> Self {
        self.product = Some(product.into());
        self
    }

    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    pub fn with_url(mut self, url: impl Into<String>) -> Self {
        self.url = Some(url.into());
        self
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

    pub fn map_resources(mut self, map_fn: impl Fn(HardwareResource) -> HardwareResource) -> Self {
        self.resources = self.resources.into_iter().map(map_fn).collect();
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

    #[test]
    fn stores_optional_board_metadata() {
        let manifest = HardwareManifest::new("board", "Board", [])
            .with_target(HardwareTarget::Esp32c6)
            .with_vendor("vendor")
            .with_product("product")
            .with_description("A board profile")
            .with_url("https://example.com/board");

        assert_eq!(manifest.target(), Some(HardwareTarget::Esp32c6));
        assert_eq!(manifest.vendor(), Some("vendor"));
        assert_eq!(manifest.product(), Some("product"));
        assert_eq!(manifest.description(), Some("A board profile"));
        assert_eq!(manifest.url(), Some("https://example.com/board"));
    }
}
