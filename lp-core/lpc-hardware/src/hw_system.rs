use alloc::boxed::Box;
use alloc::rc::Rc;
use alloc::vec::Vec;

use crate::{
    ButtonConfig, ButtonDriver, ButtonInput, HardwareEndpointError, HwAddress, HwEndpoint,
    HwEndpointId, HwEndpointKind, HwEndpointSpec, HwRegistry, RadioConfig, RadioDevice,
    RadioDriver, VirtualButtonDriver, VirtualRadioDriver, VirtualWs281xDriver, Ws281xConfig,
    Ws281xDriver, Ws281xOutput,
};

/// Driver registry and endpoint router for one hardware manifest.
///
/// `HardwareSystem` owns the set of registered drivers for a target. It does not
/// own resources directly; each opened device claims resources through the
/// shared [`HwRegistry`].
pub struct HardwareSystem {
    registry: Rc<HwRegistry>,
    ws281x_drivers: Vec<Box<dyn Ws281xDriver>>,
    button_drivers: Vec<Box<dyn ButtonDriver>>,
    radio_drivers: Vec<Box<dyn RadioDriver>>,
}

impl HardwareSystem {
    pub fn new(registry: Rc<HwRegistry>) -> Self {
        Self {
            registry,
            ws281x_drivers: Vec::new(),
            button_drivers: Vec::new(),
            radio_drivers: Vec::new(),
        }
    }

    pub fn with_virtual_drivers(registry: Rc<HwRegistry>) -> Self {
        let mut system = Self::new(Rc::clone(&registry));
        system.add_ws281x_driver(Box::new(VirtualWs281xDriver::new(Rc::clone(&registry), 0)));
        system.add_button_driver(Box::new(VirtualButtonDriver::new(Rc::clone(&registry))));
        system.add_radio_driver(Box::new(VirtualRadioDriver::new(Rc::clone(&registry), 0)));
        system.add_radio_driver(Box::new(VirtualRadioDriver::new_with_spec(
            registry,
            0,
            "radio:espnow:0",
        )));
        system
    }

    pub fn registry(&self) -> Rc<HwRegistry> {
        Rc::clone(&self.registry)
    }

    pub fn add_ws281x_driver(&mut self, driver: Box<dyn Ws281xDriver>) {
        self.ws281x_drivers.push(driver);
    }

    pub fn add_button_driver(&mut self, driver: Box<dyn ButtonDriver>) {
        self.button_drivers.push(driver);
    }

    pub fn add_radio_driver(&mut self, driver: Box<dyn RadioDriver>) {
        self.radio_drivers.push(driver);
    }

    pub fn ws281x_endpoints(&self) -> Vec<HwEndpoint> {
        collect_endpoints(&self.ws281x_drivers)
    }

    pub fn button_endpoints(&self) -> Vec<HwEndpoint> {
        collect_endpoints(&self.button_drivers)
    }

    pub fn radio_endpoints(&self) -> Vec<HwEndpoint> {
        collect_endpoints(&self.radio_drivers)
    }

    pub fn open_ws281x(
        &self,
        endpoint_id: &HwEndpointId,
        config: Ws281xConfig,
    ) -> Result<Box<dyn Ws281xOutput>, HardwareEndpointError> {
        for driver in &self.ws281x_drivers {
            if driver
                .endpoints()
                .iter()
                .any(|endpoint| endpoint.id() == endpoint_id)
            {
                return driver.open(endpoint_id, config);
            }
        }
        Err(HardwareEndpointError::UnknownEndpoint {
            kind: HwEndpointKind::Ws281x,
            endpoint_id: endpoint_id.clone(),
        })
    }

