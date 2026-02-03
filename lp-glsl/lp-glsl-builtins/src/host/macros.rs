//! Macros for host functions.
//!
//! These macros expand to calls to the underlying host functions,
//! following the same pattern as `lp-glsl-builtins-emu-app` `println!` macros.

/// Debug macro for host functions.
///
/// Usage:
/// ```ignore
/// lp_glsl_builtins::host_debug!("message: {}", value);
/// ```
///
/// This macro formats the string first, then calls `__host_log` with debug level.
/// The underlying function is linked differently depending on context:
/// - Emulator: Implemented in `lp-glsl-builtins-emu-app` (syscall-based)
/// - Tests: Implemented in `lp-glsl-builtins` with `std` (gated by feature flag)
/// - JIT: Implemented in `lp-glsl-compiler` (delegates to log crate)
#[macro_export]
macro_rules! host_debug {
    ($($arg:tt)*) => {
        {
            // Check for std feature first (this exists in all crates that might use this)
            #[cfg(feature = "std")]
            {
                // When std is available, check if test feature exists
                #[cfg(feature = "test")]
                {
                    // With std and test feature, use std::format! and call test implementation
                    let formatted = std::format!($($arg)*);
                    $crate::host::__host_log(3u8, b"".as_ptr(), 0, formatted.as_ptr(), formatted.len());
                }
                #[cfg(not(feature = "test"))]
                {
                    // With std but not test - use extern function (for JIT or other contexts)
                    let formatted = std::format!($($arg)*);
                    unsafe extern "C" {
                        fn __host_log(
                            level: u8,
                            module_path_ptr: *const u8,
                            module_path_len: usize,
                            msg_ptr: *const u8,
                            msg_len: usize,
                        );
                    }
                    unsafe {
                        __host_log(3u8, b"".as_ptr(), 0, formatted.as_ptr(), formatted.len());
                    }
                }
            }
            #[cfg(not(feature = "std"))]
            {
                // Without std, use core::format_args! and format into static buffer
                $crate::host::_debug_format(core::format_args!($($arg)*));
            }
        }
    };
}
