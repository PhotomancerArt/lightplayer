//! Formatting helpers for no_std environments.
//!
//! Uses a static buffer to format strings without allocation.

use core::fmt::{self, Write};

// Declare extern functions that will be linked from lp-glsl-builtins-emu-app
unsafe extern "C" {
    fn __host_debug(ptr: *const u8, len: usize);
    fn __host_println(ptr: *const u8, len: usize);
}

/// Static buffer for formatting (256 bytes should be enough for most debug messages)
const BUFFER_SIZE: usize = 256;
static mut FORMAT_BUFFER: [u8; BUFFER_SIZE] = [0; BUFFER_SIZE];
static mut BUFFER_LEN: usize = 0;

/// Writer that writes to the static buffer
struct BufferWriter;

impl Write for BufferWriter {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        unsafe {
            let start = *core::ptr::addr_of!(BUFFER_LEN);
            let end = (start + s.len()).min(BUFFER_SIZE);
            if end > start {
                let buffer_ptr = core::ptr::addr_of_mut!(FORMAT_BUFFER);
                let src = s.as_bytes().as_ptr();
                let dst = buffer_ptr.cast::<u8>().add(start);
                let len = end - start;
                core::ptr::copy_nonoverlapping(src, dst, len);
                *core::ptr::addr_of_mut!(BUFFER_LEN) = end;
            }
            // If buffer is full, silently truncate (better than panicking)
            Ok(())
        }
    }
}

/// Format arguments and call __host_debug with the formatted string.
///
/// This function uses a static buffer to format the string without allocation.
/// The buffer is shared, so this is not thread-safe, but that's fine for
/// single-threaded emulator execution.
pub fn _debug_format(args: fmt::Arguments) {
    unsafe {
        // Reset buffer
        *core::ptr::addr_of_mut!(BUFFER_LEN) = 0;

        // Format into buffer
        let mut writer = BufferWriter;
        let _ = writer.write_fmt(args);

        // Call host function with buffer contents
        let len = *core::ptr::addr_of!(BUFFER_LEN);
        __host_debug(core::ptr::addr_of!(FORMAT_BUFFER).cast(), len);
    }
}

/// Format arguments and call __host_println with the formatted string.
///
/// This function uses a static buffer to format the string without allocation.
/// The buffer is shared, so this is not thread-safe, but that's fine for
/// single-threaded emulator execution.
pub fn _println_format(args: fmt::Arguments) {
    unsafe {
        // Reset buffer
        *core::ptr::addr_of_mut!(BUFFER_LEN) = 0;

        // Format into buffer
        let mut writer = BufferWriter;
        let _ = writer.write_fmt(args);

        // Call host function with buffer contents
        let len = *core::ptr::addr_of!(BUFFER_LEN);
        __host_println(core::ptr::addr_of!(FORMAT_BUFFER).cast(), len);
    }
}
