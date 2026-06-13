//! ESP32-C6 incremental shader compile harness.
//!
//! This feature-gated firmware mode exercises the stepped `lps-glsl` + `lpvm-native`
//! compile pipeline on real hardware. It compiles a small shader corpus using a fixed
//! per-tick step budget and logs per-tick slice time plus heap usage.

extern crate alloc;

use alloc::rc::Rc;
use alloc::sync::Arc;
use core::cell::RefCell;

use esp_hal::usb::usb_serial_jtag::UsbSerialJtag;
use log::info;
use lp_shader::LpsEngine;
use lpvm_native::{BuiltinTable, NativeCompileOptions, NativeJitEngine};

use crate::board::esp32c6::init::{init_board, start_runtime};
use crate::logger;
use crate::serial::Esp32UsbSerialIo;

mod cycle_counter;
mod runner;
mod shader_compile_case;

pub async fn run_incremental_shader_compile(_: embassy_executor::Spawner) -> ! {
    let (sw_int, timg0, _rmt, usb_device, _gpio18, _flash, _gpio4, _gpio20, _wifi) = init_board();
    start_runtime(timg0, sw_int);

    let usb_serial = UsbSerialJtag::new(usb_device);
    let serial_io = Esp32UsbSerialIo::new(usb_serial);
    let serial_io_shared = Rc::new(RefCell::new(serial_io));

    logger::set_log_serial(serial_io_shared);
    logger::init(logger::log_write_bytes);

    embassy_time::Timer::after(embassy_time::Duration::from_millis(100)).await;
    cycle_counter::setup();

    lps_builtins::ensure_builtins_referenced();
    let mut table = BuiltinTable::new();
    table.populate();
    let mut options = NativeCompileOptions::default();
    options.stage_trace = true;
    let engine = LpsEngine::new(NativeJitEngine::new(Arc::new(table), options));

    info!("[inc-shader-compile] === incremental shader compile experiment starting ===");
    runner::run_all(&engine);
    info!("[inc-shader-compile] === DONE ===");

    loop {
        embassy_time::Timer::after(embassy_time::Duration::from_secs(60)).await;
    }
}
