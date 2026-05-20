use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::rc::Rc;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::cell::RefCell;

use super::{
    ButtonConfig, ButtonDebouncer, ButtonDriver, ButtonEvent, ButtonInput, HardwareAddress,
    HardwareCapability, HardwareClaim, HardwareDriver, HardwareEndpoint, HardwareEndpointError,
    HardwareEndpointId, HardwareEndpointKind, HardwareEndpointSpec, HardwareLease,
    HardwareRegistry,
};

#[derive(Clone)]
pub struct VirtualButtonDriver {
    registry: Rc<HardwareRegistry>,
    driver_id: String,
    pressed_by_address: Rc<RefCell<BTreeMap<HardwareAddress, bool>>>,
}

impl VirtualButtonDriver {
    pub fn new(registry: Rc<HardwareRegistry>) -> Self {
        Self {
            registry,
            driver_id: String::from("virtual-button"),
            pressed_by_address: Rc::new(RefCell::new(BTreeMap::new())),
        }
    }

    pub fn set_pressed(&self, address: HardwareAddress, pressed: bool) {
        self.pressed_by_address
            .borrow_mut()
            .insert(address, pressed);
    }

    fn endpoint_id(&self, address: &HardwareAddress) -> HardwareEndpointId {
        HardwareEndpointId::for_driver_address(self.driver_id(), address)
    }

    fn gpio_for_endpoint(
        &self,
        endpoint_id: &HardwareEndpointId,
    ) -> Result<HardwareAddress, HardwareEndpointError> {
        for endpoint in self.endpoints() {
            if endpoint.id() == endpoint_id {
                return Ok(endpoint.address().clone());
            }
        }

        Err(HardwareEndpointError::UnknownEndpoint {
            kind: HardwareEndpointKind::Button,
            endpoint_id: endpoint_id.clone(),
        })
    }
}

impl HardwareDriver for VirtualButtonDriver {
    fn driver_id(&self) -> &str {
        &self.driver_id
    }

    fn display_label(&self) -> &str {
        "Virtual Button"
    }
}

impl ButtonDriver for VirtualButtonDriver {
    fn endpoints(&self) -> Vec<HardwareEndpoint> {
        let mut endpoints = Vec::new();
        for resource in self.registry.manifest().resources() {
            if !resource.supports(HardwareCapability::GpioInput) {
                continue;
            }
            let address = resource.address().clone();
            let spec = button_gpio_spec(resource.display_label());
            endpoints.push(HardwareEndpoint::new(
                self.endpoint_id(&address),
                spec,
                HardwareEndpointKind::Button,
                self.driver_id(),
                address,
                resource.display_label(),
                self.registry.endpoint_status_for(resource.address()),
            ));
        }
        endpoints
    }

    fn open(
        &self,
        endpoint_id: &HardwareEndpointId,
        config: ButtonConfig,
    ) -> Result<Box<dyn ButtonInput>, HardwareEndpointError> {
        let source = self.gpio_for_endpoint(endpoint_id)?;
        self.registry
            .ensure_capability(&source, HardwareCapability::GpioInput)?;
        let lease = self
            .registry
            .claim_bundle(HardwareClaim::new(self.driver_id(), vec![source.clone()]))?;
        Ok(Box::new(VirtualButtonInput::new(
            Rc::clone(&self.registry),
            source,
            lease,
            config,
            Rc::clone(&self.pressed_by_address),
        )))
    }
}

fn button_gpio_spec(config: &str) -> HardwareEndpointSpec {
    HardwareEndpointSpec::parse(alloc::format!("button:gpio:{config}"))
        .expect("manifest display label should form a valid endpoint spec")
}

struct VirtualButtonInput {
    registry: Rc<HardwareRegistry>,
    source: HardwareAddress,
    lease: Option<HardwareLease>,
    debouncer: ButtonDebouncer,
    pressed_by_address: Rc<RefCell<BTreeMap<HardwareAddress, bool>>>,
}

impl VirtualButtonInput {
    fn new(
        registry: Rc<HardwareRegistry>,
        source: HardwareAddress,
        lease: HardwareLease,
        config: ButtonConfig,
        pressed_by_address: Rc<RefCell<BTreeMap<HardwareAddress, bool>>>,
    ) -> Self {
        Self {
            registry,
            source: source.clone(),
            lease: Some(lease),
            debouncer: ButtonDebouncer::new(source, config.stable_ms()),
            pressed_by_address,
        }
    }

    fn close(&mut self) {
        if let Some(lease) = self.lease.take() {
            let _ = self.registry.release(&lease);
        }
    }
}

impl ButtonInput for VirtualButtonInput {
    fn source(&self) -> &HardwareAddress {
        &self.source
    }

    fn poll(&mut self, now_ms: u64) -> Option<ButtonEvent> {
        let pressed = self
            .pressed_by_address
            .borrow()
            .get(&self.source)
            .copied()
            .unwrap_or(false);
        self.debouncer.sample(now_ms, pressed)
    }
}

impl Drop for VirtualButtonInput {
    fn drop(&mut self) {
        self.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hardware::{ButtonEventKind, HardwareManifest, HardwareResource};

    #[test]
    fn virtual_button_driver_polls_injected_state() {
        let registry = Rc::new(HardwareRegistry::new(test_manifest()));
        let driver = VirtualButtonDriver::new(Rc::clone(&registry));
        let endpoint_id =
            HardwareEndpointId::for_driver_address(driver.driver_id(), &HardwareAddress::gpio(4));
        let mut input = driver
            .open(&endpoint_id, ButtonConfig::new(10))
            .expect("button opens");

        driver.set_pressed(HardwareAddress::gpio(4), true);
        assert!(input.poll(0).is_none());
        let event = input.poll(10).expect("stable press emits event");
        assert_eq!(event.kind(), ButtonEventKind::Pressed);
    }

    fn test_manifest() -> HardwareManifest {
        HardwareManifest::new(
            "test",
            "Test Board",
            [HardwareResource::new(
                HardwareAddress::gpio(4),
                [HardwareCapability::GpioInput],
                "GPIO4",
            )],
        )
    }
}
