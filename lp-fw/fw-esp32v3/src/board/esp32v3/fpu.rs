//! Arming coprocessor 0 — the FPU — for the context that runs compiled float
//! code.
//!
//! A port of `fw-esp32s3`'s module of the same name. The LX6 and LX7 FPUs are
//! the same programming interface, and the classic's conformance capture
//! (`lp-xt-emu/tests/fixtures/fp/captures/families-esp32v3.txt`) is the
//! evidence: 5 630/5 630 agreement with the S3-fitted predictions, and
//! byte-identical estimate ROMs.
//!
//! # Why the firmware owns this and `lpvm-native` does not
//!
//! Compiled shader code contains bare FP instructions and arms nothing; on a
//! core whose `CPENABLE` bit 0 is clear, the first one takes `EXCCAUSE=32`
//! (`Coprocessor0Disabled`). Enabling a coprocessor is a property of the
//! *execution context*, not of the code being executed, and the firmware is
//! what owns contexts here.
//!
//! # Why arm it at all, when the silicon arrives armed
//!
//! Measured on this board (classic ESP32 rev v3.1, MAC `30:76:f5:ec:f6:34`,
//! 2026-08-06): the conformance harness reported `cpenable before=0x000000ff`
//! — every coprocessor already enabled at first instruction, the same value the
//! desk S3 reports and with the same unpinned provenance. Neither esp-hal 1.1.1
//! nor xtensa-lx-rt contains a `wsr.cpenable`, so it presumably comes from ROM
//! or the second-stage bootloader. That is a measured fact about *this* boot
//! chain, not a guarantee, and two instructions remove the dependency.
//!
//! # ⚠️ Which core — the one thing that did not port from the S3
//!
//! `CPENABLE` is **per-core**, and unlike the S3 this chip runs two cores with
//! a deliberate split: the RMT refill ISR lives on the APP core
//! (`docs/adr/2026-08-04-rmt-isr-on-app-core.md`,
//! `output::rmt::shared_driver::app_core_main`), while the embassy executor,
//! the server loop and the shader run on the PRO core.
//!
//! This arms **the PRO core only**, because that is where compiled shader code
//! executes. The APP core's job is pushing bytes at wire timing — it runs no
//! LPVM code and needs no FPU. If that ever changes, the APP core needs its own
//! [`arm`] call inside `app_core_main` before the first float instruction; a
//! call made here would not reach it.
//!
//! # Read-modify-write, never a blind store
//!
//! [`arm`] ORs bit 0 in rather than storing `1`. A blind `movi a2, 1;
//! wsr.cpenable a2` would *disable* coprocessors 1–7 on a board that booted
//! with `0xff` — which is exactly what this board does boot with, so the
//! hazard is real here rather than theoretical.
//!
//! Assembled from mnemonics, no assembler source adapted from binutils, GCC or
//! QEMU (AGENTS.md license rule).

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
/// does **not** propagate to other contexts: the APP core has its own
/// `CPENABLE` and would need its own call.
pub fn arm() -> u32 {
    // SAFETY: `lp_fpu_arm_cpenable` is the `global_asm!` block above — a
    // windowed-ABI leaf that touches only a2/a3 (its own window) and the
    // `CPENABLE` special register. It cannot fault: `rsr`/`wsr.cpenable` and
    // `isync` are unprivileged on the LX6 and this firmware runs at ring 0.
    unsafe { lp_fpu_arm_cpenable() }
}
