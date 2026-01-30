//! Syscall number constants shared between host and guest

/// Syscall number for panic
pub const SYSCALL_PANIC: i32 = 1;

/// Syscall number for write (always prints)
pub const SYSCALL_WRITE: i32 = 2;

/// Syscall number for debug (only prints if DEBUG=1)
pub const SYSCALL_DEBUG: i32 = 3;

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
