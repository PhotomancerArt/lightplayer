//! Syscall number constants shared between host and guest

/// Syscall number for panic
pub const SYSCALL_PANIC: i32 = 1;

/// Syscall number for write (always prints)
pub const SYSCALL_WRITE: i32 = 2;

/// Syscall number for log (supports all log levels, filtered by RUST_LOG)
pub const SYSCALL_LOG: i32 = 3;

/// Syscall number for yield (yield control back to host)
pub const SYSCALL_YIELD: i32 = 4;

/// Syscall number for serial write (write bytes to serial output buffer)
pub const SYSCALL_SERIAL_WRITE: i32 = 5;

/// Syscall number for serial read (read bytes from serial input buffer)
pub const SYSCALL_SERIAL_READ: i32 = 6;

/// Syscall number for serial has_data (check if serial input has data)
pub const SYSCALL_SERIAL_HAS_DATA: i32 = 7;

/// Syscall number for time_ms (get elapsed milliseconds since emulator start)
pub const SYSCALL_TIME_MS: i32 = 8;

/// Number of syscall arguments
pub const SYSCALL_ARGS: usize = 7;

/// Convert log level to syscall level value
pub fn level_to_syscall(level: log::Level) -> i32 {
    match level {
        log::Level::Error => 0,
        log::Level::Warn => 1,
        log::Level::Info => 2,
        log::Level::Debug => 3,
        log::Level::Trace => 3, // Map trace to debug for now
    }
}

/// Convert syscall level value to log level
pub fn syscall_to_level(level: i32) -> Option<log::Level> {
    match level {
        0 => Some(log::Level::Error),
        1 => Some(log::Level::Warn),
        2 => Some(log::Level::Info),
        3 => Some(log::Level::Debug),
        _ => None,
    }
}
