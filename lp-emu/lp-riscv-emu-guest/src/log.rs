//! Logger implementation for emulator guest code.
//!
//! Routes all log calls to SYSCALL_LOG syscall.

extern crate alloc;

use alloc::format;
use log::{Level, Log, Metadata, Record};

use crate::host::__host_log;

/// Logger that routes to syscalls
pub struct SyscallLogger;

impl Log for SyscallLogger {
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
            Level::Trace => 3, // Map trace to debug
        };

        // Get module path
        let module_path = record.module_path().unwrap_or("unknown");
        let module_path_bytes = module_path.as_bytes();

        // Format message using alloc::format!
        let msg = format!("{}", record.args());
        let msg_bytes = msg.as_bytes();

        // Call syscall
        __host_log(
            level,
            module_path_bytes.as_ptr(),
            module_path_bytes.len(),
            msg_bytes.as_ptr(),
            msg_bytes.len(),
        );
    }

    fn flush(&self) {
        // No-op for syscalls
    }
}

/// Initialize the syscall logger
///
/// Call this once at startup in emulator guest code.
pub fn init() {
    let logger = alloc::boxed::Box::new(SyscallLogger);
    log::set_logger(alloc::boxed::Box::leak(logger))
        .map(|()| log::set_max_level(log::LevelFilter::Trace))
        .expect("Failed to set syscall logger");
}
