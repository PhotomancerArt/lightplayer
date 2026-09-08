//! The `rmt-chase` payload's C6 harness.
//!
//! Everything portable — the chase pattern, the FNV-1a checksum, the
//! `rmt-frame` record, the done marker — lives in
//! [`fw_checks::checks::rmt_chase`] and is unit-tested on the host. What is
//! here is the half that needs a chip: `init_board`, the RMT peripheral, the
//! logger, [`LedChannel`] on GPIO18, and the 10 ms sleep between frames.
//!
//! The old `test_rmt` chased for ever and said nothing. This one chases
//! [`CHASES`] times — 768 frames, ≈ 13.9 s, long enough to cross the
//! `ws281x_telemetry` module's ten-second reporting period — prints one record
//! per frame, prints the done marker, and then parks. A payload that finishes
//! is a payload the runner can record: `--exit-on` stops on the marker and the
//! transcript is the whole run rather than an arbitrary slice of an endless
//! one.
//!
//! The record, the header and the done marker all go through
//! `esp_println::Printer` rather than `log`, for the reason
//! `fw-checks/src/header.rs` gives: this crate can prove a logger is
//! installed here, but the payload's own output should not depend on that and
//! should share one sink with the `[WS281X]` telemetry line, which
//! `esp_println`s too. The logger is still installed, because esp-hal's own
//! `debug!`/`info!` output is part of what a boot capture is.

extern crate alloc;

use alloc::rc::Rc;
use core::cell::RefCell;
use esp_hal::rmt::Rmt;
use fw_checks::checks::rmt_chase::{
    CHASES, FRAMES, FrameRecord, LEDS, chase_frame, frame_bytes, write_done, write_frame_record,
};
use log::info;

use crate::board::esp32c6::init::{init_board, start_runtime};
use crate::logger;
use crate::output::LedChannel;
use crate::serial::Esp32UsbSerialIo;

/// Run the `rmt-chase` payload.
pub async fn run_rmt_test(_: embassy_executor::Spawner) -> ! {
    // Initialize board (clock, heap, runtime) and get hardware peripherals
    let (sw_int, timg0, rmt_peripheral, usb_device, gpio18, _flash, _gpio4, _gpio20, _wifi, _rwdt) =
        init_board();
    start_runtime(timg0, sw_int);

    // Initialize USB-serial for logging (synchronous mode)
    let usb_serial = esp_hal::usb_serial_jtag::UsbSerialJtag::new(usb_device);
    let serial_io = Esp32UsbSerialIo::new(usb_serial);
    let serial_io_shared = Rc::new(RefCell::new(serial_io));

    // Initialize logger using static function approach (like main.rs)
    logger::set_log_serial(serial_io_shared.clone());
    logger::init(logger::log_write_bytes);

    // Give USB serial a moment to initialize
    embassy_time::Timer::after(embassy_time::Duration::from_millis(100)).await;

    // The transcript header, first, before any record.
    let _ = fw_checks::write_header(
        &mut esp_println::Printer,
        &fw_checks::PayloadHeader {
            payload: "rmt-chase",
            chip: "esp32c6",
            firmware_commit: env!("LP_BUILD_COMMIT"),
            firmware_features: env!("LP_BUILD_FEATURES"),
            firmware_dirty: fw_checks::str_is_true(env!("LP_BUILD_DIRTY")),
        },
    );
    info!("[rmt-chase] {LEDS} LEDs on gpio18, {CHASES} chases ({FRAMES} frames)");

    // Configure RMT (we already have rmt_peripheral from init_board)
    let rmt = Rmt::new(rmt_peripheral, crate::output::rmt::shared_driver::RMT_CLOCK)
        .expect("Failed to initialize RMT");

    // GPIO18 is D10 on the XIAO C6 — the pin the shipped manifest's first
    // WS281x channel uses, so the harness measures the pad the product does.
    let pin = gpio18;

    let mut channel = LedChannel::new(rmt, pin, LEDS).expect("Failed to initialize LED channel");

    info!("[rmt-chase] LedChannel ready, starting the chase");

    let mut data = [0u8; frame_bytes(LEDS)];
    for n in 0..FRAMES {
        let lit = chase_frame(n, LEDS, &mut data);
        let tx = channel.start_transmission(&data);
        channel = tx.wait_complete();
        // After the frame, not before: the record is the claim that this
        // frame was handed to the driver and the driver said it finished.
        let _ = write_frame_record(
            &mut esp_println::Printer,
            &FrameRecord::of(n, LEDS, lit, &data),
        );
        embassy_time::Timer::after(embassy_time::Duration::from_millis(10)).await;
    }

    let _ = write_done(&mut esp_println::Printer);

    loop {
        embassy_time::Timer::after(embassy_time::Duration::from_secs(1)).await;
    }
}
