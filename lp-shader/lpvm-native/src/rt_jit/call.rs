//! Direct RV32 call into a JIT entry (register arguments only).

/// `jalr` to `entry` with `a0`–`a7` set; returns `(a0, a1)` after the call.
///
/// `entry` is an **execute** address — always [`crate::rt_jit::JitBuffer::exec_ptr`],
/// never the buffer's write address.
///
/// # Safety
/// `entry` must point at valid RISC-V code; the callee must obey the RISC-V calling convention.
///
/// This gate is deliberately **not** the JIT-capable-target set spelled
/// `any(target_arch = "riscv32")` (see `lib.rs`): the body is RV32 inline
/// assembly, so the Xtensa backport adds a sibling `#[cfg(target_arch =
/// "xtensa")]` entry-call next to it rather than widening this one.
#[cfg(target_arch = "riscv32")]
pub(crate) unsafe fn rv32_jalr_a0_a7(
    entry: usize,
    mut a0: i32,
    mut a1: i32,
    a2: i32,
    a3: i32,
    a4: i32,
    a5: i32,
    a6: i32,
    a7: i32,
) -> (i32, i32) {
    unsafe {
        core::arch::asm!(
            "jalr ra, t0, 0",
            in("t0") entry,
            inlateout("a0") a0,
            inlateout("a1") a1,
            in("a2") a2,
            in("a3") a3,
            in("a4") a4,
            in("a5") a5,
            in("a6") a6,
            in("a7") a7,
            lateout("ra") _,
            clobber_abi("C"),
        );
    }
    (a0, a1)
}
