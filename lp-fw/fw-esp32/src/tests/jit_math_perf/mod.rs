//! ESP32-C6 Q32 math perf experiment.
//!
//! This feature-gated harness measures candidate JIT hot-path math kernels on
//! the actual target using the ESP32-C6 PMU cycle counter. It deliberately
//! stays outside normal firmware boot and shader execution.

extern crate alloc;

use alloc::rc::Rc;
use core::cell::RefCell;

use esp_hal::usb_serial_jtag::UsbSerialJtag;
use log::info;

use crate::board::esp32c6::init::{init_board, start_runtime};
use crate::logger;
use crate::serial::Esp32UsbSerialIo;

mod corpus;
mod cycle_counter;
mod div_kernels;
mod lut_cost;
mod mul_kernels;
mod runner;
mod trig_kernels;

const ESP32C6_HZ: u64 = 160_000_000;

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

    info!("[jit-math-perf] === JIT math perf experiment starting ===");
    info!("[jit-math-perf] esp32c6 @ {ESP32C6_HZ} Hz");
    runner::run_overhead_baseline();
    lut_cost::run();
    mul_kernels::run();
    div_kernels::run();
    trig_kernels::run();
    info!("[jit-math-perf] === DONE ===");

    loop {
        embassy_time::Timer::after(embassy_time::Duration::from_secs(60)).await;
    }
}
