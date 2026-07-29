//! Guest side of the syscall ABI (host side: `lp-xt-elf/src/abi.rs`).
//!
//! Convention: `a2` = syscall number, `a3..a5` = arguments, result returned in
//! `a2`. `SYSCALL` does not rotate the register window, so the host reads the
//! current window's registers.

use core::arch::global_asm;

/// Terminate the run; `a3` = exit code. Never returns to the guest.
pub const SYS_EXIT: u32 = 1;
/// Write bytes to host-collected output; `a3` = ptr, `a4` = len.
pub const SYS_WRITE: u32 = 2;
/// Report a panic message; `a3` = ptr, `a4` = len. Never returns to the guest.
pub const SYS_PANIC: u32 = 3;

// A tiny windowed wrapper instead of inline-asm register constraints: a CALL8
// into it moves the Rust arguments (caller a10..a13) into its a2..a5 via
// ENTRY's rotation — exactly the registers the ABI wants — and the host's
// result (written to a2) travels back through RETW as the callee return value.
global_asm!(
    ".section .text.lp_xt_guest_syscall,\"ax\"",
    ".align 4",
    ".global lp_xt_guest_syscall",
    "lp_xt_guest_syscall:",
    "entry a1, 32",
    "syscall",
    "retw.n",
);

extern "C" {
    /// See the `global_asm!` block above.
    fn lp_xt_guest_syscall(nr: u32, a: u32, b: u32, c: u32) -> u32;
}

/// Issue a syscall with up to three arguments; returns the host's result.
#[inline]
pub fn syscall3(nr: u32, a: u32, b: u32, c: u32) -> u32 {
    // SAFETY: the wrapper is a well-formed windowed function; the host either
    // resumes us with a result in a2 or terminates the run. No guest memory
    // is written by the host for these calls.
    unsafe { lp_xt_guest_syscall(nr, a, b, c) }
}

/// Write `bytes` to the host-collected output stream.
pub fn sys_write(bytes: &[u8]) -> u32 {
    if bytes.is_empty() {
        return 0;
    }
    syscall3(SYS_WRITE, bytes.as_ptr() as u32, bytes.len() as u32, 0)
}

/// Exit the run with `code`.
pub fn exit(code: u32) -> ! {
    syscall3(SYS_EXIT, code, 0, 0);
    // The host never resumes an exit; loop as the `-> !` backstop.
    #[allow(clippy::empty_loop)]
    loop {}
}
