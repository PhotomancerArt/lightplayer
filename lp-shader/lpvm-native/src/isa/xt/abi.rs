//! Xtensa windowed-ABI constants and classification for [`crate::abi`].
//!
//! Constants ported from the experiment repo's `xt-mini-emit/src/abi.rs`
//! (pinned by its ADR `2026-07-28-xtensa-abi-contract.md`, measured on S3
//! silicon, LX6-conformance-verified); the classification functions mirror
//! `isa/rv32/abi.rs` over the Xtensa register model.
//!
//! Views: [`ARG_REGS`]/[`RET_REGS`] are the **callee** view (what register
//! allocation precolors); the caller stages outgoing args at
//! `gpr::OUT_ARG_REGS` and reads call results at `gpr::CALL_RET_REGS`
//! (= callee view + `gpr::CALL_ROTATION`). See `gpr.rs`.

use alloc::vec::Vec;

use lpir::IrFunction;
use lps_shared::LpsFnSig;

use crate::abi::classify::{ArgLoc, ReturnMethod, ir_type_scalar_words, scalar_count_of_type};
use crate::abi::{PReg, PregSet, RegClass};

// --- Named address registers (a0–a15) ---

macro_rules! areg {
    ($name:ident, $hw:expr) => {
        pub const $name: PReg = PReg {
            hw: $hw,
            class: RegClass::Int,
        };
    };
}

areg!(A0, 0); // RA (windowed; CALLn writes, RETW consumes)
areg!(A1, 1); // SP (ENTRY-established; invariant per frame)
areg!(A2, 2);
areg!(A3, 3);
areg!(A4, 4);
areg!(A5, 5);
areg!(A6, 6);
areg!(A7, 7);
areg!(A8, 8); // emitter scratch
areg!(A9, 9); // emitter scratch
areg!(A10, 10);
areg!(A11, 11);
areg!(A12, 12);
areg!(A13, 13);
areg!(A14, 14);
areg!(A15, 15);

/// Incoming argument registers, callee view (windowed ABI caps register args
/// at 6 for every call increment — measured).
pub const ARG_REGS: [PReg; 6] = [A2, A3, A4, A5, A6, A7];
/// Direct-return registers, callee view.
pub const RET_REGS: [PReg; 2] = [A2, A3];

fn int_mask(regs: &[PReg]) -> u64 {
    let mut m = 0u64;
    for r in regs {
        if r.class == RegClass::Int {
            m |= 1u64 << r.hw;
        } else {
            m |= 1u64 << (32 + r.hw);
        }
    }
    m
}

/// Caller-saved GPRs for clobber sets: `a8..a15` (silicon-measured — caller
/// `a_j` survives a CALL8 iff `j < 8`; a8/a9 are also emitter scratch).
pub fn caller_saved_int() -> PregSet {
    PregSet::from_bits(int_mask(&[A8, A9, A10, A11, A12, A13, A14, A15]))
}

/// Call-preserved GPRs: exactly `a2..a7` (preserved by the window rotation —
/// no prologue save/restore cost). `a0`/`a1` are preserved too but always
/// reserved for RA/SP.
pub fn callee_saved_int() -> PregSet {
    PregSet::from_bits(int_mask(&[A2, A3, A4, A5, A6, A7]))
}

/// Always reserved for special roles: RA, SP, and the two emitter scratches.
/// Unlike rv32, the argument registers are NOT here — under CALL8 they are
/// ordinary call-preserved pool members once the precolored parameters die.
pub fn reserved_always_int() -> PregSet {
    PregSet::from_bits(int_mask(&[A0, A1, A8, A9]))
}

/// Base allocatable int set before sret adjustment: the 12-register pool
/// (`a2..a7` preserved bank + `a10..a15` caller-saved bank).
pub fn alloca_base_int() -> PregSet {
    PregSet::from_bits(int_mask(&[
        A2, A3, A4, A5, A6, A7, A10, A11, A12, A13, A14, A15,
    ]))
}

/// Caller-saved FRs for clobber sets — **all 16** (M6-P4: no FR survives a
/// `call8` under the esp toolchain that compiles our float builtins).
///
/// The `_float` sibling of [`caller_saved_int`]. There is deliberately no
/// `callee_saved_float`: the empty set has no callers, and writing one would
/// suggest an FP callee-save frame region exists. It does not (M7 D7) — which
/// is exactly why [`FRAME_TOP_RESERVED_BYTES`] and `FrameLayout::compute` are
/// unchanged by float support.
#[cfg(feature = "float-f32")]
pub fn caller_saved_float() -> PregSet {
    float_set(super::fpr::CALLER_SAVED_POOL)
}

