/// Syscall number for panic
pub(crate) const SYSCALL_PANIC: i32 = 1;

/// Number of syscall arguments
pub(crate) const SYSCALL_ARGS: usize = 7;

/// System call implementation
pub(crate) fn syscall(nr: i32, args: &[i32; SYSCALL_ARGS]) -> i32 {
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
    if error != 0 {
        error
    } else {
        value
    }
}
