use crate::syscall::{SYSCALL_ARGS, SYSCALL_LOG, syscall};

/// Log function implementation for emulator.
///
/// This function is called by the logger implementation.
/// Uses SYSCALL_LOG syscall with level, module_path, and message.
#[unsafe(no_mangle)]
pub extern "C" fn __host_log(
    level: u8,
    module_path_ptr: *const u8,
    module_path_len: usize,
    msg_ptr: *const u8,
    msg_len: usize,
) {
    let level_i32 = level as i32;
    let module_path_ptr_i32 = module_path_ptr as usize as i32;
    let module_path_len_i32 = module_path_len as i32;
    let msg_ptr_i32 = msg_ptr as usize as i32;
    let msg_len_i32 = msg_len as i32;

    let mut args = [0i32; SYSCALL_ARGS];
    args[0] = level_i32;
    args[1] = module_path_ptr_i32;
    args[2] = module_path_len_i32;
    args[3] = msg_ptr_i32;
    args[4] = msg_len_i32;
    let _ = syscall(SYSCALL_LOG, &args);
}
