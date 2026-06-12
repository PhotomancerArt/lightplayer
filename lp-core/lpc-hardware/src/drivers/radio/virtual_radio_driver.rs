use alloc::boxed::Box;
use alloc::collections::{BTreeMap, BTreeSet, VecDeque};
use alloc::rc::Rc;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::cell::RefCell;

use crate::{
    HardwareAddress, HardwareCapability, HardwareClaim, HardwareDriver, HardwareEndpoint,
    HardwareEndpointError, HardwareEndpointId, HardwareEndpointKind, HardwareEndpointSpec,
    HardwareLease, HardwareRegistry, RadioChannelId, RadioConfig, RadioDevice, RadioDeviceId,
    RadioDrainReport, RadioDriver, RadioEventId, RadioMessage, RadioMessageKind,
};

const VIRTUAL_RADIO_DEVICE_ID: RadioDeviceId = RadioDeviceId::new(0);
const VIRTUAL_RADIO_QUEUE_CAPACITY: usize = 16;

#[derive(Clone)]
pub struct VirtualRadioDriver {
    registry: Rc<HardwareRegistry>,
    driver_id: String,
    address: HardwareAddress,
    endpoint_spec: HardwareEndpointSpec,
    state: Rc<RefCell<VirtualRadioState>>,
}

impl VirtualRadioDriver {
    pub fn new(registry: Rc<HardwareRegistry>, radio_index: u8) -> Self {
        Self::new_with_spec(registry, radio_index, "radio:virtual:0")
    }

    pub fn new_with_spec(
        registry: Rc<HardwareRegistry>,
        radio_index: u8,
        spec: &'static str,
    ) -> Self {
        Self {
            registry,
            driver_id: alloc::format!("virtual-radio-{radio_index}-{spec}"),
            address: HardwareAddress::radio(radio_index),
            endpoint_spec: HardwareEndpointSpec::from_static(spec),
            state: Rc::new(RefCell::new(VirtualRadioState::default())),
        }
    }

    pub fn push_received(&self, message: RadioMessage) {
        self.state.borrow_mut().push_received(message);
    }

    pub fn take_sent(&self) -> Vec<RadioMessage> {
        self.state.borrow_mut().sent.drain(..).collect()
    }

    fn endpoint_id(&self) -> HardwareEndpointId {
        HardwareEndpointId::for_driver_spec(self.driver_id(), &self.endpoint_spec)
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
            self.endpoint_spec.clone(),
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
    subscriptions: BTreeSet<RadioChannelId>,
    received: BTreeMap<RadioChannelId, VirtualRadioQueue>,
    sent: Vec<RadioMessage>,
    next_event_id: u32,
}

impl VirtualRadioState {
    fn subscribe_channel(&mut self, channel: RadioChannelId) {
        self.subscriptions.insert(channel);
        self.received.entry(channel).or_default();
    }

    fn unsubscribe_channel(&mut self, channel: RadioChannelId) {
        self.subscriptions.remove(&channel);
        self.received.remove(&channel);
    }

    fn push_received(&mut self, message: RadioMessage) {
        if !self.subscriptions.contains(&message.channel_id()) {
            return;
        }
        self.received
            .entry(message.channel_id())
            .or_default()
            .push(message);
    }

    fn drain_channel(
        &mut self,
        channel: RadioChannelId,
        out: &mut Vec<RadioMessage>,
    ) -> RadioDrainReport {
        let Some(queue) = self.received.get_mut(&channel) else {
            return RadioDrainReport::empty();
        };
        queue.drain(out)
    }

    fn next_event_id(&mut self) -> RadioEventId {
        let event_id = RadioEventId::new(self.next_event_id);
        self.next_event_id = self.next_event_id.wrapping_add(1);
        event_id
    }
}

struct VirtualRadioQueue {
    messages: VecDeque<RadioMessage>,
    dropped_count: u32,
    overflowed: bool,
}

impl VirtualRadioQueue {
    fn push(&mut self, message: RadioMessage) {
        if self.messages.len() >= VIRTUAL_RADIO_QUEUE_CAPACITY {
            let _ = self.messages.pop_front();
            self.dropped_count = self.dropped_count.saturating_add(1);
            self.overflowed = true;
        }
        self.messages.push_back(message);
    }

    fn drain(&mut self, out: &mut Vec<RadioMessage>) -> RadioDrainReport {
        let drained_count = self.messages.len();
        out.extend(self.messages.drain(..));
        let report = RadioDrainReport::new(drained_count, self.dropped_count, self.overflowed);
        self.dropped_count = 0;
        self.overflowed = false;
        report
    }
}

impl Default for VirtualRadioQueue {
    fn default() -> Self {
        Self {
            messages: VecDeque::new(),
            dropped_count: 0,
            overflowed: false,
        }
    }
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
    fn subscribe_channel(&mut self, channel: RadioChannelId) -> Result<(), HardwareEndpointError> {
        self.state.borrow_mut().subscribe_channel(channel);
        Ok(())
    }

    fn unsubscribe_channel(
        &mut self,
        channel: RadioChannelId,
    ) -> Result<(), HardwareEndpointError> {
        self.state.borrow_mut().unsubscribe_channel(channel);
        Ok(())
    }

    fn send_channel(
        &mut self,
        channel: RadioChannelId,
        kind: RadioMessageKind,
        payload: &[u8],
    ) -> Result<(), HardwareEndpointError> {
        let mut state = self.state.borrow_mut();
        let event_id = state.next_event_id();
        let message = RadioMessage::new(VIRTUAL_RADIO_DEVICE_ID, event_id, channel, kind, payload)
            .map_err(|error| HardwareEndpointError::UnsupportedConfig {
                reason: alloc::format!("invalid radio message: {error}"),
            })?;
        state.sent.push(message);
        Ok(())
    }

