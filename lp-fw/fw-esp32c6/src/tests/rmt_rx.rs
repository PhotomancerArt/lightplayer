//! The `rmt-rx` payload's C6 harness: a frame out of gpio18 and back in on
//! gpio19.
//!
//! Everything portable — the WS2812 decode, the checksum, the `rmt-rx`
//! record, the done marker — lives in [`fw_checks::checks::rmt_rx`] and is
//! unit-tested on the host. What is here is the half that needs a chip:
//! `init_board`, the product's own RMT transmitter, esp-hal's RMT receiver,
//! and the loop that drains one while the other sends.
//!
//! # The transmitter is the product's, and it has to be
//!
//! This harness does not use esp-hal's `Channel<Tx>::transmit`, and the
//! reason is a real difference between two drivers rather than a preference.
//! `ch_tx_lim` on this chip is a **position** in the channel's window, not a
//! repeating count: the threshold fires when the read pointer reaches word
//! `tx_lim`, which is what the emulator models (RMT discovery §4, pinned
//! against silicon over 5,520 frames) and what `lp_ws281x` is written for —
//! it rewrites `tx_lim` between the half and the wrap on every event.
//! esp-hal's blocking transmitter sets `tx_lim` once, to half the window, and
//! never rewrites it, so under position semantics it is woken once per lap
//! and refills half of what the transmitter consumed. A first version of this
//! harness used it and put **twice** the frame's bits on the pad; that is a
//! finding about esp-hal on this chip, filed with the phase, and the way past
//! it is to send the way the product sends.
//!
//! So the frame goes out through `lp_ws281x`'s `DRIVER.send_blocking` on RMT
//! slot 0 — the same call `LedChannel::start_transmission` makes, with the
//! same block plan, ISR and timing — and the **spin closure that call already
//! takes** is where this payload drains the receiver. That is the whole trick
//! of the harness: the transmitter's own wait loop is the receiver's poll
//! loop.
//!
//! # The block plan
//!
//! The product's `LedChannel` publishes the ONE-channel plan, which gives its
//! transmitter all **four** RAM blocks — block 2 included, which is the
//! receiver's window. This harness publishes the **shipped two-channel plan**
//! instead (`plan_for_declared(2)`: one block each), which is the shape a
//! board with two declared strips runs and which leaves block 2 free. It
//! configures the channel itself rather than through `LedChannel::new`, which
//! takes the whole `Rmt` and would leave no `channel2` to configure.
//!
//! # The pads
//!
//! GPIO18 is D10 on the XIAO C6 and the pad the shipped manifest's first
//! WS281x channel uses, so the frame goes out where the product's frames go.
//! GPIO19 is free: it is neither USB (12/13), nor UART0 (16/17), nor the BOOT
//! strap (9). On an emulated configuration the two are tied by
//! `--wire 18:19`; on silicon they need a jumper, which is the desk batch's
//! optional item and the only part of this payload that needs hands.

extern crate alloc;

use alloc::rc::Rc;
use alloc::vec;
use core::cell::RefCell;
use esp_hal::gpio::{AnyPin, Level};
use esp_hal::rmt::{
    PulseCode, Rmt, RxChannelConfig, RxChannelCreator, TxChannelConfig, TxChannelCreator,
};
use fw_checks::checks::rmt_chase::{FrameRecord, chase_frame, frame_bytes, write_frame_record};
use fw_checks::checks::rmt_rx::{
    FRAMES, IDLE_THRES, LEDS, RxRecord, decode_frame, frame_codes, write_decode_error, write_done,
    write_rx_record, write_setup,
};
use esp_hal::time::Instant;
use log::info;
use lp_ws281x::ChannelTiming;

use crate::board::esp32c6::init::{init_board, start_runtime};
use crate::logger;
use crate::output::rmt::c6_rmt::{self, TX_PLAN, plan_for_declared};
use crate::output::rmt::shared_driver::{
    DRIVER, FRAME_TIMEOUT, RMT_CLOCK, install_isr, report_telemetry_if_due,
};
// Through the module rather than the re-export, as `cycle_probe` and
// `gpio_input` do: `serial::Esp32UsbSerialIo` is gated to a named list of
// harnesses and adding this one to that list would be an edit to a product
// file this phase has no business in.
use crate::serial::usb_serial::Esp32UsbSerialIo;