    pub fn open_ws281x_by_address(
        &self,
        address: &HwAddress,
        config: Ws281xConfig,
    ) -> Result<Box<dyn Ws281xOutput>, HardwareEndpointError> {
        match endpoint_for_address(self.ws281x_endpoints(), address) {
            EndpointAddressMatch::Available(endpoint) => self.open_ws281x(endpoint.id(), config),
            EndpointAddressMatch::Unavailable(endpoint) => self.open_ws281x(endpoint.id(), config),
            EndpointAddressMatch::Missing => Err(HardwareEndpointError::UnknownEndpoint {
                kind: HwEndpointKind::Ws281x,
                endpoint_id: HwEndpointId::new(address.as_str()),
            }),
        }
    }

    pub fn open_ws281x_by_spec(
        &self,
        spec: &HwEndpointSpec,
        config: Ws281xConfig,
    ) -> Result<Box<dyn Ws281xOutput>, HardwareEndpointError> {
        match endpoint_for_spec(self.ws281x_endpoints(), spec) {
            EndpointAddressMatch::Available(endpoint) => self.open_ws281x(endpoint.id(), config),
            EndpointAddressMatch::Unavailable(endpoint) => self.open_ws281x(endpoint.id(), config),
            EndpointAddressMatch::Missing => Err(HardwareEndpointError::UnknownEndpoint {
                kind: HwEndpointKind::Ws281x,
                endpoint_id: HwEndpointId::new(spec.as_str()),
            }),
        }
    }

    pub fn open_button(
        &self,
        endpoint_id: &HwEndpointId,
        config: ButtonConfig,
    ) -> Result<Box<dyn ButtonInput>, HardwareEndpointError> {
        for driver in &self.button_drivers {
            if driver
                .endpoints()
                .iter()
                .any(|endpoint| endpoint.id() == endpoint_id)
            {
                return driver.open(endpoint_id, config);
            }
        }
        Err(HardwareEndpointError::UnknownEndpoint {
            kind: HwEndpointKind::Button,
            endpoint_id: endpoint_id.clone(),
        })
    }

    pub fn open_button_by_address(
        &self,
        address: &HwAddress,
        config: ButtonConfig,
    ) -> Result<Box<dyn ButtonInput>, HardwareEndpointError> {
        match endpoint_for_address(self.button_endpoints(), address) {
            EndpointAddressMatch::Available(endpoint) => self.open_button(endpoint.id(), config),
            EndpointAddressMatch::Unavailable(endpoint) => self.open_button(endpoint.id(), config),
            EndpointAddressMatch::Missing => Err(HardwareEndpointError::UnknownEndpoint {
                kind: HwEndpointKind::Button,
                endpoint_id: HwEndpointId::new(address.as_str()),
            }),
        }
    }

    pub fn open_button_by_spec(
        &self,
        spec: &HwEndpointSpec,
        config: ButtonConfig,
    ) -> Result<Box<dyn ButtonInput>, HardwareEndpointError> {
        match endpoint_for_spec(self.button_endpoints(), spec) {
            EndpointAddressMatch::Available(endpoint) => self.open_button(endpoint.id(), config),
            EndpointAddressMatch::Unavailable(endpoint) => self.open_button(endpoint.id(), config),
            EndpointAddressMatch::Missing => Err(HardwareEndpointError::UnknownEndpoint {
                kind: HwEndpointKind::Button,
                endpoint_id: HwEndpointId::new(spec.as_str()),
            }),
        }
    }

    pub fn open_radio(
        &self,
        endpoint_id: &HwEndpointId,
        config: RadioConfig,
    ) -> Result<Box<dyn RadioDevice>, HardwareEndpointError> {
        for driver in &self.radio_drivers {
            if driver
                .endpoints()
                .iter()
                .any(|endpoint| endpoint.id() == endpoint_id)
            {
                return driver.open(endpoint_id, config);
            }
        }
        Err(HardwareEndpointError::UnknownEndpoint {
            kind: HwEndpointKind::Radio,
            endpoint_id: endpoint_id.clone(),
        })
    }

