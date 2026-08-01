//! Arming coprocessor 0 — the FPU — for the context that runs compiled float
//! code (M7 D6).
//!
//! # Why the firmware owns this and `lpvm-native` does not
//!
//! Compiled shader code contains bare FP instructions and arms nothing; on a
//! core whose `CPENABLE` bit 0 is clear, the first one takes `EXCCAUSE=32`
//! (`Coprocessor0Disabled`). Enabling a coprocessor is a property of the
//! *execution context*, not of the code being executed, and the firmware is
//! what owns contexts here. `lpvm-native` documents the requirement instead of
//! implementing it, and `lp-xt-emu` proves the failure mode: `Cpu::new()`
//! leaves `CPENABLE` clear deliberately, which is what
//! `xt_pipeline_f32.rs::unarmed_float_code_faults_with_a_coprocessor_trap`
//! asserts.
//!
//! # Why arm it at all, when the silicon arrives armed
//!
//! M6-P1 measured `CPENABLE == 0x000000ff` on this board at first instruction
//! — every coprocessor already enabled — and neither esp-hal 1.1.1 nor
//! xtensa-lx-rt 0.22 contains a `wsr.cpenable`, so the write presumably comes
//! from ROM or the second-stage bootloader. That is a measured fact about *this
//! boot chain*, not a guarantee from the architecture, and its provenance is
//! unpinned. Two instructions of defence cost nothing and remove the dependency.
//!
//! # Read-modify-write, never a blind store
//!
//! [`arm`] ORs bit 0 in rather than storing `1`. A blind `movi a2, 1;
//! wsr.cpenable a2` would *disable* coprocessors 1–7 on a board that booted
//! with `0xff` — a defensive measure that breaks something is worse than none.
//! (The emulator rig in `xt_pipeline_f32.rs` does store `1`, correctly: it
//! starts from a core where `CPENABLE` is 0 and nothing else wants a
//! coprocessor.)
//!
//! Assembled from mnemonics, no assembler source adapted from binutils, GCC or
//! QEMU (AGENTS.md license rule). The sibling sequence in
//! `tests/xt_fp_conformance.rs` stays as it is: that rig measures what the boot
//! chain left behind and then sets a known value, which is a different job from
//! this one.

use core::arch::global_asm;

// Windowed ABI: `entry` opens the frame, `retw` closes it, and a2 is both the
// first argument register and the return register. a2/a3 are scratch here
// because this function takes no arguments.
global_asm!(
    r#"
    .section .text.lp_fpu_arm_cpenable,"ax",@progbits
    .align 4
    .global lp_fpu_arm_cpenable
    .type lp_fpu_arm_cpenable,@function
lp_fpu_arm_cpenable:
    entry a1, 32
    rsr.cpenable a2
    movi a3, 1
    or a2, a2, a3
    wsr.cpenable a2
    isync
    rsr.cpenable a2
    retw
    .size lp_fpu_arm_cpenable, .-lp_fpu_arm_cpenable
"#
);

unsafe extern "C" {
    fn lp_fpu_arm_cpenable() -> u32;
}

/// Enable coprocessor 0 for the calling context, returning the resulting
/// `CPENABLE` so a caller can log it as evidence rather than assume it.
///
/// Idempotent, and safe to call on a core that is already armed — the write is
/// a read-modify-write that only ever sets bits.
///
/// Call this before any compiled `FloatMode::F32` code runs on the context. It
/// does **not** propagate to other contexts: a second core or a task with its
/// own `CPENABLE` needs its own call.
pub fn arm() -> u32 {
    // SAFETY: `lp_fpu_arm_cpenable` is the `global_asm!` block above — a
    // windowed-ABI leaf that touches only a2/a3 (its own window) and the
    // `CPENABLE` special register. It cannot fault: `rsr`/`wsr.cpenable` and
    // `isync` are unprivileged on the LX7 and this firmware runs at ring 0.
    unsafe { lp_fpu_arm_cpenable() }
}