/// The pad the frame goes out on — the product's strip pin.
const TX_GPIO: u8 = 18;
/// The pad it comes back in on.
const RX_GPIO: u8 = 19;
/// The RMT slot the transmitter uses, as every harness does.
const TX_SLOT: u8 = 0;

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

    // The shipped two-channel plan — one block each — published before
    // anything configures a channel, exactly as `LedChannel::new` publishes
    // its own. It leaves block 2 for the receiver, which the one-channel plan
    // would have taken.
    let plan = plan_for_declared(2).expect("the shipped two-channel plan");
    if let Err(error) = TX_PLAN.init(plan) {
        log::error!("[rmt-rx] block plan already published: {error:?}");
    }

    let mut rmt = Rmt::new(rmt_peripheral, RMT_CLOCK).expect("RMT initialises");
    install_isr(&mut rmt);

    // The transmitter: the same registers `LedChannel::new` writes, because
    // the frame path below is the same driver.
    let tx_config = TxChannelConfig::default()
        .with_clk_divider(1)
        .with_idle_output(true)
        .with_idle_output_level(Level::Low)
        .with_carrier_modulation(false)
        .with_memsize(TX_PLAN.blocks(TX_SLOT));
    let _tx_channel = rmt
        .channel0
        .configure_tx(&tx_config)
        .expect("channel 0 configures")
        .with_pin(unsafe { AnyPin::steal(TX_GPIO) });
    c6_rmt::enable_tx_interrupts(TX_SLOT);
    // All-STOP until the first frame prefills the window, so a spurious start
    // transmits nothing.
    c6_rmt::clear_ram(TX_SLOT);
    if let Err(error) = DRIVER.configure_default_clock(TX_SLOT, &ChannelTiming::WS2812) {
        log::error!("[rmt-rx] timing configuration failed: {error:?}");
    }

    // The receiver: one block, the filter off, and an idle threshold longer
    // than the WS2812 latch so that the reception ends after the frame rather
    // than inside the gap it leaves.
    let rx_config = RxChannelConfig::default()
        .with_clk_divider(1)
        .with_carrier_modulation(false)
        .with_filter_threshold(0)
        .with_idle_threshold(IDLE_THRES)
        .with_memsize(1);
    let mut rx = rmt
        .channel2
        .configure_rx(&rx_config)
        .expect("channel 2 configures")
        .with_pin(unsafe { AnyPin::steal(RX_GPIO) });

    let _ = write_setup(&mut esp_println::Printer, TX_GPIO, RX_GPIO);
    info!(
        "[rmt-rx] {LEDS} LEDs, {FRAMES} frames, gpio{TX_GPIO} -> gpio{RX_GPIO}, tx blocks {}",
        TX_PLAN.blocks(TX_SLOT)
    );

    let mut data = vec![0u8; frame_bytes(LEDS)];
    // Wire order. `lp_ws281x` permutes by `ColorOrder::Grb` at encode time and
    // the product's `LedChannel` swaps RGB into GRB before handing the frame
    // over, so the two cancel and the bytes on the wire are the caller's RGB.
    // The same swap is here so that the decode below compares against `data`
    // whatever pattern a later reader puts in it — the chase's grey dot is
    // invariant under any permutation, but relying on that would make the
    // comparison quietly pattern-dependent.
    let mut wire = vec![0u8; frame_bytes(LEDS)];
    // Room for every bit's word plus whatever the receiver writes to close
    // the reception: the trailing idle run's half and an end marker.
    let mut received = vec![PulseCode::from(0u32); frame_codes(LEDS) + 8];
    let mut received_words = vec![0u32; frame_codes(LEDS) + 8];
    let mut decoded = vec![0u8; frame_bytes(LEDS)];

    for n in 0..FRAMES {
        let lit = chase_frame(n, LEDS, &mut data);
        for led in 0..LEDS {
            let at = led * 3;
            wire[at] = data[at + 1];
            wire[at + 1] = data[at];
            wire[at + 2] = data[at + 2];
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

        // The transmitter's wait loop is the receiver's poll loop: a frame is
        // 1,536 words through the receiver's 48-word window, so a half of it
        // has to be read every 24 words or the writer laps the reader.
        let mut rx_done = false;
        // The same hang detector `LedChannel::start_transmission` installs: a
        // frame that outlives its deadline is aborted and reported rather than
        // wedging the loop.
        let started = Instant::now();
        let mut timed_out = false;
        let send = DRIVER.send_blocking(TX_SLOT, &wire, || {
            if !rx_done {
                rx_done = rx_txn.poll();
            }
            if !timed_out && started.elapsed() > FRAME_TIMEOUT {
                timed_out = true;
                DRIVER.abort(TX_SLOT);
            }
        });
        if let Err(error) = send {
            info!("[rmt-rx] frame {n}: frame failed to start: {error:?}");
        } else if timed_out {
            info!(
                "[rmt-rx] frame {n}: did not complete within {} ms",
                FRAME_TIMEOUT.as_millis()
            );
        }
        // A no-op unless `ws281x_telemetry` is on, and the product's own call
        // site: a harness that turns it on gets the same `[WS281X]` counters
        // the app path prints.
        report_telemetry_if_due();
        // The reception outlasts the transmission by the idle threshold, so
        // the loop above never sees its end.
        while !rx_done {
            rx_done = rx_txn.poll();
        }
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
