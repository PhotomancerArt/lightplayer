//! ESP32 logger implementation.
//!
//! Routes log calls to a provided print function (typically USB serial).

extern crate alloc;

use alloc::format;
use log::{Level, LevelFilter, Log, Metadata, Record};

/// Function type for printing log messages
pub type PrintFn = fn(&str);

/// ESP32 logger that routes to a print function
pub struct Esp32Logger {
    max_level: LevelFilter,
    print_fn: PrintFn,
}

impl Esp32Logger {
    /// Create a new ESP32 logger with the given max level and print function
    pub fn new(max_level: LevelFilter, print_fn: PrintFn) -> Self {
        Self {
            max_level,
            print_fn,
        }
    }

    /// Create a new ESP32 logger with default info level
    pub fn default(print_fn: PrintFn) -> Self {
        Self::new(LevelFilter::Info, print_fn)
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

        // Format message
        let msg = format!("[{}] {}: {}", level_str, module_path, record.args());

        // Call print function
        (self.print_fn)(&msg);
    }

    fn flush(&self) {
        // No-op
    }
}

/// Initialize the ESP32 logger with a print function
///
/// Call this once at startup in ESP32 firmware.
pub fn init(print_fn: PrintFn) {
    let logger = alloc::boxed::Box::new(Esp32Logger::default(print_fn));
    log::set_logger(alloc::boxed::Box::leak(logger))
        .map(|()| log::set_max_level(LevelFilter::Info))
        .expect("Failed to set ESP32 logger");
}
