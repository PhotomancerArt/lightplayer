//! The LED performance seam (M0 candidate L1): the render thread's wait
//! between two polls of the RMT driver's completion flag.
//!
//! # Why the body is what it is (K1)
//!
//! An empty `#[inline(never)]` function is exactly what LLVM deletes calls
//! to (it is side-effect-free), and two identical bodies are what rustc's
//! default `MergeFunctions` (aliases) or a linker's identical-code folding
//! merges into one address. So the body is one **non-`pure`** `asm!` (LLVM
//! must assume it has effects, so the call stays, inside the loop) holding
//! an instruction **distinct per seam**: `addi zero, zero, <id>`, an
//! architectural no-op (a write to `x0`) whose immediate is the seam id.
//! No other function in the image has that body, so nothing can fold it.
//!
//! On silicon it costs one call, one hint and one `ret` per spin iteration —
//! the spin only ever waits, so this changes how often it polls, not what
//! it waits for.

/// One wait step: call between two polls of `is_complete`.
#[inline(always)]
pub fn ws281x_wait_step() {
    #[cfg(not(feature = "spike_l0_wfi_wait"))]
    lp_seam_ws281x_wait_step();
    // L0 (spike only): `wfi` on real boards too, no seam. The emulator's idle
    // skip does the work; on silicon the RMT interrupt wakes the core.
    #[cfg(feature = "spike_l0_wfi_wait")]
    // SAFETY: `wfi` only waits for an interrupt; any pending or later enabled
    // interrupt (the RMT refill, its tx_end, a SYSTIMER tick) wakes it.
    unsafe {
        core::arch::asm!("wfi", options(nomem, nostack, preserves_flags));
    }
}

/// The seam function itself. The emulator patches its first instruction and,
/// when the seam is engaged, answers "sleep as `wfi` would, then return".
#[unsafe(no_mangle)]
#[inline(never)]
pub extern "C" fn lp_seam_ws281x_wait_step() {
    // SAFETY: writes x0 only — an architectural no-op. Not `pure`, so the
    // compiler keeps every call; the immediate makes the body unique.
    unsafe {
        core::arch::asm!(
            "addi zero, zero, {id}",
            id = const lp_seam::ws281x_wait_step::ID & 0x3ff,
            options(nomem, nostack, preserves_flags),
        );
    }
}

const _: lp_seam::ws281x_wait_step::Signature = lp_seam_ws281x_wait_step;
