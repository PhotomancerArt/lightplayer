//! ESP32 logging module
//!
//! Provides logging functionality using USB serial directly, avoiding conflicts
//! with esp_println's global singleton. Uses our own USB serial instance
//! that's shared with the transport layer.

extern crate alloc;

use alloc::format;
use core::sync::atomic::{AtomicPtr, Ordering};
use log::{Level, LevelFilter, Log, Metadata, Record};

/// Function type for writing log messages to USB serial
pub type LogWriteFn = fn(&str);

/// Global log write function
/// This is set once at startup and then used by the logger
static LOG_WRITE_FN: AtomicPtr<()> = AtomicPtr::new(core::ptr::null_mut());

/// Set the log write function
///
/// # Safety
/// The function pointer must remain valid for the lifetime of the program
pub unsafe fn set_log_write_fn(write_fn: LogWriteFn) {
    LOG_WRITE_FN.store(write_fn as *mut (), Ordering::Release);
}

/// ESP32 logger that uses USB serial directly
pub struct Esp32Logger {
    max_level: LevelFilter,
}

impl Esp32Logger {
    /// Create a new ESP32 logger with the given max level
    pub fn new(max_level: LevelFilter) -> Self {
        Self { max_level }
    }

    /// Create a new ESP32 logger with default info level
    pub fn default() -> Self {
        Self::new(LevelFilter::Info)
    }
}

impl Log for Esp32Logger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= self.max_level
    }

    fn log(&self, record: &Record) {
        if !self.enabled(record.metadata()) {
            return;
        }

        let module_path = record.module_path().unwrap_or("unknown");
        let level_str = match record.level() {
            Level::Error => "ERROR",
            Level::Warn => "WARN",
            Level::Info => "INFO",
            Level::Debug => "DEBUG",
            Level::Trace => "TRACE",
        };

        // Format message with newline
        let msg = format!("[{}] {}: {}\r\n", level_str, module_path, record.args());

        // Write to USB serial via our write function
        let write_fn_ptr = LOG_WRITE_FN.load(Ordering::Acquire);
        if !write_fn_ptr.is_null() {
            let write_fn: LogWriteFn = unsafe { core::mem::transmute(write_fn_ptr) };
            write_fn(&msg);
        }
    }

    fn flush(&self) {
        // No-op - USB serial handles flushing internally
    }
}

/// Initialize the ESP32 logger with a write function
///
/// Call this once at startup after USB serial is initialized.
/// The write function should write to your USB serial instance.
pub fn init(write_fn: LogWriteFn) {
    unsafe {
        set_log_write_fn(write_fn);
    }

    let logger = alloc::boxed::Box::new(Esp32Logger::default());
    log::set_logger(alloc::boxed::Box::leak(logger))
        .map(|()| log::set_max_level(LevelFilter::Info))
        .expect("Failed to set ESP32 logger");
}

/// Write a log message directly (for use from C code, e.g., JIT host functions)
///
/// This bypasses the log crate and writes directly to USB serial.
/// Useful for host functions called from JIT-compiled code.
pub fn write_log(msg: &str) {
    let write_fn_ptr = LOG_WRITE_FN.load(Ordering::Acquire);
    if !write_fn_ptr.is_null() {
        let write_fn: LogWriteFn = unsafe { core::mem::transmute(write_fn_ptr) };
        write_fn(msg);
    }
}
