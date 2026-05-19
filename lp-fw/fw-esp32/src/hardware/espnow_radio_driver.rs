//! ESP-NOW-backed radio hardware driver.

extern crate alloc;

use alloc::boxed::Box;
use alloc::collections::{BTreeMap, BTreeSet, VecDeque};
use alloc::format;
use alloc::rc::Rc;
use alloc::vec;
use alloc::vec::Vec;

use esp_hal::efuse::{interface_mac_address, InterfaceMacAddress};
use esp_hal::peripherals::WIFI;
use esp_radio::esp_now::{
    EspNowError, EspNowManager, EspNowReceiver, EspNowSender, ReceivedData, BROADCAST_ADDRESS,
};
use esp_radio::wifi::{ControllerConfig, WifiController};
use lpc_shared::hardware::{
    HardwareAddress, HardwareCapability, HardwareClaim, HardwareDriver, HardwareEndpoint,
    HardwareEndpointError, HardwareEndpointId, HardwareEndpointKind, HardwareEndpointSpec,
    HardwareEndpointStatus, HardwareLease, HardwareRegistry, RadioChannelId, RadioConfig,
    RadioDevice, RadioDeviceId, RadioDrainReport, RadioDriver, RadioEventId, RadioMessage,
    RadioMessageKind, RADIO_MAX_PACKET_LEN,
};

const DRIVER_ID: &str = "esp32-espnow-radio0";
const DISPLAY_LABEL: &str = "ESP32 ESP-NOW Radio 0";
const ENDPOINT_SPEC: &str = "radio:espnow:0";
pub const DEFAULT_ESPNOW_CHANNEL: u8 = 11;
const RADIO_QUEUE_CAPACITY: usize = 16;
const SEEN_RING_LEN: usize = 32;

pub struct Esp32EspNowRadioDriver {
    registry: Rc<HardwareRegistry>,
    controller: &'static WifiController<'static>,
    address: HardwareAddress,
    device_id: RadioDeviceId,
    default_channel: u8,
}

impl Esp32EspNowRadioDriver {
    pub fn new(
        registry: Rc<HardwareRegistry>,
        wifi: WIFI<'static>,
    ) -> Result<Self, HardwareEndpointError> {
        Self::with_channel(registry, wifi, DEFAULT_ESPNOW_CHANNEL)
    }

    pub fn with_channel(
        registry: Rc<HardwareRegistry>,
        wifi: WIFI<'static>,
        default_channel: u8,
    ) -> Result<Self, HardwareEndpointError> {
        validate_channel(default_channel)?;
        let controller =
            esp_radio::wifi::new(wifi, ControllerConfig::default()).map_err(|error| {
                HardwareEndpointError::Other {
                    message: format!("ESP-NOW Wi-Fi init failed: {error:?}"),
                }
            })?;
        let controller = Box::leak(Box::new(controller));

        Ok(Self {
            registry,
            controller,
            address: HardwareAddress::radio(0),
            device_id: station_device_id(),
            default_channel,
        })
    }

    pub fn device_id(&self) -> RadioDeviceId {
        self.device_id
    }

    pub fn default_channel(&self) -> u8 {
        self.default_channel
    }

    fn endpoint_id(&self) -> HardwareEndpointId {
        HardwareEndpointId::for_driver_spec(self.driver_id(), &endpoint_spec())
    }

    fn endpoint_status(&self) -> HardwareEndpointStatus {
        self.registry.endpoint_status_for(&self.address)
    }
}

impl HardwareDriver for Esp32EspNowRadioDriver {
    fn driver_id(&self) -> &str {
        DRIVER_ID
    }

    fn display_label(&self) -> &str {
        DISPLAY_LABEL
    }
}

impl RadioDriver for Esp32EspNowRadioDriver {
    fn endpoints(&self) -> Vec<HardwareEndpoint> {
        let Some(resource) = self.registry.manifest().resource(&self.address) else {
            return Vec::new();
        };
        if !resource.supports(HardwareCapability::Radio) {
            return Vec::new();
        }

        vec![HardwareEndpoint::new(
            self.endpoint_id(),
            endpoint_spec(),
            HardwareEndpointKind::Radio,
            self.driver_id(),
            self.address.clone(),
            resource.display_label(),
            self.endpoint_status(),
        )]
    }

