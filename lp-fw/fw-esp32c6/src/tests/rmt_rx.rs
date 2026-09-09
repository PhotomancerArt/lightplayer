//! The `rmt-rx` payload's C6 harness: a frame out of gpio18 and back in on
//! gpio19.
//!
//! Everything portable — the WS2812 encode, the decode, the checksum, the
//! `rmt-rx` record, the done marker — lives in
//! [`fw_checks::checks::rmt_rx`] and is unit-tested on the host. What is here
//! is the half that needs a chip: `init_board`, esp-hal's `Rmt`, one TX
//! channel with a pin and one RX channel with a pin, and the poll loop that
//! services both at once.
//!
//! # Both halves are polled in one loop, and they have to be
//!
//! Each channel gets one 48-word RAM block, so a 1,536-word frame goes
//! through each window 32 times. esp-hal's blocking `wait()` spins on one
//! transaction only, and a `wait()` on the transmitter would leave the
//! receiver's half-window unread for the length of the frame — 64 thresholds
//! missed and a receiver lapping its reader. So the two transactions are
//! **polled together**: `TxTransaction::poll` refills the transmitter's half
//! and `RxTransaction::poll` drains the receiver's, and the loop runs until
//! both say they are done.
//!
//! # Why this harness does not use `LedChannel`
//!
//! The product's harness channel publishes the one-channel block plan, which
//! gives its transmitter **all four** RAM blocks — including block 2, the
//! receiver's window. A loopback needs the shipped two-channel shape, so this
//! payload configures esp-hal's channels itself with one block each. What it
//! keeps from `rmt-chase` is everything the comparison rests on: the same
//! pattern, the same FNV-1a, and the same `rmt-frame` line.
//!
//! # The pads
//!
//! GPIO18 is D10 on the XIAO C6 and the pad the shipped manifest's first
//! WS281x channel uses, so the frame goes out where the product's frames go.
//! GPIO19 is D-none on the silkscreen's LED side and is free: it is neither
//! USB (12/13), nor UART0 (16/17), nor the BOOT strap (9). On an emulated
//! configuration the two are tied by `--wire 18:19`; on silicon they need a
//! jumper, which is the desk batch's optional item and the only part of this
//! payload that needs hands.

extern crate alloc;

use alloc::rc::Rc;
use alloc::vec;
use core::cell::RefCell;
use esp_hal::gpio::AnyPin;
use esp_hal::rmt::{
    PulseCode, Rmt, RxChannelConfig, RxChannelCreator, TxChannelConfig, TxChannelCreator,
};
use esp_hal::time::Rate;
use fw_checks::checks::rmt_chase::{FrameRecord, chase_frame, frame_bytes, write_frame_record};
use fw_checks::checks::rmt_rx::{
    FRAMES, IDLE_THRES, LEDS, RxRecord, decode_frame, encode_frame, frame_codes,
    write_decode_error, write_done, write_rx_record, write_setup,
};
use log::info;

use crate::board::esp32c6::init::{init_board, start_runtime};
use crate::logger;
// Through the module rather than the re-export, as `cycle_probe` and
// `gpio_input` do: `serial::Esp32UsbSerialIo` is gated to a named list of
// harnesses and adding this one to that list would be an edit to a product
// file this phase has no business in.
use crate::serial::usb_serial::Esp32UsbSerialIo;

/// The channel clock, transcribed from
/// `output::rmt::shared_driver::RMT_CLOCK` rather than imported.
///
/// Importing it would pull the product's whole output tree — `LedChannel`
/// included — into a build that transmits its own codes and never uses it.
/// The number is pinned on the portable side too:
/// `fw_checks::checks::rmt_rx::CLOCK_HZ` is the same 80 MHz, and every
/// duration in this payload is a tick of it.
const RMT_CLOCK: Rate = Rate::from_mhz(80);

/// The pad the frame goes out on — the product's strip pin.
const TX_GPIO: u8 = 18;
/// The pad it comes back in on.
const RX_GPIO: u8 = 19;

