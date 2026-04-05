//! Logger implementation for GLSL builtins.
//!
//! Routes log calls to __host_log function which works in both
//! emulator (syscalls) and JIT (log crate) contexts.

extern crate alloc;

use alloc::format;
use log::{Level, Log, Metadata, Record};

// External function for logging (provided by emulator or JIT)
unsafe extern "C" {
    fn __host_log(
        level: u8,
        module_path_ptr: *const u8,
        module_path_len: usize,
        msg_ptr: *const u8,
        msg_len: usize,
    );
}

/// Logger that routes to __host_log
pub struct BuiltinsLogger;

impl Log for BuiltinsLogger {
    fn enabled(&self, _metadata: &Metadata) -> bool {
        // Always enabled - filtering happens on host side (emulator) or via log crate (JIT)
        true
    }

    fn log(&self, record: &Record) {
        let level = match record.level() {
            Level::Error => 0,
            Level::Warn => 1,
            Level::Info => 2,
            Level::Debug => 3,
            Level::Trace => 3, // Map trace to debug
        };

        // Get module path
        let module_path = record.module_path().unwrap_or("unknown");
        let module_path_bytes = module_path.as_bytes();

        // Format message
        let msg = format!("{}", record.args());
        let msg_bytes = msg.as_bytes();

        // Call __host_log
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

/// Initialize the builtins logger
///
/// Call this once before running GLSL code.
pub fn init() {
    let logger = alloc::boxed::Box::new(BuiltinsLogger);
    log::set_logger(alloc::boxed::Box::leak(logger))
        .map(|()| log::set_max_level(log::LevelFilter::Trace))
        .expect("Failed to set builtins logger");
}
