//! Host function implementations for JIT mode.
//!
//! These functions delegate to the log crate for output.

/// Log function implementation for JIT mode.
///
/// Creates a log::Record and calls log::log!() directly.
#[unsafe(no_mangle)]
pub extern "C" fn __host_log(
    level: u8,
    module_path_ptr: *const u8,
    module_path_len: usize,
    msg_ptr: *const u8,
    msg_len: usize,
) {
    unsafe {
        // Read strings from pointers
        let module_path_slice = core::slice::from_raw_parts(module_path_ptr, module_path_len);
        let msg_slice = core::slice::from_raw_parts(msg_ptr, msg_len);

        let module_path = core::str::from_utf8_unchecked(module_path_slice);
        let msg = core::str::from_utf8_unchecked(msg_slice);

        // Convert level
        let level = match level {
            0 => log::Level::Error,
            1 => log::Level::Warn,
            2 => log::Level::Info,
            3 => log::Level::Debug,
            _ => log::Level::Debug,
        };

        // Create log record and call log::log!()
        log::log!(target: module_path, level, "{msg}");
    }
}
