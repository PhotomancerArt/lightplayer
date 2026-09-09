//! ESP32-C6 `espnow-broadcast`: the payload's device half.
//!
//! The schedule, the payload-length ladder, the record shapes and the
//! peer-counter reduction live in `fw_checks::checks::espnow_broadcast`, where
//! they are `no_std` and know nothing about this chip. What stays here is what
//! cannot leave, and it is four kinds of chip fact:
//!
//! 1. **Board init and the executor.** `init_board` hands out the `WIFI`
//!    peripheral, the timer group and the software interrupt, and
//!    `start_runtime` is what makes `embassy_time::Ticker` tick at all.
//! 2. **The product's ESP-NOW driver.** `Esp32EspNowRadioDriver` is
//!    `fw-esp32c6`'s, over `esp_radio`'s `EspNow` and `lpc-hardware`'s
//!    `HwRegistry`. This payload **calls** it. It does not re-implement it and
//!    it does not modify it — the driver is read, never written (E-product),
//!    which is the whole reason a pass here is a pass for the code the product
//!    runs.
//! 3. **The hardware registry and the board manifest.** The radio endpoint is
//!    opened off `default_esp32c6_hardware_manifest()`, exactly as
//!    `test_espnow` opens it.
//! 4. **The console.** `esp_println::Printer`, so the header, the human lines
//!    and the records share one path and cannot interleave.
//!
//! # Why the harness is a `src/tests/` entry point at all — DD25, on the record
//!
//! Every payload in this crate that can be a `fw-checks` module and one feature
//! line is exactly that, and the M4 P3 brief is explicit that DD25 is a
//! permission rather than an instruction. This one takes the permission, and
//! here is what the harness needs that `fw-checks` cannot have:
//!
//! * `esp_hal::peripherals::WIFI` and `crate::board::esp32c6::init::init_board`
//!   — the chip's radio peripheral, handed over by this crate's own board
//!   bring-up.
//! * `crate::hardware::espnow_radio_driver::Esp32EspNowRadioDriver` — the
//!   product driver under test, which pulls in `esp_radio`, `esp_hal::efuse`
//!   and `lpc_hardware`'s registry.
//! * `embassy_executor` and `embassy_time` — the runtime the 50 ms tick and the
//!   send callback's `.wait()` both need.
//!
//! A `fw-checks` module that could call any of that would have dragged the chip
//! crate, `esp-radio` and the product's hardware registry into a crate whose
//! README's first rule is "keep it cheap" — and would have made `cargo test -p
//! fw-checks` need a RISC-V toolchain. So this follows `cycle_probe.rs`'s and
//! `gpio_input.rs`'s precedent (#621, #629): the portable half is a `fw-checks`
//! module, the chip-bound half is here, and `main.rs` gains two cfg-gated
//! lines.
//!
//! # `test_espnow` is migrated, not replaced
//!
//! `src/tests/test_espnow.rs` is untouched and undeleted (RD13/DD31: deleting a
//! file under `lp-fw/` outside `fw-checks` fires E-product). Its facts are
//! here — the diagnostic channel, the broadcast address by way of the driver,
//! the tick loop, the drain, the four fields a receiver reports — and the two
//! now overlap. That duplication is a ruling for the director and a line for
//! M6's sweep; it is not this phase's to resolve.
//!
//! # One number for one frame
//!
//! `test_espnow` printed its own 1-based `tx_count` on the tx line and the
//! driver's 0-based `RadioEventId` on the rx line, so its two numbers for one
//! frame never matched. This payload prints **the driver's event id** on both
//! sides — the number that is actually on the wire — and mirrors it locally
//! rather than inventing a second counter. The mirror is exact because
//! `Esp32EspNowRadioDevice::send_channel` takes its event id *before* it can
//! fail, so the id is the number of calls made, not the number that succeeded.
//!
//! # After the sentinel: still broadcasting, and silent
//!
//! The desk captures two boards through one port, one after the other
//! (`d1-desk-batch.md` step 3), so the board that is not being recorded has to
//! still be on the air. The payload therefore keeps ticking, keeps
//! broadcasting and keeps draining for ever after `=== DONE ===` — and prints
//! nothing more. Printing on would spend serial bandwidth on a board nobody is
//! reading and would put lines after the sentinel in an emulated capture, which
//! stops at the sentinel on the host's side rather than on the firmware's.

extern crate alloc;

use alloc::boxed::Box;
use alloc::rc::Rc;
use alloc::vec::Vec;
use core::cell::RefCell;

use embassy_time::{Duration, Ticker};
use fw_checks::checks::espnow_broadcast::{
    DEADLINE_TICKS, Peers, RX_EVENTS, TICK_MS, TICKS_PER_SEND, TX_EVENTS, TxRecord, fill_payload,
    rx_record, write_done, write_ready, write_rx_line, write_rx_record, write_summary,
    write_tx_line, write_tx_record,
};
use lpc_hardware::{
    HardwareSystem, HwAddress, HwRegistry, RADIO_MAX_PAYLOAD_LEN, RadioChannelId, RadioConfig,
    RadioMessageKind, default_esp32c6_hardware_manifest,
};

use crate::board::esp32c6::init::{init_board, start_runtime};
use crate::hardware::espnow_radio_driver::Esp32EspNowRadioDriver;
use crate::logger;
// Through the module rather than the re-export, for `gpio_input.rs`'s reason:
// `serial::Esp32UsbSerialIo` is gated to a named list of harnesses and adding a
// name to that list would be an edit outside the two files DD25 allows.
use crate::serial::usb_serial::Esp32UsbSerialIo;