    fn drain_channel(
        &mut self,
        channel: RadioChannelId,
        out: &mut Vec<RadioMessage>,
    ) -> Result<RadioDrainReport, HardwareEndpointError> {
        Ok(self.state.borrow_mut().drain_channel(channel, out))
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
    use crate::{HardwareManifest, HardwareResource};

    #[test]
    fn virtual_radio_records_sent_messages() {
        let registry = Rc::new(HardwareRegistry::new(test_manifest()));
        let driver = VirtualRadioDriver::new(Rc::clone(&registry), 0);
        let mut radio = open_test_radio(&driver);
        let channel = RadioChannelId::new(7);

        radio
            .send_channel(channel, RadioMessageKind::Custom(9), b"hello")
            .expect("send");

        let sent = driver.take_sent();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].source_device_id(), VIRTUAL_RADIO_DEVICE_ID);
        assert_eq!(sent[0].event_id(), RadioEventId::new(0));
        assert_eq!(sent[0].channel_id(), channel);
        assert_eq!(sent[0].kind(), RadioMessageKind::Custom(9));
        assert_eq!(sent[0].payload(), b"hello");
    }

    #[test]
    fn virtual_radio_subscribes_and_drains_injected_messages() {
        let registry = Rc::new(HardwareRegistry::new(test_manifest()));
        let driver = VirtualRadioDriver::new(Rc::clone(&registry), 0);
        let mut radio = open_test_radio(&driver);
        let channel = RadioChannelId::new(3);
        radio.subscribe_channel(channel).expect("subscribe");

        driver.push_received(test_message(channel, 1));
        let mut messages = Vec::new();
        let report = radio
            .drain_channel(channel, &mut messages)
            .expect("drain subscribed channel");

        assert_eq!(report.drained_count(), 1);
        assert_eq!(report.dropped_count(), 0);
        assert!(!report.overflowed());
        assert_eq!(messages[0].channel_id(), channel);
        assert_eq!(messages[0].event_id(), RadioEventId::new(1));
    }

    #[test]
    fn virtual_radio_ignores_unsubscribed_channels() {
        let registry = Rc::new(HardwareRegistry::new(test_manifest()));
        let driver = VirtualRadioDriver::new(Rc::clone(&registry), 0);
        let mut radio = open_test_radio(&driver);
        let channel = RadioChannelId::new(3);

        driver.push_received(test_message(channel, 1));
        let mut messages = Vec::new();
        let report = radio
            .drain_channel(channel, &mut messages)
            .expect("drain unsubscribed channel");

        assert_eq!(report.drained_count(), 0);
        assert!(messages.is_empty());
    }

    #[test]
    fn virtual_radio_drops_unsubscribed_channel_queue() {
        let registry = Rc::new(HardwareRegistry::new(test_manifest()));
        let driver = VirtualRadioDriver::new(Rc::clone(&registry), 0);
        let mut radio = open_test_radio(&driver);
        let channel = RadioChannelId::new(3);

        radio.subscribe_channel(channel).expect("subscribe");
        driver.push_received(test_message(channel, 1));
        radio.unsubscribe_channel(channel).expect("unsubscribe");

        let mut messages = Vec::new();
        let report = radio
            .drain_channel(channel, &mut messages)
            .expect("drain unsubscribed channel");

        assert_eq!(report.drained_count(), 0);
        assert!(messages.is_empty());
    }

    #[test]
    fn virtual_radio_reports_overflow_when_queue_exceeds_capacity() {
        let registry = Rc::new(HardwareRegistry::new(test_manifest()));
        let driver = VirtualRadioDriver::new(Rc::clone(&registry), 0);
        let mut radio = open_test_radio(&driver);
        let channel = RadioChannelId::new(5);
        radio.subscribe_channel(channel).expect("subscribe");

        for event_id in 0..(VIRTUAL_RADIO_QUEUE_CAPACITY + 2) {
            driver.push_received(test_message(channel, event_id as u32));
        }

        let mut messages = Vec::new();
        let report = radio
            .drain_channel(channel, &mut messages)
            .expect("drain overflowed channel");

        assert_eq!(report.drained_count(), VIRTUAL_RADIO_QUEUE_CAPACITY);
        assert_eq!(report.dropped_count(), 2);
        assert!(report.overflowed());
        assert_eq!(messages[0].event_id(), RadioEventId::new(2));
    }

    #[test]
    fn second_radio_open_contends_with_first() {
        let registry = Rc::new(HardwareRegistry::new(test_manifest()));
        let driver = VirtualRadioDriver::new(Rc::clone(&registry), 0);
        let first_radio = open_test_radio(&driver);
        let endpoint_id = driver.endpoint_id();

        let result = driver.open(&endpoint_id, RadioConfig::default());

        assert!(matches!(
            result,
            Err(HardwareEndpointError::EndpointUnavailable { .. })
        ));
        drop(first_radio);
    }

    #[test]
    fn dropping_radio_releases_endpoint() {
        let registry = Rc::new(HardwareRegistry::new(test_manifest()));
        let driver = VirtualRadioDriver::new(Rc::clone(&registry), 0);
        let radio = open_test_radio(&driver);
        drop(radio);

        let _ = open_test_radio(&driver);
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

    fn open_test_radio(driver: &VirtualRadioDriver) -> Box<dyn RadioDevice> {
        let endpoint_id = driver.endpoint_id();
        driver
            .open(&endpoint_id, RadioConfig::default())
            .expect("radio opens")
    }

    fn test_message(channel: RadioChannelId, event_id: u32) -> RadioMessage {
        RadioMessage::button_press(
            RadioDeviceId::new(0x55aa),
            RadioEventId::new(event_id),
            channel,
        )
    }
}
