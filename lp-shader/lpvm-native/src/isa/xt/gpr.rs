//! GPR index helpers for Xtensa emission (`PReg` = `u8`, a0–a15) — the
//! hardware-validated register model for the ESP32-S3 / classic-ESP32 backend.
//!
//! Ported from the experiment repo's `xt-mini-emit/src/gpr.rs` (its docs are
//! the long-form record); pinned by the experiment ADR
//! `2026-07-28-xtensa-abi-contract.md` and the P1 call-increment study,
//! measured on S3 silicon and LX6-conformance-verified on classic ESP32.
//!
//! The one Xtensa-specific wrinkle rv32 does not have: a call *rotates* the
//! register window, so the caller and callee see different names for the same
//! physical registers. Every constant below states which view it is in. The
//! two views are linked by [`CALL_ROTATION`]:
//! `caller a[n + CALL_ROTATION]` == `callee a[n]`.

/// Physical GPR index (a0–a15).
pub type PReg = u8;

/// `a0` — return address (written by `CALLn`, mangled with the CALLINC bits;
/// `RETW` consumes it). Never allocatable.
pub const RA_REG: PReg = 0;

/// `a1` — stack pointer. `ENTRY a1, frame` derives the callee SP from the
/// caller's; stable for the whole frame. Never allocatable.
pub const SP_REG: PReg = 1;

/// Frame pointer: **Xtensa needs none — aliased to [`SP_REG`]**. `ENTRY`
/// establishes the frame in one instruction and `a1` is then invariant for
/// the frame's lifetime (frames are fixed-size; the emitter hard-errors past
/// `ENTRY`'s 32760-byte immediate rather than emitting the `movsp` idiom).
pub const FP_REG: PReg = SP_REG;

/// Window rotation of a `CALL8`, in registers: `caller a[n + 8]` is the same
/// physical register as `callee a[n]`.
pub const CALL_ROTATION: u8 = 8;

/// Incoming argument registers, **callee view**: parameters arrive in the
/// callee's `a2..=a7` (staged by the caller at `a10..=a15` =
/// [`OUT_ARG_REGS`] pre-rotation). This is the view register allocation uses
/// (the windowed ABI caps register args at 6 for every increment — measured).
pub const ARG_REGS: [PReg; 6] = [2, 3, 4, 5, 6, 7];

/// Outgoing argument staging registers, **caller view**: the emitter writes
/// call arguments to its `a10..=a15`, which the callee's `ENTRY` rotates into
/// its `a2..=a7`. Disjoint from [`ARG_REGS`] — CALL8 staging never aliases
/// the preserved bank, so argument moves need no parallel-move resolution.
pub const OUT_ARG_REGS: [PReg; 6] = [10, 11, 12, 13, 14, 15];

/// Return-value registers, **callee view**: `Ret` writes `a2` (`a3` for a
/// second word).
pub const RET_REGS: [PReg; 2] = [2, 3];

/// Return-value registers, **caller view**: after the call returns, the
/// callee's `a2, a3` are the caller's `a10, a11`. This is where the emitter
/// reads a call's result.
pub const CALL_RET_REGS: [PReg; 2] = [10, 11];

/// Primary emitter scratch (`a8`) for lowering sequences — icmp/select
/// staging, out-of-range address arithmetic, `callx8` targets. NOT in
/// [`ALLOC_POOL`]. `a8`/`a9` are the only caller-saved registers that are
/// not argument staging — `a8` is where `CALL8` writes the mangled return
/// address, and `a9` falls in the same dead zone below the staging area, so
/// nothing can be live there across a call anyway. Zero-cost scratch.
pub const SCRATCH: PReg = 8;

/// Secondary emitter scratch (`a9`); same rationale as [`SCRATCH`].
pub const SCRATCH2: PReg = 9;

/// Registers available to the allocator for temporaries — **12** (vs rv32's
/// 13: near parity, the windowed file does not halve the pool).
///
/// Everything except a0/a1 (RA/SP) and a8/a9 (emitter scratch). Unlike rv32,
/// the incoming-argument registers ARE in the pool: rv32's arg registers
/// double as every call's outgoing staging area, but under CALL8 the staging
/// is the *separate* caller-saved bank `a10..=a15`, and the callee's
/// `a2..=a7` are, after the precolored parameters die, ordinary
/// call-preserved temporaries (preserved FREE by the rotation).
///
/// Order = LRU initialization order: caller-saved first, `a15` down to `a10`
/// (outgoing args stage upward from `a10`; handing out `a15` first keeps the
/// low staging slots free longest), then the preserved bank `a7` down to
/// `a2` (`a2`/`a3` are the return registers and first parameters — keeping
/// them free longest makes the pre-`retw` move a no-op more often).
pub const ALLOC_POOL: &[PReg] = &[15, 14, 13, 12, 11, 10, 7, 6, 5, 4, 3, 2];

/// Pool members clobbered by a call (measured on silicon: caller `a_j`
/// survives a CALL8 iff `j < 8`). Same order as their [`ALLOC_POOL`] prefix.
pub const CALLER_SAVED_POOL: &[PReg] = &[15, 14, 13, 12, 11, 10];

pub fn is_caller_saved_pool(r: PReg) -> bool {
    CALLER_SAVED_POOL.contains(&r)
}

