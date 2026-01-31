//! Host function implementations for JIT mode.
//!
//! These functions delegate to `lp-glsl-compiler` macros for output.

/// Debug function implementation for JIT mode.
///
/// Delegates to `lp-glsl-compiler::debug!` macro.
#[unsafe(no_mangle)]
pub extern "C" fn __host_debug(ptr: *const u8, len: usize) {
    unsafe {
        let slice = core::slice::from_raw_parts(ptr, len);
        let msg = core::str::from_utf8_unchecked(slice);
        // Delegate to lp-glsl-compiler debug macro
        crate::debug!("{}", msg);
    }
}

/// Println function implementation for JIT mode.
///
/// Delegates to `std::println!`.
#[unsafe(no_mangle)]
pub extern "C" fn __host_println(ptr: *const u8, len: usize) {
    unsafe {
        let slice = core::slice::from_raw_parts(ptr, len);
        let msg = core::str::from_utf8_unchecked(slice);
        // Delegate to std::println!
        std::println!("{msg}");
    }
}
