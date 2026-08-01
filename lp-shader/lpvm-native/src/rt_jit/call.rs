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
/// `any(target_arch = "riscv32", target_arch = "xtensa")` (see `lib.rs`): the
/// body is RV32 inline assembly, so the Xtensa entry-call is the sibling
/// [`xtensa_call8_args`] below rather than a widening of this one.
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

/// Windowed call into a JIT entry — the Xtensa sibling of
/// [`rv32_jalr_a0_a7`].
///
/// On Xtensa the emitter's CALL8 windowed convention **is** the platform C
/// ABI, so no inline assembly is needed: transmuting to `extern "C"` makes
/// the compiler emit the CALL8/CALLX8 sequence itself (the exact pattern the
/// experiment repo's xt-runner used to call payloads on silicon). Positional
/// args 0..=5 arrive in the callee's `a2..a7`; args 6..=7 go to the caller's
/// outgoing stack area; a `u64` return reads callee `a2`/`a3` — all matching
/// `isa/xt/abi.rs`'s classification.
///
/// # Safety
/// `entry` must be the **execute** address of valid windowed Xtensa code
/// obeying the CALL8 convention (always [`crate::rt_jit::JitBuffer::exec_ptr`]).
#[cfg(target_arch = "xtensa")]
#[allow(
    clippy::too_many_arguments,
    reason = "mirrors the rv32 sibling's 8-slot shape"
)]
pub(crate) unsafe fn xtensa_call8_args(
    entry: usize,
    a0: i32,
    a1: i32,
    a2: i32,
    a3: i32,
    a4: i32,
    a5: i32,
    a6: i32,
    a7: i32,
) -> (i32, i32) {
    let f: extern "C" fn(i32, i32, i32, i32, i32, i32, i32, i32) -> u64 =
        unsafe { core::mem::transmute(entry) };
    let r = f(a0, a1, a2, a3, a4, a5, a6, a7);
    (r as u32 as i32, (r >> 32) as u32 as i32)
}