/// Base allocatable float set: the whole FR file (M7 D8 reserves no scratch).
///
/// Unlike [`alloca_base_int`] there is no sret adjustment to make — the sret
/// pointer is an address, and addresses are never float.
#[cfg(feature = "float-f32")]
pub fn alloca_base_float() -> PregSet {
    float_set(super::fpr::ALLOC_POOL)
}

/// Build a [`PregSet`] over the float lanes from raw FR indices.
#[cfg(feature = "float-f32")]
fn float_set(regs: &[u8]) -> PregSet {
    let mut s = PregSet::EMPTY;
    for &r in regs {
        s.insert(PReg::float(r));
    }
    s
}

/// Direct-return width: more than 2 scalar return words go through an sret
/// buffer. Same value as rv32 deliberately — keeps LPIR-level return
/// classification target-invariant (the windowed ABI would permit 4, but
/// widening buys nothing LPIR needs and diverges shared filetests).
pub const SRET_SCALAR_THRESHOLD: usize = 2;

/// The Xtensa windowed ABI mandates 16-byte SP alignment (the 16-byte base
/// save area sits at `[SP-16, SP)` and assumes it). Same value as rv32 for
/// the ABI's own reasons — not copied.
pub const STACK_ALIGNMENT: u32 = 16;

/// Reserved bytes at the **top** of every frame for the windowed ABI's
/// register save areas — the ISA hook `abi/frame.rs::compute()` consumes
/// (rv32 = 0; Xtensa/CALL8 = 32 = `16 * units`).
///
/// The window overflow/underflow handlers write/read these bytes *unbidden*
/// whenever call depth exceeds the physical register file: the 16-byte base
/// save area (an ancestor's `a0..a3` at `[SP-16, SP)` of its callee) plus 16
/// bytes for the `a4..a7` group spilled by `_WindowOverflow8`. Hardware-
/// validated by the experiment's recursion torture corpus (68 dual-run cases,
/// depths 1..=100, address-level no-collision assertions). Getting this wrong
/// corrupts *ancestor* frames invisibly.
pub const FRAME_TOP_RESERVED_BYTES: u32 = 32;

/// Flattened parameter locations: vmctx word first, then each scalar of each
/// `FnParam` in order. Mirrors rv32's layout over 6 argument registers.
pub fn classify_params(sig: &LpsFnSig, is_sret: bool) -> Vec<ArgLoc> {
    let mut out = Vec::new();
    let mut reg_idx = if is_sret { 1usize } else { 0usize };
    let mut stack_off = 0i32;

    push_scalar_words(&mut out, &mut reg_idx, &mut stack_off, 1); // vmctx / pointer word

    for p in &sig.parameters {
        let n = scalar_count_of_type(&p.ty);
        push_scalar_words(&mut out, &mut reg_idx, &mut stack_off, n);
    }

    out
}

fn push_scalar_words(
    out: &mut Vec<ArgLoc>,
    reg_idx: &mut usize,
    stack_off: &mut i32,
    count: usize,
) {
    for _ in 0..count {
        if *reg_idx < ARG_REGS.len() {
            out.push(ArgLoc::Reg(ARG_REGS[*reg_idx]));
            *reg_idx += 1;
        } else {
            out.push(ArgLoc::Stack {
                offset: *stack_off,
                size: 4,
            });
            *stack_off += 4;
        }
    }
}

/// Classify return value from the surface signature: more than
/// [`SRET_SCALAR_THRESHOLD`] scalars ⇒ sret buffer.
///
/// sret registers: the pointer arrives as the **first argument** (callee
/// `a2`, matching the esp-toolchain oracle) and — unlike rv32, which must
/// move it from caller-saved `a0` into callee-saved `s1` — it can simply
/// STAY in `a2`: the preserved bank survives calls by rotation, so
/// `preserved_reg == ptr_reg` and the prologue move is a no-op. `FuncAbi`
/// removes `a2` from the allocatable pool for sret functions, exactly as
/// rv32 removes `s1`.
pub fn classify_return(sig: &LpsFnSig) -> ReturnMethod {
    let n = scalar_count_of_type(&sig.return_type);
    match n {
        0 => ReturnMethod::Void,
        1..=2 => {
            let mut locs = Vec::with_capacity(n);
            for i in 0..n {
                locs.push(ArgLoc::Reg(RET_REGS[i]));
            }
            ReturnMethod::Direct { locs }
        }
        _ => ReturnMethod::Sret {
            ptr_reg: A2,
            preserved_reg: A2,
            word_count: n as u32,
        },
    }
}

