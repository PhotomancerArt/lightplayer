// Re-export syscall constants from shared crate
pub use lp_riscv_emu_shared::{
    SYSCALL_ARGS, SYSCALL_LOG, SYSCALL_PANIC, SYSCALL_SERIAL_HAS_DATA, SYSCALL_SERIAL_READ,
    SYSCALL_SERIAL_WRITE, SYSCALL_TIME_MS, SYSCALL_WRITE, SYSCALL_YIELD,
};

/// System call implementation
pub fn syscall(nr: i32, args: &[i32; SYSCALL_ARGS]) -> i32 {
    let error: i32;
    let value: i32;
    unsafe {
        core::arch::asm!(
            "ecall",
            in("x17") nr,
            inlateout("x10") args[0] => error,
            inlateout("x11") args[1] => value,
            in("x12") args[2],
            in("x13") args[3],
            in("x14") args[4],
            in("x15") args[5],
            in("x16") args[6],
        );
    }
    if error != 0 { error } else { value }
}

pub fn sys_yield() {
    let args = [0i32; SYSCALL_ARGS];
    syscall(SYSCALL_YIELD, &args);
}

/// Write bytes to serial output buffer
///
/// # Arguments
/// * `data` - Bytes to write
///
/// # Returns
/// * Positive number: bytes written
/// * Negative number: error code
pub fn sys_serial_write(data: &[u8]) -> i32 {
    if data.is_empty() {
        return 0;
    }

    let mut args = [0i32; SYSCALL_ARGS];
    args[0] = data.as_ptr() as i32;
    args[1] = data.len() as i32;
    syscall(SYSCALL_SERIAL_WRITE, &args)
}

/// Read bytes from serial input buffer
///
/// # Arguments
/// * `buf` - Buffer to read into
///
/// # Returns
/// * Positive number: bytes read
/// * Negative number: error code
pub fn sys_serial_read(buf: &mut [u8]) -> i32 {
    if buf.is_empty() {
        return 0;
    }

    let mut args = [0i32; SYSCALL_ARGS];
    args[0] = buf.as_ptr() as i32;
    args[1] = buf.len() as i32;
    syscall(SYSCALL_SERIAL_READ, &args)
}

/// Check if serial input has data available
///
/// # Returns
/// * `true` if data is available
/// * `false` otherwise
pub fn sys_serial_has_data() -> bool {
    let args = [0i32; SYSCALL_ARGS];
    syscall(SYSCALL_SERIAL_HAS_DATA, &args) != 0
}