/// Run the `rmt-rx` payload.
pub async fn run_rmt_rx(_: embassy_executor::Spawner) -> ! {
    let (sw_int, timg0, rmt_peripheral, usb_device, _gpio18, _flash, _gpio4, _gpio20, _wifi, _rwdt) =
        init_board();
    start_runtime(timg0, sw_int);

    let usb_serial = esp_hal::usb_serial_jtag::UsbSerialJtag::new(usb_device);
    let serial_io = Esp32UsbSerialIo::new(usb_serial);
    let serial_io_shared = Rc::new(RefCell::new(serial_io));
    logger::set_log_serial(serial_io_shared);
    logger::init(logger::log_write_bytes);

    embassy_time::Timer::after(embassy_time::Duration::from_millis(100)).await;

    // The transcript header, first, before any record.
    let _ = fw_checks::write_header(
        &mut esp_println::Printer,
        &fw_checks::PayloadHeader {
            payload: "rmt-rx",
            chip: "esp32c6",
            firmware_commit: env!("LP_BUILD_COMMIT"),
            firmware_features: env!("LP_BUILD_FEATURES"),
            firmware_dirty: fw_checks::str_is_true(env!("LP_BUILD_DIRTY")),
        },
    );

    let rmt = Rmt::new(rmt_peripheral, RMT_CLOCK).expect("RMT initialises");

    // One block each: the transmitter takes block 0 and the receiver block 2,
    // which is what leaves the loopback any RAM to run in at all.
    let tx_config = TxChannelConfig::default()
        .with_clk_divider(1)
        .with_idle_output(true)
        .with_idle_output_level(esp_hal::gpio::Level::Low)
        .with_carrier_modulation(false)
        .with_memsize(1);
    let rx_config = RxChannelConfig::default()
        .with_clk_divider(1)
        .with_carrier_modulation(false)
        // Off, and deliberately: the emulated wire has no glitches on it and
        // the filter is exercised by the emulator's own unit tests. A filter
        // here would be an unmeasured variable between the two sides.
        .with_filter_threshold(0)
        .with_idle_threshold(IDLE_THRES)
        .with_memsize(1);

    let mut tx = rmt
        .channel0
        .configure_tx(&tx_config)
        .expect("channel 0 configures")
        .with_pin(unsafe { AnyPin::steal(TX_GPIO) });
    let mut rx = rmt
        .channel2
        .configure_rx(&rx_config)
        .expect("channel 2 configures")
        .with_pin(unsafe { AnyPin::steal(RX_GPIO) });

    let _ = write_setup(&mut esp_println::Printer, TX_GPIO, RX_GPIO);
    info!("[rmt-rx] {LEDS} LEDs, {FRAMES} frames, gpio{TX_GPIO} -> gpio{RX_GPIO}");

    let mut data = vec![0u8; frame_bytes(LEDS)];
    let mut codes = vec![0u32; frame_codes(LEDS)];
    // The same words as `codes`, in the driver's newtype. `fw-checks` speaks
    // plain `u32` because it is a host-tested crate that must not depend on
    // esp-hal, and `PulseCode` is that `u32` with a name on it.
    let mut tx_codes = vec![PulseCode::from(0u32); frame_codes(LEDS)];
    // Room for every bit's word plus whatever the receiver writes to close
    // the reception: the trailing idle run's half and an end marker.
    let mut received = vec![PulseCode::from(0u32); frame_codes(LEDS) + 4];
    let mut received_words = vec![0u32; frame_codes(LEDS) + 4];
    let mut decoded = vec![0u8; frame_bytes(LEDS)];

    for n in 0..FRAMES {
        let lit = chase_frame(n, LEDS, &mut data);
        let written = encode_frame(&data, &mut codes).expect("codes fit");
        for (dst, src) in tx_codes.iter_mut().zip(codes.iter()) {
            *dst = PulseCode::from(*src);
        }

        // Armed *before* the frame goes out, so no edge of it can arrive with
        // the receiver still idle.
        let mut rx_txn = match rx.receive(&mut received) {
            Ok(txn) => txn,
            Err((error, channel)) => {
                rx = channel;
                info!("[rmt-rx] frame {n}: receive refused: {error:?}");
                continue;
            }
        };
        let mut tx_txn = match tx.transmit(&tx_codes[..written]) {
            Ok(txn) => txn,
            Err((error, channel)) => {
                tx = channel;
                info!("[rmt-rx] frame {n}: transmit refused: {error:?}");
                let (_, channel) = rx_txn.wait().unwrap_or_else(|(_, c)| (0, c));
                rx = channel;
                continue;
            }
        };

        let (mut tx_done, mut rx_done) = (false, false);
        while !(tx_done && rx_done) {
            if !tx_done {
                tx_done = tx_txn.poll();
            }
            if !rx_done {
                rx_done = rx_txn.poll();
            }
        }
        tx = match tx_txn.wait() {
            Ok(channel) => channel,
            Err((error, channel)) => {
                info!("[rmt-rx] frame {n}: transmit ended with {error:?}");
                channel
            }
        };
        let (words, channel) = match rx_txn.wait() {
            Ok(pair) => pair,
            Err((error, channel)) => {
                info!("[rmt-rx] frame {n}: receive ended with {error:?}");
                (0, channel)
            }
        };
        rx = channel;

        // What the guest built…
        let _ = write_frame_record(
            &mut esp_println::Printer,
            &FrameRecord::of(n, LEDS, lit, &data),
        );
        // …and what came back off the wire.
        let words = words.min(received.len());
        for (dst, src) in received_words.iter_mut().zip(received.iter()) {
            *dst = src.0;
        }
        match decode_frame(&received_words[..words], &mut decoded) {
            Ok(_) => {
                let _ =
                    write_rx_record(&mut esp_println::Printer, &RxRecord::of(n, words, &decoded));
            }
            Err(error) => {
                let _ = write_decode_error(&mut esp_println::Printer, n, words, error);
            }
        }

        embassy_time::Timer::after(embassy_time::Duration::from_millis(10)).await;
    }

    let _ = write_done(&mut esp_println::Printer);

    loop {
        embassy_time::Timer::after(embassy_time::Duration::from_secs(1)).await;
    }
}
