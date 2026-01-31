use crate::syscall::{SYSCALL_ARGS, syscall};

/// Syscall number for write (always prints)
const SYSCALL_WRITE: i32 = 2;

/// Syscall number for debug (only prints if DEBUG=1)
const SYSCALL_DEBUG: i32 = 3;

/// Debug function implementation for emulator.
///
/// This function is called by the `host_debug!` macro.
/// Uses a separate syscall so the emulator can check DEBUG=1 env var.
#[unsafe(no_mangle)]
pub extern "C" fn __host_debug(ptr: *const u8, len: usize) {
    let ptr = ptr as usize as i32;
    let len = len as i32;

    let mut args = [0i32; SYSCALL_ARGS];
    args[0] = ptr;
    args[1] = len;
    let _ = syscall(SYSCALL_DEBUG, &args);

    // Add trailing newline
    let newline = "\n";
    let ptr = newline.as_ptr() as usize as i32;
    let len = newline.len() as i32;
    let mut args = [0i32; SYSCALL_ARGS];
    args[0] = ptr;
    args[1] = len;
    let _ = syscall(SYSCALL_DEBUG, &args);
}

/// Println function implementation for emulator.
///
/// This function is called by the `host_println!` macro.
#[unsafe(no_mangle)]
pub extern "C" fn __host_println(ptr: *const u8, len: usize) {
    // Print the message
    let ptr = ptr as usize as i32;
    let len = len as i32;

    let mut args = [0i32; SYSCALL_ARGS];
    args[0] = ptr;
    args[1] = len;
    let _ = syscall(SYSCALL_WRITE, &args);

    // Print newline
    let newline = "\n";
    let ptr = newline.as_ptr() as usize as i32;
    let len = newline.len() as i32;
    let mut args = [0i32; SYSCALL_ARGS];
    args[0] = ptr;
    args[1] = len;
    let _ = syscall(SYSCALL_WRITE, &args);
}
