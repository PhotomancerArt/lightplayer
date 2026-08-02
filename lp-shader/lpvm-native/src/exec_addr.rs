//! The write→execute address rule: the single definition of how a byte stored
//! into a JIT buffer is named by the instruction fetch path.
//!
//! Emitted code is stored through the buffer's own address (the `Vec<u8>`'s).
//! On some targets the CPU is not allowed to fetch from that address, and the
//! same physical bytes are fetched through a different one:
//!
//! - RV32 (ESP32-C6) and host: **identity**. DRAM is executable at the address
//!   it is written at, so execute == write.
//! - Xtensa (ESP32-S3): **`exec = write + 0x6F_0000`** for writes inside
//!   SRAM1's dual-mapped window. Data writes go to the D-bus address; the CPU
//!   fetches the same bytes through the I-bus alias above it. Hardware-proven
//!   (spike E2), and visible in any S3 boot log — the linker places `.text` at
//!   I-bus `0x4037_8000`, which is D-bus `0x3FC8_8000` plus this offset.
//!
//! # Two callers, and why both must use it
//!
//! 1. [`crate::rt_jit::JitBuffer::exec_ptr`] — the *entry* addresses the
//!    runtime calls into the buffer with.
//! 2. [`crate::link::link_jit`] — the *intra-module call targets* it patches
//!    into the image. Those are addresses the emitted code itself jumps to, so
//!    they are execute addresses even though the linker writes them through
//!    the write-side mapping.
//!
//! Caller 2 was missed when the Xtensa backend landed (M3), and the bug is
//! invisible on RV32 because the rule is identity there. Left unfixed, any
//! shader whose function calls another function in the same module would
//! `callx8` to a D-bus address and fault with EXCCAUSE=2 on the first call —
//! `lp-xt-emu` models this exactly, refusing fetches at the D-bus view.
//!
//! Note this rule is about *storage placement*, not about the ISA being
//! emitted for: it is keyed on `target_arch` (where this code runs), not on
//! [`crate::IsaTarget`] (what it emits). A host cross-compiling an Xtensa
//! image is not executing it, so identity is the correct answer there.

/// ESP32-S3: the D-bus view of SRAM1's dual-mapped window. The esp-alloc heap
/// lives here, so heap-allocated JIT buffers land in it.
const S3_DUAL_MAPPED_DBUS: core::ops::Range<usize> = 0x3FC8_8000..0x3FCF_0000;

/// ESP32-S3: distance from the D-bus view of SRAM1 to its I-bus alias.
const S3_IBUS_ALIAS_OFFSET: usize = 0x6F_0000;

/// The ESP32-S3 rule as a pure function, so it is testable on any host.
///
/// Returns `None` when `write` is outside the dual-mapped window, which means
/// the caller must decide: an address that is already on the I-bus is fine,
/// anything else is a buffer-placement bug.
///
/// Split out from [`exec_addr`] so the arithmetic of a target-only rule can be
/// asserted from a host test run — without this the S3 arm would have no
/// coverage at all outside a hardware session.
#[must_use]
#[cfg_attr(
    not(target_arch = "xtensa"),
    allow(
        dead_code,
        reason = "only `exec_addr`'s Xtensa arm calls this; on other targets it exists \
                  solely so the host test run can assert the S3 arithmetic"
    )
)]
pub(crate) const fn s3_exec_addr(write: usize) -> Option<usize> {
    if write >= S3_DUAL_MAPPED_DBUS.start && write < S3_DUAL_MAPPED_DBUS.end {
        Some(write + S3_IBUS_ALIAS_OFFSET)
    } else {
        None
    }
}

/// Execute address for code stored at write address `write`.
///
/// Identity on every target whose DRAM is executable where it is written.
#[cfg(not(target_arch = "xtensa"))]
#[must_use]
pub(crate) fn exec_addr(write: usize) -> usize {
    write
}

/// Xtensa sibling of [`exec_addr`] — the **in-place** (heap-backed) rule,
/// which among Xtensa chips only the ESP32-S3 has:
///
/// - **ESP32-S3**: the dual-mapped window above.
/// - **Classic ESP32**: has NO in-place rule at all — the heap is not
///   executable and SRAM1's alias is word-mirrored, so no address offset can
///   make a heap buffer fetchable. Classic JIT code takes the *placed* path
///   instead ([`crate::codemem_esp32`] + `JitBuffer::Placed` +
///   [`crate::link::link_jit_at`]) and never consults this function. If
///   classic code DOES land here, it is a wiring bug (an in-place buffer on
///   a chip that cannot execute one); the fall-through arm's debug assert
///   catches the heap case, and I-bus addresses (≥ `0x4000_0000`) pass
///   through as identity, which is correct on every Xtensa chip.
#[cfg(target_arch = "xtensa")]
#[must_use]
pub(crate) fn exec_addr(write: usize) -> usize {
    match s3_exec_addr(write) {
        Some(exec) => exec,
        None => {
            debug_assert!(
                write >= 0x4000_0000,
                "JIT code at {write:#x} is neither in a known dual-mapped D-bus window nor an I-bus address"
            );
            write
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn s3_window_maps_to_the_ibus_alias() {
        // The window's base is the pair every S3 boot log shows: the linker
        // places .text at I-bus 0x4037_8000, which is this D-bus base + the
        // alias offset.
        assert_eq!(s3_exec_addr(0x3FC8_8000), Some(0x4037_8000));
        assert_eq!(s3_exec_addr(0x3FC9_0000), Some(0x4038_0000));
        // Last mapped byte.
        assert_eq!(s3_exec_addr(0x3FCE_FFFF), Some(0x403D_FFFF));
    }

    #[test]
    fn addresses_outside_the_window_are_not_translated() {
        // Below the window (S3 DRAM that is not dual-mapped).
        assert_eq!(s3_exec_addr(0x3FC8_7FFF), None);
        // The exclusive end.
        assert_eq!(s3_exec_addr(0x3FCF_0000), None);
        // Already an I-bus address.
        assert_eq!(s3_exec_addr(0x4037_8000), None);
    }

    #[test]
    fn translation_is_offset_preserving() {
        // Callers translate a base and then index into it, so the rule must
        // not distort distances within the window.
        let base = 0x3FC9_0000;
        let a = s3_exec_addr(base).unwrap();
        let b = s3_exec_addr(base + 0x40).unwrap();
        assert_eq!(b - a, 0x40);
    }

    #[test]
    fn host_exec_addr_is_identity() {
        // Guards the cross-compilation case: a host linking an Xtensa image is
        // not executing it, so it must not apply the S3 rule.
        #[cfg(not(target_arch = "xtensa"))]
        assert_eq!(exec_addr(0x3FC9_0000), 0x3FC9_0000);
    }
}
