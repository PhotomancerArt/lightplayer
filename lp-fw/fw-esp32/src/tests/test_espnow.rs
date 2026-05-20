//! ESP-NOW broadcast smoke test through the firmware radio abstraction.
//!
//! Simulates a 1 Hz button press on every device. The same firmware can be
//! flashed to multiple boards; each board broadcasts channel messages through
//! the production ESP-NOW radio driver and drains received messages through the
//! same capability API nodes will use later.

extern crate alloc;

use alloc::boxed::Box;
use alloc::rc::Rc;
use alloc::vec::Vec;

use embassy_time::{Duration, Ticker};
use esp_println::println;
use lpc_shared::hardware::{
    HardwareAddress, HardwareRegistry, HardwareSystem, RadioChannelId, RadioConfig,
    RadioMessageKind, default_esp32c6_hardware_manifest,
};

use crate::board::esp32c6::init::{init_board, start_runtime};
use crate::hardware::espnow_radio_driver::Esp32EspNowRadioDriver;

const DIAGNOSTIC_CHANNEL: RadioChannelId = RadioChannelId::new(1);
const TICKS_PER_SEND: u32 = 10;

pub async fn run_espnow_test(_: embassy_executor::Spawner) -> ! {
    println!("[test_espnow] initializing board");
    let (sw_int, timg0, _rmt, _usb_device, _gpio18, _flash, _gpio4, _gpio20, wifi) = init_board();
    start_runtime(timg0, sw_int);

    let registry = Rc::new(HardwareRegistry::new(default_esp32c6_hardware_manifest()));
    let mut hardware_system = HardwareSystem::new(Rc::clone(&registry));
    let radio_driver = Esp32EspNowRadioDriver::new(Rc::clone(&registry), wifi)
        .expect("ESP-NOW radio driver initializes");
    let device_id = radio_driver.device_id();
    let espnow_channel = radio_driver.default_channel();
    hardware_system.add_radio_driver(Box::new(radio_driver));

    let mut radio = hardware_system
        .open_radio_by_address(
            &HardwareAddress::radio(0),
            RadioConfig::new(Some(espnow_channel)),
        )
        .expect("ESP-NOW radio opens");
    radio
        .subscribe_channel(DIAGNOSTIC_CHANNEL)
        .expect("diagnostic channel subscribes");

    println!(
        "[test_espnow] radio ready device_id={:?} espnow_channel={} logical_channel={}",
        device_id,
        espnow_channel,
        DIAGNOSTIC_CHANNEL.as_u32()
    );

    let mut ticker = Ticker::every(Duration::from_millis(100));
    let mut tick_count = 0u32;
    let mut tx_count = 0u32;

    loop {
        ticker.next().await;
        tick_count = tick_count.wrapping_add(1);

        if tick_count % TICKS_PER_SEND == 0 {
            tx_count = tx_count.wrapping_add(1);
            match radio.send_channel(DIAGNOSTIC_CHANNEL, RadioMessageKind::ButtonPress, &[]) {
                Ok(()) => println!(
                    "[test_espnow] tx simulated_button device={:?} event={}",
                    device_id, tx_count
                ),
                Err(error) => println!("[test_espnow] tx failed: {error}"),
            }
        }

        let mut messages = Vec::new();
        match radio.drain_channel(DIAGNOSTIC_CHANNEL, &mut messages) {
            Ok(report) => {
                if report.dropped_count() > 0 || report.overflowed() {
                    println!(
                        "[test_espnow] rx queue overflow drained={} dropped={} overflowed={}",
                        report.drained_count(),
                        report.dropped_count(),
                        report.overflowed()
                    );
                }
                for message in messages {
                    println!(
                        "[test_espnow] rx device={:?} event={:?} kind={:?} payload_len={}",
                        message.source_device_id(),
                        message.event_id(),
                        message.kind(),
                        message.payload().len()
                    );
                }
            }
            Err(error) => println!("[test_espnow] rx drain failed: {error}"),
        }
    }
}