/// Parameter locations matching LPIR vreg order for a concrete [`IrFunction`].
///
/// When `func.sret_arg` is set (M1 aggregate return), incoming layout is
/// `a3=vmctx`, `a2=sret`, then user args from `a4` — the rotation image of
/// rv32's `a1=vmctx, a0=sret` (the caller staged `[sret→a10, vmctx→a11]`).
pub fn classify_params_for_compile(sig: &LpsFnSig, func: &IrFunction) -> Vec<ArgLoc> {
    if func.sret_arg.is_some() {
        let mut out = Vec::new();
        let mut reg_idx = 2usize;
        let mut stack_off = 0i32;
        out.push(ArgLoc::Reg(A3));
        out.push(ArgLoc::Reg(A2));
        for i in 0..func.param_count {
            let v = func.user_param_vreg(i);
            let ty = func.vreg_types[v.0 as usize];
            let n = ir_type_scalar_words(ty);
            push_scalar_words(&mut out, &mut reg_idx, &mut stack_off, n);
        }
        return out;
    }

    let implicit_scalar_sret = classify_return(sig).is_sret();
    let mut out = Vec::new();
    let mut reg_idx = if implicit_scalar_sret { 1usize } else { 0usize };
    let mut stack_off = 0i32;
    push_scalar_words(&mut out, &mut reg_idx, &mut stack_off, 1);
    for i in 0..func.param_count {
        let v = func.user_param_vreg(i);
        let ty = func.vreg_types[v.0 as usize];
        let n = ir_type_scalar_words(ty);
        push_scalar_words(&mut out, &mut reg_idx, &mut stack_off, n);
    }
    out
}

/// Build a `FuncAbi` using the Xtensa CALL8 windowed calling convention.
/// Mirrors [`crate::isa::rv32::abi::func_abi_rv32`] shape-for-shape.
pub fn func_abi_xt(sig: &LpsFnSig, func: Option<&IrFunction>) -> crate::abi::FuncAbi {
    use crate::abi::FuncAbi;
    use crate::abi::classify::entry_param_scalar_count;

    let return_method = match func {
        Some(f) if f.sret_arg.is_some() => {
            let n = scalar_count_of_type(&sig.return_type) as u32;
            ReturnMethod::Sret {
                ptr_reg: A2,
                preserved_reg: A2,
                word_count: n,
            }
        }
        _ => classify_return(sig),
    };
    let is_sret = return_method.is_sret();
    let param_locs = match func {
        Some(f) => classify_params_for_compile(sig, f),
        None => classify_params(sig, is_sret),
    };

    let mut allocatable = alloca_base_int();
    // The two classes are independent lanes of the same set, so the float file
    // joins the pool by union and needs no sret adjustment of its own — the
    // sret pointer is an address.
    #[cfg(feature = "float-f32")]
    {
        allocatable = allocatable.union(alloca_base_float());
    }
    if is_sret {
        // The sret pointer lives in a2 for the whole function.
        allocatable.remove(A2);
    }

    // Every FR is clobbered by a call (M6-P4). The clobber set is what makes
    // the allocator evict live floats around a call, so omitting the float
    // lanes here would leave a value in a register the callee overwrites.
    #[cfg(feature = "float-f32")]
    let caller_saved = caller_saved_int().union(caller_saved_float());
    #[cfg(not(feature = "float-f32"))]
    let caller_saved = caller_saved_int();

    let total_param_slots = match func {
        Some(f) => f.total_param_slots() as usize,
        None => entry_param_scalar_count(sig),
    };
    let precolors = build_precolors(&param_locs, total_param_slots);

    FuncAbi::new_raw(
        param_locs,
        return_method,
        allocatable,
        precolors,
        caller_saved,
        // No float lane here, deliberately: no FR is callee-saved, which is
        // what removes the FP callee-save frame region entirely (M7 D7).
        callee_saved_int(),
        crate::isa::IsaTarget::Xtensa,
    )
}