    fn open(
        &self,
        endpoint_id: &HardwareEndpointId,
        config: RadioConfig,
    ) -> Result<Box<dyn RadioDevice>, HardwareEndpointError> {
        if endpoint_id != &self.endpoint_id() {
            return Err(HardwareEndpointError::UnknownEndpoint {
                kind: HardwareEndpointKind::Radio,
                endpoint_id: endpoint_id.clone(),
            });
        }

        let channel = config.channel().unwrap_or(self.default_channel);
        validate_channel(channel)?;

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

        let esp_now = self.controller.esp_now();
        if let Err(error) = esp_now.set_channel(channel) {
            let _ = self.registry.release(&lease);
            return Err(map_esp_now_error("set channel", error));
        }
        match esp_now.version() {
            Ok(version) => {
                log::info!("[fw-esp32] ESP-NOW radio version={version} channel={channel}");
            }
            Err(error) => {
                log::warn!("[fw-esp32] ESP-NOW version query failed: {error:?}");
            }
        }
        let (manager, sender, receiver) = esp_now.split();

        Ok(Box::new(Esp32EspNowRadioDevice::new(
            Rc::clone(&self.registry),
            lease,
            manager,
            sender,
            receiver,
            self.device_id,
        )))
    }
}

struct Esp32EspNowRadioDevice {
    registry: Rc<HardwareRegistry>,
    lease: Option<HardwareLease>,
    _manager: EspNowManager<'static>,
    sender: EspNowSender<'static>,
    receiver: EspNowReceiver<'static>,
    device_id: RadioDeviceId,
    subscriptions: BTreeSet<RadioChannelId>,
    queues: BTreeMap<RadioChannelId, RadioQueue>,
    next_event_id: u32,
    seen: SeenRing,
}

impl Esp32EspNowRadioDevice {
    fn new(
        registry: Rc<HardwareRegistry>,
        lease: HardwareLease,
        manager: EspNowManager<'static>,
        sender: EspNowSender<'static>,
        receiver: EspNowReceiver<'static>,
        device_id: RadioDeviceId,
    ) -> Self {
        Self {
            registry,
            lease: Some(lease),
            _manager: manager,
            sender,
            receiver,
            device_id,
            subscriptions: BTreeSet::new(),
            queues: BTreeMap::new(),
            next_event_id: 0,
            seen: SeenRing::new(),
        }
    }

    fn next_event_id(&mut self) -> RadioEventId {
        let event_id = RadioEventId::new(self.next_event_id);
        self.next_event_id = self.next_event_id.wrapping_add(1);
        event_id
    }

    fn pull_received(&mut self) {
        while let Some(received) = self.receiver.receive() {
            self.process_received(received);
        }
    }

    fn process_received(&mut self, received: ReceivedData) {
        let message = match RadioMessage::decode(received.data()) {
            Ok(message) => message,
            Err(error) => {
                log::debug!(
                    "[fw-esp32] ESP-NOW ignored packet src={:02x?} len={} error={error}",
                    received.info.src_address,
                    received.data().len()
                );
                return;
            }
        };

        let channel = message.channel_id();
        if !self.subscriptions.contains(&channel) {
            return;
        }
        if !self.seen.remember_new(received.info.src_address, &message) {
            log::debug!(
                "[fw-esp32] ESP-NOW duplicate packet src={:02x?} device={:?} event={:?}",
                received.info.src_address,
                message.source_device_id(),
                message.event_id()
            );
            return;
        }

        let Some(queue) = self.queues.get_mut(&channel) else {
            return;
        };
        queue.push(message);
    }

    fn close(&mut self) {
        if let Some(lease) = self.lease.take() {
            if let Err(error) = self.registry.release(&lease) {
                log::warn!("Esp32EspNowRadioDevice: failed to release hardware lease: {error}");
            }
        }
    }
}

impl RadioDevice for Esp32EspNowRadioDevice {
    fn subscribe_channel(&mut self, channel: RadioChannelId) -> Result<(), HardwareEndpointError> {
        self.subscriptions.insert(channel);
        self.queues.entry(channel).or_insert_with(RadioQueue::new);
        Ok(())
    }

