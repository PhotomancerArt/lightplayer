use alloc::boxed::Box;
use alloc::collections::VecDeque;
use alloc::rc::Rc;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::cell::RefCell;

use super::{
    HardwareAddress, HardwareCapability, HardwareClaim, HardwareDriver, HardwareEndpoint,
    HardwareEndpointError, HardwareEndpointId, HardwareEndpointKind, HardwareLease,
    HardwareRegistry, RadioConfig, RadioDevice, RadioDriver, RadioPacket,
};

pub struct VirtualRadioDriver {
    registry: Rc<HardwareRegistry>,
    driver_id: String,
    address: HardwareAddress,
    state: Rc<RefCell<VirtualRadioState>>,
}

impl VirtualRadioDriver {
    pub fn new(registry: Rc<HardwareRegistry>, radio_index: u8) -> Self {
        Self {
            registry,
            driver_id: alloc::format!("virtual-radio-{radio_index}"),
            address: HardwareAddress::radio(radio_index),
            state: Rc::new(RefCell::new(VirtualRadioState::default())),
        }
    }

    pub fn push_received(&self, packet: RadioPacket) {
        self.state.borrow_mut().received.push_back(packet);
    }

    pub fn take_sent(&self) -> Vec<RadioPacket> {
        self.state.borrow_mut().sent.drain(..).collect()
    }

    fn endpoint_id(&self) -> HardwareEndpointId {
        HardwareEndpointId::for_driver_address(self.driver_id(), &self.address)
    }
}

impl HardwareDriver for VirtualRadioDriver {
    fn driver_id(&self) -> &str {
        &self.driver_id
    }

    fn display_label(&self) -> &str {
        "Virtual Radio"
    }
}

impl RadioDriver for VirtualRadioDriver {
    fn endpoints(&self) -> Vec<HardwareEndpoint> {
        let Some(resource) = self.registry.manifest().resource(&self.address) else {
            return Vec::new();
        };
        if !resource.supports(HardwareCapability::Radio) {
            return Vec::new();
        }

        vec![HardwareEndpoint::new(
            self.endpoint_id(),
            HardwareEndpointKind::Radio,
            self.driver_id(),
            self.address.clone(),
            resource.display_label(),
            self.registry.endpoint_status_for(&self.address),
        )]
    }

    fn open(
        &self,
        endpoint_id: &HardwareEndpointId,
        config: RadioConfig,
    ) -> Result<Box<dyn RadioDevice>, HardwareEndpointError> {
        let _ = config;
        let expected_id = self.endpoint_id();
        if endpoint_id != &expected_id {
            return Err(HardwareEndpointError::UnknownEndpoint {
                kind: HardwareEndpointKind::Radio,
                endpoint_id: endpoint_id.clone(),
            });
        }

        let endpoint = self.endpoints().into_iter().next().ok_or_else(|| {
            HardwareEndpointError::UnknownEndpoint {
                kind: HardwareEndpointKind::Radio,
                endpoint_id: endpoint_id.clone(),
            }
        })?;
        if !endpoint.is_available() {
            return Err(HardwareEndpointError::EndpointUnavailable {
                endpoint_id: endpoint_id.clone(),
                reason: endpoint
                    .status()
                    .unavailable_reason()
                    .unwrap_or("endpoint unavailable")
                    .into(),
            });
        }

        self.registry
            .ensure_capability(&self.address, HardwareCapability::Radio)?;
        let lease = self.registry.claim_bundle(HardwareClaim::new(
            self.driver_id(),
            vec![self.address.clone()],
        ))?;
        Ok(Box::new(VirtualRadioDevice::new(
            Rc::clone(&self.registry),
            lease,
            Rc::clone(&self.state),
        )))
    }
}

#[derive(Default)]
struct VirtualRadioState {
    received: VecDeque<RadioPacket>,
    sent: Vec<RadioPacket>,
}

struct VirtualRadioDevice {
    registry: Rc<HardwareRegistry>,
    lease: Option<HardwareLease>,
    state: Rc<RefCell<VirtualRadioState>>,
}

impl VirtualRadioDevice {
    fn new(
        registry: Rc<HardwareRegistry>,
        lease: HardwareLease,
        state: Rc<RefCell<VirtualRadioState>>,
    ) -> Self {
        Self {
            registry,
            lease: Some(lease),
            state,
        }
    }

    fn close(&mut self) {
        if let Some(lease) = self.lease.take() {
            let _ = self.registry.release(&lease);
        }
    }
}

impl RadioDevice for VirtualRadioDevice {
    fn send(&mut self, peer: [u8; 6], payload: &[u8]) -> Result<(), HardwareEndpointError> {
        self.state
            .borrow_mut()
            .sent
            .push(RadioPacket::new(peer, payload.to_vec()));
        Ok(())
    }

    fn receive(&mut self) -> Result<Option<RadioPacket>, HardwareEndpointError> {
        Ok(self.state.borrow_mut().received.pop_front())
    }
}

impl Drop for VirtualRadioDevice {
    fn drop(&mut self) {
        self.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hardware::{HardwareManifest, HardwareResource};

    #[test]
    fn virtual_radio_records_sent_packets_and_receives_injected_packets() {
        let registry = Rc::new(HardwareRegistry::new(test_manifest()));
        let driver = VirtualRadioDriver::new(Rc::clone(&registry), 0);
        let endpoint_id =
            HardwareEndpointId::for_driver_address(driver.driver_id(), &HardwareAddress::radio(0));
        let mut radio = driver
            .open(&endpoint_id, RadioConfig::default())
            .expect("radio opens");

        radio.send([0xff; 6], b"hello").expect("send");
        assert_eq!(driver.take_sent()[0].payload(), b"hello");

        driver.push_received(RadioPacket::new([1, 2, 3, 4, 5, 6], b"world".to_vec()));
        let packet = radio.receive().expect("receive").expect("packet");
        assert_eq!(packet.payload(), b"world");
    }

    fn test_manifest() -> HardwareManifest {
        HardwareManifest::new(
            "test",
            "Test Board",
            [HardwareResource::new(
                HardwareAddress::radio(0),
                [HardwareCapability::Radio],
                "Radio 0",
            )],
        )
    }
}