    pub fn open_radio_by_address(
        &self,
        address: &HwAddress,
        config: RadioConfig,
    ) -> Result<Box<dyn RadioDevice>, HardwareEndpointError> {
        match endpoint_for_address(self.radio_endpoints(), address) {
            EndpointAddressMatch::Available(endpoint) => self.open_radio(endpoint.id(), config),
            EndpointAddressMatch::Unavailable(endpoint) => self.open_radio(endpoint.id(), config),
            EndpointAddressMatch::Missing => Err(HardwareEndpointError::UnknownEndpoint {
                kind: HwEndpointKind::Radio,
                endpoint_id: HwEndpointId::new(address.as_str()),
            }),
        }
    }

    pub fn open_radio_by_spec(
        &self,
        spec: &HwEndpointSpec,
        config: RadioConfig,
    ) -> Result<Box<dyn RadioDevice>, HardwareEndpointError> {
        match endpoint_for_spec(self.radio_endpoints(), spec) {
            EndpointAddressMatch::Available(endpoint) => self.open_radio(endpoint.id(), config),
            EndpointAddressMatch::Unavailable(endpoint) => self.open_radio(endpoint.id(), config),
            EndpointAddressMatch::Missing => Err(HardwareEndpointError::UnknownEndpoint {
                kind: HwEndpointKind::Radio,
                endpoint_id: HwEndpointId::new(spec.as_str()),
            }),
        }
    }
}

trait EndpointDriver {
    fn endpoints(&self) -> Vec<HwEndpoint>;
}

impl EndpointDriver for Box<dyn Ws281xDriver> {
    fn endpoints(&self) -> Vec<HwEndpoint> {
        (**self).endpoints()
    }
}

impl EndpointDriver for Box<dyn ButtonDriver> {
    fn endpoints(&self) -> Vec<HwEndpoint> {
        (**self).endpoints()
    }
}

impl EndpointDriver for Box<dyn RadioDriver> {
    fn endpoints(&self) -> Vec<HwEndpoint> {
        (**self).endpoints()
    }
}

fn collect_endpoints<D>(drivers: &[D]) -> Vec<HwEndpoint>
where
    D: EndpointDriver,
{
    let mut endpoints = Vec::new();
    for driver in drivers {
        endpoints.extend(driver.endpoints());
    }
    endpoints
}

enum EndpointAddressMatch {
    Available(HwEndpoint),
    Unavailable(HwEndpoint),
    Missing,
}

fn endpoint_for_address(endpoints: Vec<HwEndpoint>, address: &HwAddress) -> EndpointAddressMatch {
    let mut first_match = None;
    for endpoint in endpoints {
        if endpoint.address() != address {
            continue;
        }
        if endpoint.is_available() {
            return EndpointAddressMatch::Available(endpoint);
        }
        if first_match.is_none() {
            first_match = Some(endpoint);
        }
    }
    match first_match {
        Some(endpoint) => EndpointAddressMatch::Unavailable(endpoint),
        None => EndpointAddressMatch::Missing,
    }
}