fn build_precolors(
    param_locs: &[crate::abi::classify::ArgLoc],
    total_param_slots: usize,
) -> alloc::vec::Vec<(u32, crate::abi::PReg)> {
    let n = total_param_slots.min(param_locs.len());
    let mut out = alloc::vec::Vec::with_capacity(n);
    for i in 0..n {
        if let ArgLoc::Reg(p) = param_locs[i] {
            out.push((i as u32, p));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use lps_shared::LpsType;

    use super::*;

    fn param(name: &str, ty: LpsType) -> lps_shared::FnParam {
        lps_shared::FnParam {
            name: name.into(),
            ty,
            qualifier: lps_shared::ParamQualifier::In,
        }
    }

    fn sig_with_params(name: &str, ret: LpsType, params: &[lps_shared::FnParam]) -> LpsFnSig {
        LpsFnSig {
            name: name.into(),
            return_type: ret,
            parameters: params.iter().cloned().collect(),
            kind: lps_shared::LpsFnKind::UserDefined,
        }
    }

    /// The frame reservation is what the CALL8 handler contract requires.
    #[test]
    fn frame_reservation_matches_policy() {
        assert_eq!(FRAME_TOP_RESERVED_BYTES, 32);
        assert_eq!(FRAME_TOP_RESERVED_BYTES % STACK_ALIGNMENT, 0);
    }

    /// Direct returns fit the return-register pairs in both views.
    #[test]
    fn sret_threshold_fits_ret_regs() {
        assert_eq!(SRET_SCALAR_THRESHOLD, RET_REGS.len());
        assert_eq!(
            SRET_SCALAR_THRESHOLD,
            super::super::gpr::CALL_RET_REGS.len()
        );
    }

    #[test]
    fn void_return() {
        let sig = sig_with_params("f", LpsType::Void, &[]);
        assert!(matches!(classify_return(&sig), ReturnMethod::Void));
    }

    #[test]
    fn float_return_a2() {
        let sig = sig_with_params("f", LpsType::Float, &[]);
        match classify_return(&sig) {
            ReturnMethod::Direct { locs } => {
                assert_eq!(locs.len(), 1);
                assert_eq!(locs[0], ArgLoc::Reg(A2));
            }
            _ => panic!("expected Direct"),
        }
    }

    #[test]
    fn vec2_return_a2_a3() {
        let sig = sig_with_params("f", LpsType::Vec2, &[]);
        match classify_return(&sig) {
            ReturnMethod::Direct { locs } => {
                assert_eq!(locs.len(), 2);
                assert_eq!(locs[0], ArgLoc::Reg(A2));
                assert_eq!(locs[1], ArgLoc::Reg(A3));
            }
            _ => panic!("expected Direct"),
        }
    }

    #[test]
    fn vec4_return_is_sret_ptr_stays_in_a2() {
        let sig = sig_with_params("f", LpsType::Vec4, &[]);
        match classify_return(&sig) {
            ReturnMethod::Sret {
                word_count,
                ptr_reg,
                preserved_reg,
            } => {
                assert_eq!(word_count, 4);
                assert_eq!(ptr_reg, A2);
                assert_eq!(preserved_reg, A2);
            }
            _ => panic!("expected Sret"),
        }
    }

    #[test]
    fn params_vmctx_then_user_no_sret() {
        let sig = sig_with_params(
            "f",
            LpsType::Void,
            &[param("a", LpsType::Float), param("b", LpsType::Float)],
        );
        let locs = classify_params(&sig, false);
        assert_eq!(locs.len(), 3);
        assert_eq!(locs[0], ArgLoc::Reg(A2));
        assert_eq!(locs[1], ArgLoc::Reg(A3));
        assert_eq!(locs[2], ArgLoc::Reg(A4));
    }

    #[test]
    fn params_sret_vmctx_in_a3() {
        let sig = sig_with_params("f", LpsType::Vec4, &[param("a", LpsType::Float)]);
        let locs = classify_params(&sig, true);
        assert_eq!(locs[0], ArgLoc::Reg(A3));
        assert_eq!(locs[1], ArgLoc::Reg(A4));
    }

    /// vmctx + 6 scalars fill a2..a7; everything after spills to the stack.
    #[test]
    fn params_spill_past_a7() {
        let sig = sig_with_params(
            "f",
            LpsType::Void,
            &[
                param("a", LpsType::Vec4),
                param("b", LpsType::Vec4),
                param("c", LpsType::Float),
            ],
        );
        let locs = classify_params(&sig, false);
        assert_eq!(locs.len(), 1 + 4 + 4 + 1);
        // vmctx @ a2; then a3–a7 hold five more scalars; the rest spill.
        for i in 0..6 {
            assert!(
                matches!(locs[i], ArgLoc::Reg(_)),
                "expected reg for word {i}"
            );
        }
        for (i, loc) in locs.iter().enumerate().skip(6) {
            assert!(
                matches!(loc, ArgLoc::Stack { .. }),
                "expected stack for word {i}"
            );
        }
    }

    #[test]
    fn caller_saved_is_a8_up() {
        let s = caller_saved_int();
        assert!(s.contains(A8));
        assert!(s.contains(A10));
        assert!(s.contains(A15));
        assert!(!s.contains(A2));
        assert!(!s.contains(A7));
    }

    #[test]
    fn callee_saved_is_the_preserved_bank() {
        let s = callee_saved_int();
        for r in [A2, A3, A4, A5, A6, A7] {
            assert!(s.contains(r));
        }
        assert!(!s.contains(A10));
    }

    #[test]
    fn alloca_base_is_the_12_reg_pool() {
        let a = alloca_base_int();
        for r in [A2, A3, A4, A5, A6, A7, A10, A11, A12, A13, A14, A15] {
            assert!(a.contains(r));
        }
        for r in [A0, A1, A8, A9] {
            assert!(!a.contains(r));
        }
    }
}
