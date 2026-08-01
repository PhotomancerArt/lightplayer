//! FR index helpers for Xtensa float emission (`f0`–`f15`) — the register
//! model for the ESP32-S3's Floating-Point Coprocessor.
//!
//! Sibling of [`gpr`](super::gpr), and deliberately shaped the same way, but
//! the two files describe very different hardware.
//!
//! # The FR file is flat
//!
//! `gpr.rs` spends most of its length on the register *window*: a call rotates
//! the address-register file, so the caller and the callee name the same
//! physical register differently, and `a2..a7` are preserved across a call for
//! free. **None of that has an FR analogue.** The Floating-Point Coprocessor
//! Option (Xtensa ISA Reference Manual §4.3.11, p. 67) adds a flat 16-entry
//! register file that `ENTRY`/`RETW` do not touch. So there is no caller view,
//! no callee view, and no free preservation.
//!
//! # No FR is callee-saved
//!
//! Measured, not assumed: M6-P4's static ABI probe over
//! `xtensa-esp32s3-elf-gcc 14.2.0` at `-O3` found that **every** FR is
//! call-clobbered — an FR live across a `call8` is spilled by the caller with
//! an integer `s32i.n` and reloaded with `lsi`, and no FR is saved or restored
//! by any callee. That toolchain compiles the `lps-builtins` f32 family this
//! backend calls, so its convention is the one we must interoperate with, not
//! one we get to pick.
//!
//! Two consequences the rest of the backend depends on:
//!
//! - [`CALLER_SAVED_POOL`] is the whole of [`ALLOC_POOL`], so the allocator
//!   evicts every live float across a call.
//! - There is **no FP callee-save frame region** (M7 D7). `FrameLayout::compute`
//!   and [`FRAME_TOP_RESERVED_BYTES`](super::abi::FRAME_TOP_RESERVED_BYTES) are
//!   unchanged by float support; float spills land in the existing spill region
//!   at the *bottom* of the frame, structurally distant from the
//!   window-overflow reservation at the top.
//!
//! # One FR is reserved as scratch
//!
//! `gpr.rs` holds back `a8`/`a9`; this file holds back exactly one FR,
//! [`SCRATCH`] (`f15`), leaving 15 allocatable.
//!
//! **This narrows M7 D8, and the reason is a demonstrated need, not caution.**
//! D8 was written against the spill/reload path, where it is right: an `ssi`
//! or `lsi` names the allocated FR directly and needs no third register, and
//! the two sequences that *do* need a scratch — the out-of-range spill offset
//! and the compare's 0/1 materialization — need an *address* register, which
//! `a8`/`a9` already are. What D8 did not account for is the **spilled def**.
//!
//! The register allocator can allocate an instruction's *destination* to
//! `Alloc::Stack` (`regalloc::walk::process_generic`: a def whose home register
//! was already freed by a later eviction in the backward walk, but which has a
//! spill slot). The emitter's contract for that case is to compute into a
//! scratch and then store it to the slot — that is what `def_vreg`/
//! `store_def_vreg` do, and for integers the scratch is `a8`. An FP
//! instruction has to write *somewhere*, and there is no FR-free way to
//! produce a float result, so the float half needs the same affordance.
//!
//! **One is provably enough.** Only defs are ever stack-allocated: every *use*
//! goes through `regalloc::walk::alloc_use`, which returns `Alloc::Reg` on all
//! three of its paths (reloading through an inserted edit when the value lives
//! in a slot), and the one code path that forces uses to the stack is the sret
//! `Ret` constraint, which no float `VInst` reaches — float returns travel in
//! address registers (D1), so a float value reaches `Ret` only after an `Rfr`.
//! And no float `VInst` has two float defs. So at most one FR-sized
//! materialization is live at a time within a single emitted sequence.
//!
//! A future inline divide or sqrt sequence needing more scratch FRs carves
//! them out of [`ALLOC_POOL`] here, which remains the single place to do it.

/// Physical FR index (`f0`–`f15`).
pub type FReg = u8;

/// Number of floating-point registers the coprocessor provides.
pub const FR_COUNT: u8 = 16;

/// The emitter's float scratch (`f15`) — **not allocatable**.
///
/// The FR counterpart of [`gpr::SCRATCH`](super::gpr::SCRATCH) (`a8`), and it
/// exists for one reason: a float `VInst` whose destination the allocator put
/// on the stack must still compute into a register before it can be stored.
/// See this module's "One FR is reserved as scratch" section for why exactly
/// one suffices.
///
/// `f15` rather than `f0` so the low-numbered registers — the ones a
/// disassembly listing shows first — stay allocatable, matching `gpr.rs`'s
/// habit of reserving out of the way of the program bank.
pub const SCRATCH: FReg = 15;

