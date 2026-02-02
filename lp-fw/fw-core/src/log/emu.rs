//! Emulator logger implementation.
//!
//! Routes log calls to syscalls via __host_log.

extern crate alloc;

use alloc::format;
use log::{Level, LevelFilter, Log, Metadata, Record};

/// External function for logging (provided by lp-riscv-emu-guest)
extern "C" {
    fn __host_log(
        level: u8,
        module_path_ptr: *const u8,
        module_path_len: usize,
        msg_ptr: *const u8,
        msg_len: usize,
    );
}

/// Logger that routes to syscalls
pub struct EmuLogger;

impl Log for EmuLogger {
    fn enabled(&self, _metadata: &Metadata) -> bool {
        // Always enabled - filtering happens on host side
        true
    }

    fn log(&self, record: &Record) {
        let level = match record.level() {
            Level::Error => 0,
            Level::Warn => 1,
            Level::Info => 2,
            Level::Debug => 3,
            Level::Trace => 3,
        };

        let module_path = record.module_path().unwrap_or("unknown");
        let module_path_bytes = module_path.as_bytes();

        let msg = format!("{}", record.args());
        let msg_bytes = msg.as_bytes();

        unsafe {
            __host_log(
                level,
                module_path_bytes.as_ptr(),
                module_path_bytes.len(),
                msg_bytes.as_ptr(),
                msg_bytes.len(),
            );
        }
    }

    fn flush(&self) {
        // No-op
    }
}

/// Initialize the emulator logger
pub fn init() {
    let logger = alloc::boxed::Box::new(EmuLogger);
    log::set_logger(alloc::boxed::Box::leak(logger))
        .map(|()| log::set_max_level(LevelFilter::Trace))
        .expect("Failed to set emulator logger");
}
