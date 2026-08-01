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

/// Syscall number for allocation tracing (alloc/dealloc/realloc events)
///
/// Args for alloc:   a0=0, a1=ptr, a2=size, a3=free_bytes
/// Args for dealloc: a0=1, a1=ptr, a2=size, a3=free_bytes
/// Args for realloc: a0=2, a1=old_ptr, a2=new_ptr, a3=old_size, a4=new_size, a5=free_bytes
pub const SYSCALL_ALLOC_TRACE: i32 = 9;

/// Syscall number for emitting a perf event from guest to host.
///
/// ABI: a0=name_ptr, a1=name_len, a2=kind (0=Begin, 1=End, 2=Instant).
/// a3 reserved for a future `arg: u32` payload.
pub const SYSCALL_PERF_EVENT: i32 = 10;

/// JIT-symbol overlay: notify host that a JIT module has been linked.
///
/// ABI: `a0 = base_addr (u32)`, `a1 = len (u32)`, `a2 = count (u32)`,
/// `a3 = entries_ptr (u32)`. The entries array is `count` records of
/// `JitSymbolEntry` (see [`crate::JitSymbolEntry`]).
pub const SYSCALL_JIT_MAP_LOAD: i32 = 11;

/// Reserved for m5 JIT-symbol overlay (unload).
/// Not yet implemented; reserving the number to avoid collision — deferred, see m5 plan / future-work doc.
pub const SYSCALL_JIT_MAP_UNLOAD: i32 = 12;

/// Allocation event type constants
pub const ALLOC_TRACE_ALLOC: i32 = 0;
pub const ALLOC_TRACE_DEALLOC: i32 = 1;
pub const ALLOC_TRACE_REALLOC: i32 = 2;
/// OOM: a0=3, a1=0, a2=requested_size (uses trace_event layout: event_type, ptr, size, free)
pub const ALLOC_TRACE_OOM: i32 = 3;

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