fn endpoint_for_spec(endpoints: Vec<HwEndpoint>, spec: &HwEndpointSpec) -> EndpointAddressMatch {
    let mut first_match = None;
    for endpoint in endpoints {
        if endpoint.spec() != spec {
            continue;
        }
        if endpoint.is_available() {
            return EndpointAddressMatch::Available(endpoint);
        }
        if first_match.is_none() {
            first_match = Some(endpoint);
        }
    }
    match first_match {
        Some(endpoint) => EndpointAddressMatch::Unavailable(endpoint),
        None => EndpointAddressMatch::Missing,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{HwCapability, HwManifest, HwResource};

    #[test]
    fn virtual_system_lists_three_capability_families() {
        let registry = Rc::new(HwRegistry::new(HwManifest::virtual_single_rmt_gpio_board()));
        let system = HardwareSystem::with_virtual_drivers(registry);

        assert!(!system.ws281x_endpoints().is_empty());
        assert!(!system.button_endpoints().is_empty());
        assert_eq!(system.radio_endpoints().len(), 2);
    }

    #[test]
    fn virtual_system_opens_ws281x_by_gpio_address() {
        let registry = Rc::new(HwRegistry::new(HwManifest::virtual_single_rmt_gpio_board()));
        let system = HardwareSystem::with_virtual_drivers(Rc::clone(&registry));
        let output = system
            .open_ws281x_by_address(&HwAddress::gpio(18), Ws281xConfig::new(3))
            .unwrap();

        assert!(registry.is_claimed(&HwAddress::gpio(18)));
        assert!(registry.is_claimed(&HwAddress::rmt_ws281x(0)));

        drop(output);

        assert!(!registry.is_claimed(&HwAddress::gpio(18)));
        assert!(!registry.is_claimed(&HwAddress::rmt_ws281x(0)));
    }

    #[test]
    fn virtual_system_opens_ws281x_by_endpoint_spec() {
        let registry = Rc::new(HwRegistry::new(HwManifest::virtual_single_rmt_gpio_board()));
        let system = HardwareSystem::with_virtual_drivers(Rc::clone(&registry));
        let spec = HwEndpointSpec::from_static("ws281x:rmt:D10");
        let output = system
            .open_ws281x_by_spec(&spec, Ws281xConfig::new(3))
            .unwrap();

        assert!(registry.is_claimed(&HwAddress::gpio(18)));
        assert!(registry.is_claimed(&HwAddress::rmt_ws281x(0)));

        drop(output);

        assert!(!registry.is_claimed(&HwAddress::gpio(18)));
        assert!(!registry.is_claimed(&HwAddress::rmt_ws281x(0)));
    }

    #[test]
    fn virtual_system_reports_unknown_ws281x_endpoint_spec() {
        let registry = Rc::new(HwRegistry::new(HwManifest::virtual_single_rmt_gpio_board()));
        let system = HardwareSystem::with_virtual_drivers(registry);
        let spec = HwEndpointSpec::from_static("ws281x:rmt:NOPE");

        let result = system.open_ws281x_by_spec(&spec, Ws281xConfig::new(3));

        assert!(matches!(
            result,
            Err(HardwareEndpointError::UnknownEndpoint { .. })
        ));
    }

    #[test]
    fn virtual_system_opens_button_by_endpoint_spec() {
        let registry = Rc::new(HwRegistry::new(test_manifest()));
        let mut system = HardwareSystem::new(Rc::clone(&registry));
        let driver = VirtualButtonDriver::new(Rc::clone(&registry));
        let control = driver.clone();
        system.add_button_driver(Box::new(driver));
        let spec = HwEndpointSpec::from_static("button:gpio:GPIO4");
        let mut input = system
            .open_button_by_spec(&spec, ButtonConfig::new(10))
            .unwrap();

        control.set_pressed(HwAddress::gpio(4), true);
        assert!(input.poll(0).is_none());
        assert!(input.poll(10).is_some());
    }

    #[test]
    fn virtual_button_and_ws281x_contend_for_same_gpio() {
        let registry = Rc::new(HwRegistry::new(test_manifest()));
        let system = HardwareSystem::with_virtual_drivers(Rc::clone(&registry));
        let _button = system
            .open_button_by_address(&HwAddress::gpio(4), ButtonConfig::default())
            .unwrap();

        let result = system.open_ws281x_by_address(&HwAddress::gpio(4), Ws281xConfig::new(3));

        assert!(matches!(
            result,
            Err(HardwareEndpointError::EndpointUnavailable { .. })
                | Err(HardwareEndpointError::Hardware { .. })
        ));
    }

    fn test_manifest() -> HwManifest {
        HwManifest::new(
            "test",
            "Test Board",
            [
                HwResource::new(
                    HwAddress::gpio(4),
                    [HwCapability::GpioOutput, HwCapability::GpioInput],
                    "GPIO4",
                ),
                HwResource::new(
                    HwAddress::rmt_ws281x(0),
                    [HwCapability::Rmt, HwCapability::Ws281xOutput],
                    "RMT WS281x 0",
                ),
                HwResource::new(HwAddress::radio(0), [HwCapability::Radio], "Radio 0"),
            ],
        )
    }
}
