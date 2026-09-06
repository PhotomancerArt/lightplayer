//! ESP32-C6 Q32 math perf experiment: the `jit-math-perf` payload's device
//! half.
//!
//! This feature-gated harness measures candidate JIT hot-path math kernels on
//! the actual target using the ESP32-C6 PMU cycle counter. It deliberately
//! stays outside normal firmware boot and shader execution.
//!
//! The corpus, the kernels and the benchmark runner live in
//! `fw_checks::checks::jit_math_perf`, where they are `no_std` and have no
//! chip dependency beyond the cycle counter, which this harness injects as a
//! plain function pointer. What stays here is board init, the USB-Serial-JTAG
//! link, the clock, and the one PMU register that has no portable CSR.

extern crate alloc;

use alloc::rc::Rc;
use core::cell::RefCell;

use esp_hal::usb_serial_jtag::UsbSerialJtag;
use log::info;

use crate::board::esp32c6::constants::CPU_HZ;
use crate::board::esp32c6::cycle_counter;
use crate::board::esp32c6::init::{init_board, start_runtime};
use crate::logger;
use crate::serial::Esp32UsbSerialIo;

pub async fn run_jit_math_perf(_: embassy_executor::Spawner) -> ! {
    let (sw_int, timg0, _rmt, usb_device, _gpio18, _flash, _gpio4, _gpio20, _wifi, _rwdt) =
        init_board();
    start_runtime(timg0, sw_int);

    let usb_serial = UsbSerialJtag::new(usb_device);
    let serial_io = Esp32UsbSerialIo::new(usb_serial);
    let serial_io_shared = Rc::new(RefCell::new(serial_io));

    logger::set_log_serial(serial_io_shared);
    logger::init(logger::log_write_bytes);

    embassy_time::Timer::after(embassy_time::Duration::from_millis(100)).await;
    cycle_counter::setup();

    // The transcript header, first, before any record — printed through
    // `esp_println` rather than logged, the same way every other C6 payload
    // harness does it now (see lp-fw/fw-checks/src/header.rs).
    let _ = fw_checks::write_header(
        &mut esp_println::Printer,
        &fw_checks::PayloadHeader {
            payload: "jit-math-perf",
            chip: "esp32c6",
            firmware_commit: env!("LP_BUILD_COMMIT"),
            firmware_features: env!("LP_BUILD_FEATURES"),
            firmware_dirty: fw_checks::str_is_true(env!("LP_BUILD_DIRTY")),
        },
    );
    info!("[jit-math-perf] esp32c6 @ {CPU_HZ} Hz");
    fw_checks::checks::jit_math_perf::run_all(cycle_counter::read);

    loop {
        embassy_time::Timer::after(embassy_time::Duration::from_secs(60)).await;
    }
}
