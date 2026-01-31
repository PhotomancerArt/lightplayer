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
/// This macro formats the string first, then calls `__host_debug(&str)`.
/// The underlying function is linked differently depending on context:
/// - Emulator: Implemented in `lp-glsl-builtins-emu-app` (syscall-based)
/// - Tests: Implemented in `lp-glsl-builtins` with `std` (gated by feature flag)
/// - JIT: Implemented in `lp-glsl-compiler` (delegates to `lp-glsl-compiler::debug!`)
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
                    $crate::host::__host_debug(formatted.as_ptr(), formatted.len());
                }
                #[cfg(not(feature = "test"))]
                {
                    // With std but not test - use extern function (for JIT or other contexts)
                    let formatted = std::format!($($arg)*);
                    unsafe extern "C" {
                        fn __host_debug(ptr: *const u8, len: usize);
                    }
                    unsafe {
                        __host_debug(formatted.as_ptr(), formatted.len());
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

/// Println macro for host functions.
///
/// Usage:
/// ```ignore
/// lp_glsl_builtins::host_println!("message: {}", value);
/// ```
///
/// This macro formats the string first, then calls `__host_println(&str)`.
/// The underlying function is linked differently depending on context:
/// - Emulator: Implemented in `lp-glsl-builtins-emu-app` (syscall-based)
/// - Tests: Implemented in `lp-glsl-builtins` with `std` (gated by feature flag)
/// - JIT: Implemented in `lp-glsl-compiler` (delegates to `std::println!`)
#[macro_export]
macro_rules! host_println {
    () => {
        let newline = "\n";
        #[cfg(feature = "std")]
        {
            #[cfg(feature = "test")]
            {
                $crate::host::__host_println(newline.as_ptr(), newline.len());
            }
            #[cfg(not(feature = "test"))]
            {
                unsafe extern "C" {
                    fn __host_println(ptr: *const u8, len: usize);
                }
                unsafe {
                    __host_println(newline.as_ptr(), newline.len());
                }
            }
        }
        #[cfg(not(feature = "std"))]
        {
            $crate::host::_println_format(core::format_args!("{}", newline));
        }
    };
    ($($arg:tt)*) => {
        {
            #[cfg(feature = "std")]
            {
                #[cfg(feature = "test")]
                {
                    // With std and test feature, use std::format! and call test implementation
                    let formatted = std::format!($($arg)*);
                    $crate::host::__host_println(formatted.as_ptr(), formatted.len());
                }
                #[cfg(not(feature = "test"))]
                {
                    // With std but not test - use extern function (for JIT or other contexts)
                    let formatted = std::format!($($arg)*);
                    unsafe extern "C" {
                        fn __host_println(ptr: *const u8, len: usize);
                    }
                    unsafe {
                        __host_println(formatted.as_ptr(), formatted.len());
                    }
                }
            }
            #[cfg(not(feature = "std"))]
            {
                // Without std, use core::format_args! and format into static buffer
                $crate::host::_println_format(core::format_args!($($arg)*));
            }
        }
    };
}
