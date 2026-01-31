//! Test implementations of host functions.
//!
//! These implementations use `std` and are only available when the `test` feature is enabled.

#[cfg(feature = "test")]
extern crate std;

/// Debug function implementation for tests.
///
/// Checks `DEBUG=1` environment variable and only prints if set.
#[cfg(feature = "test")]
#[unsafe(no_mangle)]
pub extern "C" fn __host_debug(ptr: *const u8, len: usize) {
    if std::env::var("DEBUG").as_deref() == Ok("1") {
        unsafe {
            let slice = core::slice::from_raw_parts(ptr, len);
            let msg = core::str::from_utf8_unchecked(slice);
            std::println!("{}", msg);
        }
    }
}

/// Println function implementation for tests.
///
/// Always prints to stdout.
#[cfg(feature = "test")]
#[unsafe(no_mangle)]
pub extern "C" fn __host_println(ptr: *const u8, len: usize) {
    unsafe {
        let slice = core::slice::from_raw_parts(ptr, len);
        let msg = core::str::from_utf8_unchecked(slice);
        std::println!("{}", msg);
    }
}
