//! Host function implementations for JIT-compiled GLSL code (no_std mode).
//!
//! These functions are called by JIT-compiled GLSL code when using host functions
//! like __host_log. They must be provided by the firmware binary.

#[cfg(not(feature = "test_app"))]
extern crate alloc;

#[cfg(not(feature = "test_app"))]
use crate::logger::write_log;

/// Host function implementation for log output (no_std mode).
/// Called by JIT-compiled GLSL code when using __host_log.
#[unsafe(no_mangle)]
pub extern "C" fn lp_jit_host_log(
    level: u8,
    module_path_ptr: *const u8,
    module_path_len: usize,
    msg_ptr: *const u8,
    msg_len: usize,
) {
    unsafe {
        let module_path_slice = core::slice::from_raw_parts(module_path_ptr, module_path_len);
        let msg_slice = core::slice::from_raw_parts(msg_ptr, msg_len);

        let level_str = match level {
            0 => "ERROR",
            1 => "WARN",
            2 => "INFO",
            3 => "DEBUG",
            _ => "DEBUG",
        };

        if let (Ok(module_path), Ok(msg)) = (
            core::str::from_utf8(module_path_slice),
            core::str::from_utf8(msg_slice),
        ) {
            #[cfg(not(feature = "test_app"))]
            {
                let log_msg = alloc::format!("[{}] {}: {}\r\n", level_str, module_path, msg);
                write_log(&log_msg);
            }
            #[cfg(feature = "test_app")]
            {
                esp_println::println!("[{}] {}: {}", level_str, module_path, msg);
            }
        } else {
            #[cfg(not(feature = "test_app"))]
            {
                let log_msg = alloc::format!("[{}] [invalid UTF-8 log message]\r\n", level_str);
                write_log(&log_msg);
            }
            #[cfg(feature = "test_app")]
            {
                esp_println::println!("[{}] [invalid UTF-8 log message]", level_str);
            }
        }
    }
}
