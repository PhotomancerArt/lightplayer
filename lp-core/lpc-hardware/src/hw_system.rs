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
        system.add_ws281x_driver(Box::new(VirtualWs281xDriver::new(Rc::clone(&registry))));
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
        match find_endpoint(&self.ws281x_drivers, |endpoint| {
            endpoint.address() == address
        }) {
            Some((driver, endpoint_id)) => self.ws281x_drivers[driver].open(&endpoint_id, config),
            None => Err(HardwareEndpointError::UnknownEndpoint {
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
        match find_endpoint(&self.ws281x_drivers, |endpoint| endpoint.spec() == spec) {
            Some((driver, endpoint_id)) => self.ws281x_drivers[driver].open(&endpoint_id, config),
            None => Err(HardwareEndpointError::UnknownEndpoint {
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
        match find_endpoint(&self.button_drivers, |endpoint| {
            endpoint.address() == address
        }) {
            Some((driver, endpoint_id)) => self.button_drivers[driver].open(&endpoint_id, config),
            None => Err(HardwareEndpointError::UnknownEndpoint {
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
        match find_endpoint(&self.button_drivers, |endpoint| endpoint.spec() == spec) {
            Some((driver, endpoint_id)) => self.button_drivers[driver].open(&endpoint_id, config),
            None => Err(HardwareEndpointError::UnknownEndpoint {
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
        match find_endpoint(&self.radio_drivers, |endpoint| {
            endpoint.address() == address
        }) {
            Some((driver, endpoint_id)) => self.radio_drivers[driver].open(&endpoint_id, config),
            None => Err(HardwareEndpointError::UnknownEndpoint {
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
        match find_endpoint(&self.radio_drivers, |endpoint| endpoint.spec() == spec) {
            Some((driver, endpoint_id)) => self.radio_drivers[driver].open(&endpoint_id, config),
            None => Err(HardwareEndpointError::UnknownEndpoint {
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

/// The driver offering the wanted endpoint, as an index into `drivers`, with
/// that endpoint's id.
///
/// Prefers an available endpoint and otherwise reports the first match, so an
/// endpoint that exists but is claimed still reaches its driver and fails with
/// that driver's own account of why.
///
/// Enumerating a driver costs a formatted spec and a live status lookup *per
/// endpoint it offers* — on a board declaring every GPIO, hundreds of them. So
/// the walk stops at the first available match and the caller opens on the
/// driver found here, rather than enumerating once to pick an endpoint and
/// again to discover which driver owns it.
fn find_endpoint<D>(
    drivers: &[D],
    matches: impl Fn(&HwEndpoint) -> bool,
) -> Option<(usize, HwEndpointId)>
where
    D: EndpointDriver,
{
    let mut first_match: Option<(usize, HwEndpointId)> = None;
    for (index, driver) in drivers.iter().enumerate() {
        for endpoint in driver.endpoints() {
            if !matches(&endpoint) {
                continue;
            }
            if endpoint.is_available() {
                return Some((index, endpoint.id().clone()));
            }
            if first_match.is_none() {
                first_match = Some((index, endpoint.id().clone()));
            }
        }
    }
    first_match
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

    /// A board that declares four WS281x timing resources must be able to
    /// drive four strips at once, exactly as the S3's RMT driver does. The
    /// virtual driver used to pin `/rmt/ws281x0` at construction, so the second
    /// open failed with "already claimed" and every host run of a four-channel
    /// project silently lit one strip.
    #[test]
    fn virtual_ws281x_opens_one_output_per_declared_timing_resource() {
        let registry = Rc::new(HwRegistry::new(crate::default_esp32s3_hardware_manifest()));
        let system = HardwareSystem::with_virtual_drivers(Rc::clone(&registry));
        let specs = ["D10", "D9", "D8", "D7"]
            .map(|pin| HwEndpointSpec::parse(alloc::format!("ws281x:rmt:{pin}")).expect("spec"));

        let outputs = specs
            .iter()
            .map(|spec| {
                system
                    .open_ws281x_by_spec(spec, Ws281xConfig::new(3))
                    .unwrap_or_else(|error| panic!("{spec} should open: {error}"))
            })
            .collect::<Vec<_>>();

        for channel in 0..4 {
            assert!(
                registry.is_claimed(&HwAddress::rmt_ws281x(channel)),
                "/rmt/ws281x{channel} should back one of the four outputs"
            );
        }

        // A fifth output has no timing resource left to claim.
        let fifth = HwEndpointSpec::from_static("ws281x:rmt:D6");
        assert!(matches!(
            system.open_ws281x_by_spec(&fifth, Ws281xConfig::new(3)),
            Err(HardwareEndpointError::Hardware { .. })
        ));

        drop(outputs);

        for channel in 0..4 {
            assert!(!registry.is_claimed(&HwAddress::rmt_ws281x(channel)));
        }
    }

    /// Opening by spec must enumerate a driver once.
    ///
    /// It used to cost three passes — one to choose the endpoint, one to work
    /// out which driver owned the id, and one inside the driver to recover the
    /// GPIO — and each pass computes a live status for every endpoint the board
    /// declares. That is the difference between a lookup and a survey.
    #[test]
    fn opening_by_spec_enumerates_the_driver_once() {
        struct CountingWs281xDriver {
            inner: VirtualWs281xDriver,
            enumerations: core::cell::Cell<usize>,
        }

        impl crate::HwDriver for CountingWs281xDriver {
            fn driver_id(&self) -> &str {
                self.inner.driver_id()
            }

            fn display_label(&self) -> &str {
                self.inner.display_label()
            }
        }

        impl Ws281xDriver for CountingWs281xDriver {
            fn endpoints(&self) -> Vec<HwEndpoint> {
                self.enumerations.set(self.enumerations.get() + 1);
                self.inner.endpoints()
            }

            fn open(
                &self,
                endpoint_id: &HwEndpointId,
                config: Ws281xConfig,
            ) -> Result<Box<dyn Ws281xOutput>, HardwareEndpointError> {
                self.inner.open(endpoint_id, config)
            }
        }

        let registry = Rc::new(HwRegistry::new(HwManifest::virtual_single_rmt_gpio_board()));
        let driver = Rc::new(CountingWs281xDriver {
            inner: VirtualWs281xDriver::new(Rc::clone(&registry)),
            enumerations: core::cell::Cell::new(0),
        });

        struct SharedDriver(Rc<CountingWs281xDriver>);

        impl crate::HwDriver for SharedDriver {
            fn driver_id(&self) -> &str {
                self.0.driver_id()
            }

            fn display_label(&self) -> &str {
                self.0.display_label()
            }
        }

        impl Ws281xDriver for SharedDriver {
            fn endpoints(&self) -> Vec<HwEndpoint> {
                self.0.endpoints()
            }

            fn open(
                &self,
                endpoint_id: &HwEndpointId,
                config: Ws281xConfig,
            ) -> Result<Box<dyn Ws281xOutput>, HardwareEndpointError> {
                self.0.open(endpoint_id, config)
            }
        }

        let mut system = HardwareSystem::new(Rc::clone(&registry));
        system.add_ws281x_driver(Box::new(SharedDriver(Rc::clone(&driver))));

        let output = system
            .open_ws281x_by_spec(
                &HwEndpointSpec::from_static("ws281x:rmt:D10"),
                Ws281xConfig::new(3),
            )
            .expect("D10 opens");

        assert_eq!(
            driver.enumerations.get(),
            1,
            "one open should survey the board once"
        );
        drop(output);
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