/// Incoming (callee-view) argument register?
pub fn is_arg_reg(r: PReg) -> bool {
    (2..=7).contains(&r)
}

/// Outgoing (caller-view) argument staging register?
pub fn is_out_arg_reg(r: PReg) -> bool {
    (10..=15).contains(&r)
}

/// Call-preserved pool member (`a2..=a7` — survives CALL8 by rotation)?
/// The rv32 analogue is `is_callee_saved_pool_gpr`; here "callee-saved"
/// costs no prologue code.
#[inline]
pub fn is_callee_saved_pool(r: PReg) -> bool {
    (2..=7).contains(&r)
}

/// Parse register name to physical register number (standard Xtensa names:
/// `a0`–`a15`, plus the `sp` alias for `a1` accepted by gas).
#[allow(
    clippy::result_unit_err,
    reason = "rv32 shape parity: same signature as isa/rv32/gpr.rs"
)]
pub fn parse_reg(name: &str) -> Result<PReg, ()> {
    match name {
        "a0" => Ok(0),
        "a1" | "sp" => Ok(1),
        "a2" => Ok(2),
        "a3" => Ok(3),
        "a4" => Ok(4),
        "a5" => Ok(5),
        "a6" => Ok(6),
        "a7" => Ok(7),
        "a8" => Ok(8),
        "a9" => Ok(9),
        "a10" => Ok(10),
        "a11" => Ok(11),
        "a12" => Ok(12),
        "a13" => Ok(13),
        "a14" => Ok(14),
        "a15" => Ok(15),
        _ => Err(()),
    }
}

/// Name for debugging / text format (`aN` is the canonical spelling; even
/// `sp` disassembles as `a1`).
pub fn reg_name(reg: PReg) -> &'static str {
    match reg {
        0 => "a0",
        1 => "a1",
        2 => "a2",
        3 => "a3",
        4 => "a4",
        5 => "a5",
        6 => "a6",
        7 => "a7",
        8 => "a8",
        9 => "a9",
        10 => "a10",
        11 => "a11",
        12 => "a12",
        13 => "a13",
        14 => "a14",
        15 => "a15",
        _ => "???",
    }
}

#[inline]
pub fn pool_contains(r: PReg) -> bool {
    ALLOC_POOL.contains(&r)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_reg() {
        assert_eq!(parse_reg("a0"), Ok(0));
        assert_eq!(parse_reg("sp"), Ok(1));
        assert_eq!(parse_reg("a15"), Ok(15));
        assert_eq!(parse_reg("a16"), Err(()));
        assert_eq!(parse_reg("t0"), Err(()));
    }

    #[test]
    fn test_reg_name_roundtrip() {
        for i in 0..16u8 {
            let name = reg_name(i);
            assert_eq!(parse_reg(name), Ok(i), "Roundtrip failed for {i}");
        }
    }

    /// The caller-view constants are the callee-view constants shifted by the
    /// CALL8 window rotation — the classic silent-wrong-register trap.
    #[test]
    fn views_linked_by_call_rotation() {
        for i in 0..ARG_REGS.len() {
            assert_eq!(OUT_ARG_REGS[i], ARG_REGS[i] + CALL_ROTATION);
        }
        for i in 0..RET_REGS.len() {
            assert_eq!(CALL_RET_REGS[i], RET_REGS[i] + CALL_ROTATION);
        }
    }

    /// Pool size and membership: 12 registers (vs rv32's 13), excluding
    /// exactly RA, SP, and the two emitter scratches.
    #[test]
    fn pool_size_and_exclusions() {
        assert_eq!(ALLOC_POOL.len(), 12);
        assert!(!pool_contains(RA_REG));
        assert!(!pool_contains(SP_REG));
        assert!(!pool_contains(SCRATCH));
        assert!(!pool_contains(SCRATCH2));
        for r in 0..16u8 {
            let reserved = r == RA_REG || r == SP_REG || r == SCRATCH || r == SCRATCH2;
            assert_eq!(pool_contains(r), !reserved, "a{r}");
        }
        for (i, &a) in ALLOC_POOL.iter().enumerate() {
            assert!(!ALLOC_POOL[i + 1..].contains(&a), "duplicate a{a}");
        }
    }

    /// The caller-saved split matches the silicon measurement: `a_j`
    /// survives a CALL8 iff `j < 8`.
    #[test]
    fn caller_saved_matches_measured_survival() {
        for &r in ALLOC_POOL {
            let survives_call8 = r < 8;
            assert_eq!(is_caller_saved_pool(r), !survives_call8, "a{r}");
            assert_eq!(is_callee_saved_pool(r), survives_call8, "a{r}");
        }
        assert_eq!(&ALLOC_POOL[..CALLER_SAVED_POOL.len()], CALLER_SAVED_POOL);
    }

    #[test]
    fn arg_predicates() {
        for &r in &ARG_REGS {
            assert!(is_arg_reg(r));
            assert!(!is_out_arg_reg(r));
        }
        for &r in &OUT_ARG_REGS {
            assert!(is_out_arg_reg(r));
            assert!(!is_arg_reg(r));
        }
        assert!(!is_arg_reg(SCRATCH));
        assert!(!is_out_arg_reg(SCRATCH2));
    }
}
