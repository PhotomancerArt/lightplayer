//! ESP-NOW broadcast smoke test.
//!
//! Simulates a 1 Hz button press on every device. The same firmware can be
//! flashed to multiple boards; each board broadcasts events and de-dupes
//! received events by `(source_mac, device_id, event_id)`.

use embassy_futures::select::{Either, select};
use embassy_time::{Duration, Ticker};
use esp_hal::efuse::{InterfaceMacAddress, interface_mac_address};
use esp_println::println;
use esp_radio::esp_now::BROADCAST_ADDRESS;

use crate::board::esp32c6::init::{init_board, start_runtime};

const CHANNEL: u8 = 11;
const EVENT_MAGIC: u16 = 0x4c50;
const EVENT_VERSION: u8 = 1;
const EVENT_KIND_BUTTON_PRESS: u8 = 1;
const EVENT_PACKET_LEN: usize = 12;
const SEEN_RING_LEN: usize = 32;

#[derive(Clone, Copy)]
struct ButtonEvent {
    device_id: u32,
    event_id: u32,
}

impl ButtonEvent {
    fn encode(self) -> [u8; EVENT_PACKET_LEN] {
        let mut packet = [0u8; EVENT_PACKET_LEN];
        packet[0..2].copy_from_slice(&EVENT_MAGIC.to_le_bytes());
        packet[2] = EVENT_VERSION;
        packet[3] = EVENT_KIND_BUTTON_PRESS;
        packet[4..8].copy_from_slice(&self.device_id.to_le_bytes());
        packet[8..12].copy_from_slice(&self.event_id.to_le_bytes());
        packet
    }

    fn decode(packet: &[u8]) -> Option<Self> {
        if packet.len() != EVENT_PACKET_LEN {
            return None;
        }

        let magic = u16::from_le_bytes([packet[0], packet[1]]);
        if magic != EVENT_MAGIC
            || packet[2] != EVENT_VERSION
            || packet[3] != EVENT_KIND_BUTTON_PRESS
        {
            return None;
        }

        Some(Self {
            device_id: u32::from_le_bytes([packet[4], packet[5], packet[6], packet[7]]),
            event_id: u32::from_le_bytes([packet[8], packet[9], packet[10], packet[11]]),
        })
    }
}

#[derive(Clone, Copy, Default)]
struct SeenEvent {
    source_mac: [u8; 6],
    device_id: u32,
    event_id: u32,
    valid: bool,
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

    fn remember_new(&mut self, source_mac: [u8; 6], event: ButtonEvent) -> bool {
        if self.events.iter().any(|seen| {
            seen.valid
                && seen.source_mac == source_mac
                && seen.device_id == event.device_id
                && seen.event_id == event.event_id
        }) {
            return false;
        }

        self.events[self.next] = SeenEvent {
            source_mac,
            device_id: event.device_id,
            event_id: event.event_id,
            valid: true,
        };
        self.next = (self.next + 1) % SEEN_RING_LEN;
        true
    }
}

pub async fn run_espnow_test(_: embassy_executor::Spawner) -> ! {
    println!("[test_espnow] initializing board");
    let (sw_int, timg0, _rmt, _usb_device, _gpio18, _flash, _gpio4, wifi) = init_board();
    start_runtime(timg0, sw_int);

    let mac = interface_mac_address(InterfaceMacAddress::Station);
    let mac_bytes = mac.as_bytes();
    let device_id = u32::from_le_bytes([mac_bytes[2], mac_bytes[3], mac_bytes[4], mac_bytes[5]]);
    println!(
        "[test_espnow] station_mac={} device_id=0x{:08x}",
        mac, device_id
    );

    let controller = esp_radio::wifi::new(wifi, Default::default()).unwrap();
    let mut esp_now = controller.esp_now();
    esp_now.set_channel(CHANNEL).unwrap();
    println!(
        "[test_espnow] esp-now version={} channel={}",
        esp_now.version().unwrap(),
        CHANNEL
    );

    let mut ticker = Ticker::every(Duration::from_secs(1));
    let mut event_id = 0u32;
    let mut seen = SeenRing::new();

    loop {
        match select(ticker.next(), esp_now.receive_async()).await {
            Either::First(_) => {
                event_id = event_id.wrapping_add(1);
                let event = ButtonEvent {
                    device_id,
                    event_id,
                };
                let packet = event.encode();
                match esp_now.send_async(&BROADCAST_ADDRESS, &packet).await {
                    Ok(()) => println!(
                        "[test_espnow] tx simulated_button device=0x{:08x} event={}",
                        device_id, event_id
                    ),
                    Err(err) => println!("[test_espnow] tx failed: {:?}", err),
                }
            }
            Either::Second(received) => {
                if let Some(event) = ButtonEvent::decode(received.data()) {
                    if seen.remember_new(received.info.src_address, event) {
                        println!(
                            "[test_espnow] rx src={:02x?} device=0x{:08x} event={}",
                            received.info.src_address, event.device_id, event.event_id
                        );
                    } else {
                        println!(
                            "[test_espnow] rx duplicate src={:02x?} device=0x{:08x} event={}",
                            received.info.src_address, event.device_id, event.event_id
                        );
                    }
                } else {
                    println!(
                        "[test_espnow] rx ignored src={:02x?} len={}",
                        received.info.src_address,
                        received.data().len()
                    );
                }
            }
        }
    }
}