/// The logical channel, as `fw-checks` states it.
const DIAGNOSTIC_CHANNEL: RadioChannelId =
    RadioChannelId::new(fw_checks::checks::espnow_broadcast::DIAGNOSTIC_CHANNEL);

pub async fn run_espnow_broadcast(_: embassy_executor::Spawner) -> ! {
    let (sw_int, timg0, _rmt, usb_device, _gpio18, _flash, _gpio4, _gpio20, wifi, _rwdt) =
        init_board();
    start_runtime(timg0, sw_int);

    // The logger, so the product driver's own `log::info!` lines reach the
    // transcript too. The payload's header, lines and records do NOT go through
    // it — see the module docs.
    let usb_serial = esp_hal::usb_serial_jtag::UsbSerialJtag::new(usb_device);
    let serial_io = Esp32UsbSerialIo::new(usb_serial);
    let serial_io_shared = Rc::new(RefCell::new(serial_io));
    logger::set_log_serial(serial_io_shared);
    logger::init(logger::log_write_bytes);

    embassy_time::Timer::after(Duration::from_millis(100)).await;

    // The transcript header, first, before any record.
    let _ = fw_checks::write_header(
        &mut esp_println::Printer,
        &fw_checks::PayloadHeader {
            payload: "espnow-broadcast",
            chip: "esp32c6",
            firmware_commit: env!("LP_BUILD_COMMIT"),
            firmware_features: env!("LP_BUILD_FEATURES"),
            firmware_dirty: fw_checks::str_is_true(env!("LP_BUILD_DIRTY")),
        },
    );

    let registry = Rc::new(HwRegistry::new(default_esp32c6_hardware_manifest()));
    let mut hardware_system = HardwareSystem::new(Rc::clone(&registry));
    let radio_driver = Esp32EspNowRadioDriver::new(Rc::clone(&registry), wifi)
        .expect("ESP-NOW radio driver initializes");
    let device_id = radio_driver.device_id();
    let espnow_channel = radio_driver.default_channel();
    hardware_system.add_radio_driver(Box::new(radio_driver));

    let mut radio = hardware_system
        .open_radio_by_address(&HwAddress::radio(0), RadioConfig::new(Some(espnow_channel)))
        .expect("ESP-NOW radio opens");
    radio
        .subscribe_channel(DIAGNOSTIC_CHANNEL)
        .expect("diagnostic channel subscribes");

    let device = device_id.as_u32();
    let _ = write_ready(&mut esp_println::Printer, device, espnow_channel);

    let mut ticker = Ticker::every(Duration::from_millis(u64::from(TICK_MS)));
    let mut tick = 0u32;
    // The driver's next `RadioEventId`, mirrored: it is taken before the send
    // can fail, so it counts calls rather than successes.
    let mut next_event = 0u32;
    let mut tx_recorded = 0u32;
    let mut rx_recorded = 0u32;
    let mut peers = Peers::new();
    let mut dropped = 0u32;
    let mut done = false;
    let mut payload_buf = [0u8; RADIO_MAX_PAYLOAD_LEN];
    let mut messages: Vec<lpc_hardware::RadioMessage> = Vec::new();

    loop {
        ticker.next().await;
        tick = tick.wrapping_add(1);

        if tick % TICKS_PER_SEND == 0 {
            let event = next_event;
            next_event = next_event.wrapping_add(1);
            let len = fill_payload(event, &mut payload_buf);
            match radio.send_channel(
                DIAGNOSTIC_CHANNEL,
                RadioMessageKind::ButtonPress,
                &payload_buf[..len],
            ) {
                Ok(()) => {
                    if !done && tx_recorded < TX_EVENTS {
                        let record = TxRecord {
                            n: tx_recorded,
                            device,
                            event,
                            msg_kind: RadioMessageKind::ButtonPress.as_u8(),
                            payload_len: len,
                        };
                        let _ = write_tx_line(&mut esp_println::Printer, &record);
                        let _ = write_tx_record(&mut esp_println::Printer, &record);
                        tx_recorded += 1;
                    }
                }
                Err(error) => {
                    if !done {
                        esp_println::println!("[espnow-broadcast] tx failed: {error}");
                    }
                }
            }
        }

        messages.clear();
        match radio.drain_channel(DIAGNOSTIC_CHANNEL, &mut messages) {
            Ok(report) => {
                dropped = dropped.saturating_add(report.dropped_count());
                for message in messages.drain(..) {
                    let peer = message.source_device_id().as_u32();
                    let event = message.event_id().as_u32();
                    let msg_kind = message.kind().as_u8();
                    let payload_len = message.payload().len();
                    let gap = peers.observe(peer, event);
                    if done || rx_recorded >= RX_EVENTS {
                        continue;
                    }
                    let _ =
                        write_rx_line(&mut esp_println::Printer, peer, event, msg_kind, payload_len);
                    let record = rx_record(rx_recorded, peer, event, msg_kind, payload_len, gap);
                    let _ = write_rx_record(&mut esp_println::Printer, &record);
                    rx_recorded += 1;
                }
            }
            Err(error) => {
                if !done {
                    esp_println::println!("[espnow-broadcast] rx drain failed: {error}");
                }
            }
        }

        if !done && (tx_recorded >= TX_EVENTS && rx_recorded >= RX_EVENTS || tick >= DEADLINE_TICKS)
        {
            let _ = write_summary(
                &mut esp_println::Printer,
                tx_recorded,
                rx_recorded,
                &peers,
                dropped,
            );
            let _ = write_done(&mut esp_println::Printer);
            done = true;
        }
    }
}
