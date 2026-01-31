/// Host function implementation for debug output (no_std mode).
/// Called by JIT-compiled GLSL code when using __host_debug.
#[unsafe(no_mangle)]
pub extern "C" fn lp_jit_host_debug(ptr: *const u8, len: usize) {
    unsafe {
        let slice = core::slice::from_raw_parts(ptr, len);
        if let Ok(msg) = core::str::from_utf8(slice) {
            esp_println::println!("{}", msg);
        } else {
            esp_println::println!("[invalid UTF-8 debug message]");
        }
    }
}

/// Host function implementation for print output (no_std mode).
/// Called by JIT-compiled GLSL code when using __host_println.
#[unsafe(no_mangle)]
pub extern "C" fn lp_jit_host_println(ptr: *const u8, len: usize) {
    unsafe {
        let slice = core::slice::from_raw_parts(ptr, len);
        if let Ok(msg) = core::str::from_utf8(slice) {
            esp_println::println!("{}", msg);
        } else {
            esp_println::println!("[invalid UTF-8 print message]");
        }
    }
}