/// Registers available to the allocator — 15 of the 16, all but [`SCRATCH`].
///
/// Order is the LRU initialization order, high to low. There is no
/// caller-saved / callee-saved split to order around (every FR is clobbered by
/// a call), and no FR carries an ABI role, so the order carries no meaning
/// beyond determinism: high-first keeps the low-numbered registers free
/// longest, matching `gpr::ALLOC_POOL`'s habit and making the two files' dumps
/// read alike.
pub const ALLOC_POOL: &[FReg] = &[14, 13, 12, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1, 0];

/// Pool members clobbered by a call — **all of them**.
///
/// Not a conservative default: the esp toolchain preserves no FR across a
/// `call8` (M6-P4's measured probe), so a float value live across a call must
/// be spilled by the caller. Being wrong in the other direction — claiming an
/// FR survives when it does not — silently corrupts a live value across every
/// builtin call.
pub const CALLER_SAVED_POOL: &[FReg] = ALLOC_POOL;

/// The single Boolean register FP compares write (`b0`).
///
/// FP compares do not produce an address-register 0/1; they set a bit in the
/// Boolean register file (ISA RM §4.3.10, p. 65 — the Boolean Option is a
/// prerequisite of the FP Coprocessor Option). M7 uses one fixed BR as an
/// implicit scratch and materializes the result into an AR with `movt`/`movf`
/// (ISA RM pp. 471, 479) inside the same emitted sequence, so the allocator
/// never learns that Boolean registers exist (M7 D5, Q1).
///
/// **Invariant: no Boolean register is live across a `VInst` boundary.** That
/// is what makes a single fixed `b0` safe rather than a source of aliasing —
/// see the FP emitter's module doc.
pub const CMP_BREG: u8 = 0;

/// Name for debugging / text format.
pub fn reg_name(reg: FReg) -> &'static str {
    match reg {
        0 => "f0",
        1 => "f1",
        2 => "f2",
        3 => "f3",
        4 => "f4",
        5 => "f5",
        6 => "f6",
        7 => "f7",
        8 => "f8",
        9 => "f9",
        10 => "f10",
        11 => "f11",
        12 => "f12",
        13 => "f13",
        14 => "f14",
        15 => "f15",
        _ => "???",
    }
}

/// Parse a register name (`f0`–`f15`) to its physical index.
#[allow(
    clippy::result_unit_err,
    reason = "gpr.rs shape parity: same signature as isa/xt/gpr.rs::parse_reg"
)]
pub fn parse_reg(name: &str) -> Result<FReg, ()> {
    let digits = name.strip_prefix('f').ok_or(())?;
    // Reject `f00`, `f+1` and similar spellings that `parse` would accept or
    // that would not round-trip through `reg_name`.
    if digits.is_empty() || (digits.len() > 1 && digits.starts_with('0')) {
        return Err(());
    }
    match digits.parse::<u8>() {
        Ok(n) if n < FR_COUNT => Ok(n),
        _ => Err(()),
    }
}

#[inline]
pub fn pool_contains(r: FReg) -> bool {
    ALLOC_POOL.contains(&r)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reg_name_round_trips() {
        for i in 0..FR_COUNT {
            assert_eq!(parse_reg(reg_name(i)), Ok(i), "round trip failed for f{i}");
        }
    }

    #[test]
    fn parse_reg_rejects_non_registers() {
        for bad in ["f16", "f-1", "f", "a0", "", "f00", "F0"] {
            assert_eq!(parse_reg(bad), Err(()), "{bad} parsed as a register");
        }
    }

    /// Every FR except [`SCRATCH`], no duplicates. A *second* reservation
    /// creeping in later would show up here rather than as a mysteriously
    /// smaller pool — and the scratch's exclusion is the whole point: if `f15`
    /// were both allocatable and the emitter's materialization target, a
    /// spilled def would silently clobber a live value.
    #[test]
    fn every_fr_but_the_scratch_is_allocatable() {
        assert_eq!(ALLOC_POOL.len(), FR_COUNT as usize - 1);
        for r in 0..FR_COUNT {
            assert_eq!(
                pool_contains(r),
                r != SCRATCH,
                "f{r} is on the wrong side of the scratch reservation"
            );
        }
        for (i, &f) in ALLOC_POOL.iter().enumerate() {
            assert!(!ALLOC_POOL[i + 1..].contains(&f), "duplicate f{f}");
        }
    }

    /// The measured ABI fact, pinned: no FR survives a call, so the caller-saved
    /// set is the entire pool. If this ever narrows, the frame story in
    /// `abi.rs` (no FP callee-save region) narrows with it.
    #[test]
    fn every_fr_is_caller_saved() {
        assert_eq!(CALLER_SAVED_POOL, ALLOC_POOL);
    }

    /// `b0` is an implicit scratch, not an allocatable resource. It shares the
    /// numeric space with `f0`, which is exactly the confusion the separate
    /// constant and this assertion exist to prevent.
    #[test]
    fn cmp_breg_is_b0() {
        assert_eq!(CMP_BREG, 0);
    }
}
