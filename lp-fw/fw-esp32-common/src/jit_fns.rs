//! Host function implementations for JIT-compiled GLSL code (no_std mode).
//!
//! These functions are called by JIT-compiled GLSL code when using host functions
//! like __host_log. They must be provided by the firmware binary.

/// Host function implementation for log output (no_std mode).
/// Called by JIT-compiled GLSL code when using __host_log.
///
/// Goes through the `log` facade to whichever logger the image installed:
/// the log ring on a USB-link image ([`crate::log_ring_logger`], so a shader's
/// log line rides the link's log channel like any other), the serial writer
/// ([`crate::logger`]) on the classic and in the harnesses — where it prints
/// `[LEVEL] module: message` exactly as the old direct write did. Like that
/// write, it is not filtered by `log::max_level()` (the facade's macros apply
/// that gate; this calls the logger itself).
#[unsafe(no_mangle)]
pub extern "C" fn lp_jit_host_log(
    level: u8,
    module_path_ptr: *const u8,
    module_path_len: usize,
    msg_ptr: *const u8,
    msg_len: usize,
) {
    let (module_path, msg) = unsafe {
        (
            core::slice::from_raw_parts(module_path_ptr, module_path_len),
            core::slice::from_raw_parts(msg_ptr, msg_len),
        )
    };
    let level = match level {
        0 => log::Level::Error,
        1 => log::Level::Warn,
        2 => log::Level::Info,
        _ => log::Level::Debug,
    };
    match (core::str::from_utf8(module_path), core::str::from_utf8(msg)) {
        (Ok(module_path), Ok(msg)) => log::logger().log(
            &log::Record::builder()
                .level(level)
                .target(module_path)
                .module_path(Some(module_path))
                .args(format_args!("{msg}"))
                .build(),
        ),
        _ => log::logger().log(
            &log::Record::builder()
                .level(level)
                .args(format_args!("[invalid UTF-8 log message]"))
                .build(),
        ),
    }
}
