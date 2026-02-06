//! ESP32 logging module
//!
//! Provides logging functionality using USB serial directly. Uses our own USB serial instance
//! that's shared with the transport layer.

extern crate alloc;

use alloc::format;
use core::sync::atomic::{AtomicPtr, Ordering};
use fw_core::serial::SerialIo;
use log::{Level, LevelFilter, Log, Metadata, Record};

const LOG_LEVEL: LevelFilter = LevelFilter::Debug;

/// Initialize the ESP32 logger with a write function
///
/// Call this once at startup after USB serial is initialized.
/// The write function should write to your USB serial instance.
pub fn init(write_fn: LogWriteFn) {
    unsafe {
        set_log_write_fn(write_fn);
    }

    let logger = alloc::boxed::Box::new(Esp32Logger::new(LevelFilter::Debug));
    log::set_logger(alloc::boxed::Box::leak(logger))
        .map(|()| log::set_max_level(LOG_LEVEL))
        .expect("Failed to set ESP32 logger");
}

/// Function type for writing log messages to USB serial
pub type LogWriteFn = fn(&str);

/// Global log write function
/// This is set once at startup and then used by the logger
static LOG_WRITE_FN: AtomicPtr<()> = AtomicPtr::new(core::ptr::null_mut());

/// USB serial instance for our logger
static LOG_SERIAL: AtomicPtr<()> = AtomicPtr::new(core::ptr::null_mut());

/// USB serial instance for esp-println (used by esp-backtrace for panic output)
static ESP_PRINTLN_SERIAL: AtomicPtr<()> = AtomicPtr::new(core::ptr::null_mut());

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

        // Filter out verbose esp_rtos debug logs
        if let Some(module_path) = record.module_path() {
            if module_path.starts_with("esp_rtos") {
                return;
            }
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

/// Set the USB serial instance for our logger to use
pub fn set_log_serial(
    serial_io: alloc::rc::Rc<core::cell::RefCell<crate::serial::Esp32UsbSerialIo>>,
) {
    // Leak the Rc to get a 'static reference
    let leaked = alloc::boxed::Box::leak(alloc::boxed::Box::new(serial_io));
    LOG_SERIAL.store(leaked as *mut _ as *mut (), Ordering::Release);
}

/// Write function for our logger to use
///
/// This function is called synchronously from the log crate and writes
/// synchronously to USB serial.
pub fn log_write_bytes(msg: &str) {
    let serial_ptr = LOG_SERIAL.load(Ordering::Acquire);
    if !serial_ptr.is_null() {
        let serial_io: &alloc::rc::Rc<core::cell::RefCell<crate::serial::Esp32UsbSerialIo>> = unsafe {
            &*(serial_ptr
                as *const alloc::rc::Rc<core::cell::RefCell<crate::serial::Esp32UsbSerialIo>>)
        };
        if let Ok(mut io) = serial_io.try_borrow_mut() {
            // Use synchronous write directly
            let _ = io.write(msg.as_bytes());
        }
    }
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

/// Set the USB serial instance for esp-println to use
///
/// This allows esp-println (used by esp-backtrace) to route output through
/// our shared USB serial instance.
pub fn set_esp_println_serial(
    serial_io: alloc::rc::Rc<core::cell::RefCell<crate::serial::Esp32UsbSerialIo>>,
) {
    // Leak the Rc to get a 'static reference
    let leaked = alloc::boxed::Box::leak(alloc::boxed::Box::new(serial_io));
    ESP_PRINTLN_SERIAL.store(leaked as *mut _ as *mut (), Ordering::Release);
}

/// Write function for esp-println to use
///
/// This is called by esp-println when a custom writer is set.
/// It writes bytes to our shared USB serial instance.
pub fn esp_println_write_bytes(bytes: &[u8]) {
    let serial_ptr = ESP_PRINTLN_SERIAL.load(Ordering::Acquire);
    if !serial_ptr.is_null() {
        let serial_io: &alloc::rc::Rc<core::cell::RefCell<crate::serial::Esp32UsbSerialIo>> = unsafe {
            &*(serial_ptr
                as *const alloc::rc::Rc<core::cell::RefCell<crate::serial::Esp32UsbSerialIo>>)
        };
        if let Ok(mut io) = serial_io.try_borrow_mut() {
            let _ = SerialIo::write(&mut *io, bytes);
        }
    }
}