    fn unsubscribe_channel(
        &mut self,
        channel: RadioChannelId,
    ) -> Result<(), HardwareEndpointError> {
        self.subscriptions.remove(&channel);
        self.queues.remove(&channel);
        Ok(())
    }

    fn send_channel(
        &mut self,
        channel: RadioChannelId,
        kind: RadioMessageKind,
        payload: &[u8],
    ) -> Result<(), HardwareEndpointError> {
        let event_id = self.next_event_id();
        let message = RadioMessage::new(self.device_id, event_id, channel, kind, payload).map_err(
            |error| HardwareEndpointError::UnsupportedConfig {
                reason: format!("invalid radio message: {error}"),
            },
        )?;
        let mut packet = [0u8; RADIO_MAX_PACKET_LEN];
        let len = message.encode(&mut packet);
        self.sender
            .send(&BROADCAST_ADDRESS, &packet[..len])
            .map_err(|error| map_esp_now_error("send", error))?
            .wait()
            .map_err(|error| map_esp_now_error("send wait", error))
    }

    fn drain_channel(
        &mut self,
        channel: RadioChannelId,
        out: &mut Vec<RadioMessage>,
    ) -> Result<RadioDrainReport, HardwareEndpointError> {
        self.pull_received();
        let Some(queue) = self.queues.get_mut(&channel) else {
            return Ok(RadioDrainReport::empty());
        };
        Ok(queue.drain(out))
    }
}

impl Drop for Esp32EspNowRadioDevice {
    fn drop(&mut self) {
        self.close();
    }
}

struct RadioQueue {
    messages: VecDeque<RadioMessage>,
    dropped_count: u32,
    overflowed: bool,
}

impl RadioQueue {
    fn new() -> Self {
        Self {
            messages: VecDeque::with_capacity(RADIO_QUEUE_CAPACITY),
            dropped_count: 0,
            overflowed: false,
        }
    }

    fn push(&mut self, message: RadioMessage) {
        if self.messages.len() >= RADIO_QUEUE_CAPACITY {
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

impl Default for RadioQueue {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Copy)]
struct SeenEvent {
    source_mac: [u8; 6],
    source_device_id: RadioDeviceId,
    event_id: RadioEventId,
    valid: bool,
}

impl Default for SeenEvent {
    fn default() -> Self {
        Self {
            source_mac: [0; 6],
            source_device_id: RadioDeviceId::new(0),
            event_id: RadioEventId::new(0),
            valid: false,
        }
    }
}

struct SeenRing {
    events: [SeenEvent; SEEN_RING_LEN],
    next: usize,
}

impl SeenRing {
    fn new() -> Self {
        Self {
            events: [SeenEvent::default(); SEEN_RING_LEN],
            next: 0,
        }
    }

    fn remember_new(&mut self, source_mac: [u8; 6], message: &RadioMessage) -> bool {
        if self.events.iter().any(|seen| {
            seen.valid
                && seen.source_mac == source_mac
                && seen.source_device_id == message.source_device_id()
                && seen.event_id == message.event_id()
        }) {
            return false;
        }

        self.events[self.next] = SeenEvent {
            source_mac,
            source_device_id: message.source_device_id(),
            event_id: message.event_id(),
            valid: true,
        };
        self.next = (self.next + 1) % SEEN_RING_LEN;
        true
    }
}

fn station_device_id() -> RadioDeviceId {
    let mac = interface_mac_address(InterfaceMacAddress::Station);
    let bytes = mac.as_bytes();
    RadioDeviceId::new(u32::from_le_bytes([bytes[2], bytes[3], bytes[4], bytes[5]]))
}

fn endpoint_spec() -> HardwareEndpointSpec {
    HardwareEndpointSpec::from_static(ENDPOINT_SPEC)
}

fn validate_channel(channel: u8) -> Result<(), HardwareEndpointError> {
    if !(1..=14).contains(&channel) {
        return Err(HardwareEndpointError::UnsupportedConfig {
            reason: format!("ESP-NOW channel must be between 1 and 14, got {channel}"),
        });
    }
    Ok(())
}

fn map_esp_now_error(context: &str, error: EspNowError) -> HardwareEndpointError {
    HardwareEndpointError::Other {
        message: format!("ESP-NOW {context} failed: {error:?}"),
    }
}
