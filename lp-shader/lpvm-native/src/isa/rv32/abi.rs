//! RV32 ILP32 ABI constants and classification for [`crate::abi`].

use alloc::vec::Vec;

use lpir::IrFunction;
use lps_shared::LpsFnSig;

use crate::abi::classify::{ArgLoc, ReturnMethod, ir_type_scalar_words, scalar_count_of_type};
use crate::abi::{PReg, PregSet, RegClass};

// --- Named integer registers (x0–x31) ---

pub const ZERO: PReg = PReg {
    hw: 0,
    class: RegClass::Int,
};
pub const RA: PReg = PReg {
    hw: 1,
    class: RegClass::Int,
};
pub const SP: PReg = PReg {
    hw: 2,
    class: RegClass::Int,
};
pub const T0: PReg = PReg {
    hw: 5,
    class: RegClass::Int,
};
pub const T1: PReg = PReg {
    hw: 6,
    class: RegClass::Int,
};
pub const T2: PReg = PReg {
    hw: 7,
    class: RegClass::Int,
};
pub const S0: PReg = PReg {
    hw: 8,
    class: RegClass::Int,
};
pub const S1: PReg = PReg {
    hw: 9,
    class: RegClass::Int,
};
pub const A0: PReg = PReg {
    hw: 10,
    class: RegClass::Int,
};
pub const A1: PReg = PReg {
    hw: 11,
    class: RegClass::Int,
};
pub const A2: PReg = PReg {
    hw: 12,
    class: RegClass::Int,
};
pub const A3: PReg = PReg {
    hw: 13,
    class: RegClass::Int,
};
pub const A4: PReg = PReg {
    hw: 14,
    class: RegClass::Int,
};
pub const A5: PReg = PReg {
    hw: 15,
    class: RegClass::Int,
};
pub const A6: PReg = PReg {
    hw: 16,
    class: RegClass::Int,
};
pub const A7: PReg = PReg {
    hw: 17,
    class: RegClass::Int,
};
pub const S2: PReg = PReg {
    hw: 18,
    class: RegClass::Int,
};
pub const S3: PReg = PReg {
    hw: 19,
    class: RegClass::Int,
};
pub const S4: PReg = PReg {
    hw: 20,
    class: RegClass::Int,
};
pub const S5: PReg = PReg {
    hw: 21,
    class: RegClass::Int,
};
pub const S6: PReg = PReg {
    hw: 22,
    class: RegClass::Int,
};
pub const S7: PReg = PReg {
    hw: 23,
    class: RegClass::Int,
};
pub const S8: PReg = PReg {
    hw: 24,
    class: RegClass::Int,
};
pub const S9: PReg = PReg {
    hw: 25,
    class: RegClass::Int,
};
pub const S10: PReg = PReg {
    hw: 26,
    class: RegClass::Int,
};
pub const S11: PReg = PReg {
    hw: 27,
    class: RegClass::Int,
};
pub const T3: PReg = PReg {
    hw: 28,
    class: RegClass::Int,
};
pub const T4: PReg = PReg {
    hw: 29,
    class: RegClass::Int,
};
pub const T5: PReg = PReg {
    hw: 30,
    class: RegClass::Int,
};
pub const T6: PReg = PReg {
    hw: 31,
    class: RegClass::Int,
};

pub const ARG_REGS: [PReg; 8] = [A0, A1, A2, A3, A4, A5, A6, A7];
pub const RET_REGS: [PReg; 2] = [A0, A1];

pub const SPILL_TEMPS: [PReg; 2] = [T0, T1];

/// Registers the allocator may use for non-parameter values (integers only for now).
/// Excludes: zero, ra, sp, fp (s0), emitter scratch (t0–t2), argument registers (a0–a7).
/// Includes: s1–s11, t3–t6. For sret functions, [`crate::abi::FuncAbi`] removes `s1`.
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

/// Caller-saved integer GPRs used for clobber sets: a0–a7, t0–t6.
pub fn caller_saved_int() -> PregSet {
    PregSet::from_bits(int_mask(&[
        A0, A1, A2, A3, A4, A5, A6, A7, T0, T1, T2, T3, T4, T5, T6,
    ]))
}

/// Callee-saved integer GPRs: s0–s11.
pub fn callee_saved_int() -> PregSet {
    PregSet::from_bits(int_mask(&[
        S0, S1, S2, S3, S4, S5, S6, S7, S8, S9, S10, S11,
    ]))
}

/// Always reserved for special roles (not allocatable as general values).
/// t0–t2 are emitter scratch registers (TEMP0–TEMP2).
pub fn reserved_always_int() -> PregSet {
    PregSet::from_bits(int_mask(&[
        ZERO, RA, SP, T0, T1, T2, S0, A0, A1, A2, A3, A4, A5, A6, A7,
    ]))
}

/// Base allocatable int set before sret adjustment.
/// Excludes: zero, ra, sp, fp (s0), emitter scratch (t0–t2), argument registers (a0–a7).
pub fn alloca_base_int() -> PregSet {
    PregSet::from_bits(int_mask(&[
        S1, S2, S3, S4, S5, S6, S7, S8, S9, S10, S11, T3, T4, T5, T6,
    ]))
}

/// RV32: match Cranelift / `signature_for_ir_func`: more than two **scalar** return words use sret.
pub const SRET_SCALAR_THRESHOLD: usize = 2;

pub const STACK_ALIGNMENT: u32 = 16;

/// Flattened parameter locations: vmctx word first, then each scalar of each `FnParam` in order.
/// All LPIR functions (entry and non-entry) receive vmctx as the first argument.
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

/// Classify return value from the surface signature. RV32: more than two scalars ⇒ sret buffer.
///
/// M1 aggregate returns use [`IrFunction::sret_arg`] instead; see [`func_abi_rv32`].
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
            ptr_reg: A0,
            preserved_reg: S1,
            word_count: n as u32,
        },
    }
}

// `classify_params(sig, …)` is for [`func_abi_rv32`] without an [`IrFunction`] (tests / metadata).
// Normal compilation uses [`classify_params_for_compile`].

/// Parameter locations matching LPIR vreg order for a concrete [`IrFunction`].
///
/// When `func.sret_arg` is set (M1 aggregate return), incoming layout is
/// `a1=vmctx`, `a0=sret`, then user args from `a2` (see emitter prologue).
pub fn classify_params_for_compile(sig: &LpsFnSig, func: &IrFunction) -> Vec<ArgLoc> {
    if func.sret_arg.is_some() {
        let mut out = Vec::new();
        let mut reg_idx = 2usize;
        let mut stack_off = 0i32;
        out.push(ArgLoc::Reg(A1));
        out.push(ArgLoc::Reg(A0));
        for i in 0..func.param_count {
            let v = func.user_param_vreg(i);
            let ty = func.vreg_types[v.0 as usize];
            let n = ir_type_scalar_words(ty);
            push_scalar_words(&mut out, &mut reg_idx, &mut stack_off, n);
        }
        return out;
    }

    let legacy_sret_return = classify_return(sig).is_sret();
    let mut out = Vec::new();
    let mut reg_idx = if legacy_sret_return { 1usize } else { 0usize };
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

/// Build a `FuncAbi` using RV32 calling convention.
///
/// When `func` is `Some`, parameter layout follows that [`IrFunction`]'s vregs
/// (including M1 `sret_arg` hidden slot). Return classification uses
/// `func.sret_arg` when set, otherwise [`classify_return`] (scalar / mat / vec3+ heuristic).
///
/// When `func` is `None`, falls back to [`entry_param_scalar_count`] and flattened
/// [`LpsFnSig`] parameters (tests and signature-only metadata).
pub fn func_abi_rv32(sig: &LpsFnSig, func: Option<&IrFunction>) -> crate::abi::FuncAbi {
    use crate::abi::FuncAbi;
    use crate::abi::classify::entry_param_scalar_count;

    let return_method = match func {
        Some(f) if f.sret_arg.is_some() => {
            let n = scalar_count_of_type(&sig.return_type) as u32;
            ReturnMethod::Sret {
                ptr_reg: A0,
                preserved_reg: S1,
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
    if is_sret {
        allocatable.remove(S1);
    }

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
        caller_saved_int(),
        callee_saved_int(),
        crate::isa::IsaTarget::Rv32imac,
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

    #[test]
    fn void_return() {
        let sig = sig_with_params("f", LpsType::Void, &[]);
        assert!(matches!(classify_return(&sig), ReturnMethod::Void));
    }

    #[test]
    fn float_return_a0() {
        let sig = sig_with_params("f", LpsType::Float, &[]);
        match classify_return(&sig) {
            ReturnMethod::Direct { locs } => {
                assert_eq!(locs.len(), 1);
                assert_eq!(locs[0], ArgLoc::Reg(A0));
            }
            _ => panic!("expected Direct"),
        }
    }

    #[test]
    fn vec2_return_a0_a1() {
        let sig = sig_with_params("f", LpsType::Vec2, &[]);
        match classify_return(&sig) {
            ReturnMethod::Direct { locs } => {
                assert_eq!(locs.len(), 2);
                assert_eq!(locs[0], ArgLoc::Reg(A0));
                assert_eq!(locs[1], ArgLoc::Reg(A1));
            }
            _ => panic!("expected Direct"),
        }
    }

    #[test]
    fn vec3_return_is_sret_all_words() {
        let sig = sig_with_params("f", LpsType::Vec3, &[]);
        match classify_return(&sig) {
            ReturnMethod::Sret { word_count, .. } => assert_eq!(word_count, 3),
            _ => panic!("expected Sret"),
        }
    }

    #[test]
    fn vec4_return_is_sret() {
        let sig = sig_with_params("f", LpsType::Vec4, &[]);
        match classify_return(&sig) {
            ReturnMethod::Sret {
                word_count,
                ptr_reg,
                preserved_reg,
            } => {
                assert_eq!(word_count, 4);
                assert_eq!(ptr_reg, A0);
                assert_eq!(preserved_reg, S1);
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
        assert_eq!(locs[0], ArgLoc::Reg(A0));
        assert_eq!(locs[1], ArgLoc::Reg(A1));
        assert_eq!(locs[2], ArgLoc::Reg(A2));
    }

    #[test]
    fn params_sret_vmctx_in_a1() {
        let sig = sig_with_params("f", LpsType::Vec4, &[param("a", LpsType::Float)]);
        let locs = classify_params(&sig, true);
        assert_eq!(locs[0], ArgLoc::Reg(A1));
        assert_eq!(locs[1], ArgLoc::Reg(A2));
    }

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
        // vmctx @ a0; then a1–a7 hold seven more scalars; eighth scalar spills.
        for i in 0..7 {
            assert!(
                matches!(locs[1 + i], ArgLoc::Reg(_)),
                "expected reg for word {i}",
            );
        }
        assert!(matches!(locs[8], ArgLoc::Stack { .. }));
        assert!(matches!(locs[9], ArgLoc::Stack { .. }));
    }

    // --- Register set tests ---

    #[test]
    fn caller_saved_covers_a_and_t() {
        let s = caller_saved_int();
        assert!(s.contains(A0));
        assert!(s.contains(A7));
        assert!(s.contains(T0));
        assert!(s.contains(T6));
        assert!(!s.contains(S1));
    }

    #[test]
    fn callee_saved_covers_s_regs() {
        let s = callee_saved_int();
        assert!(s.contains(S0));
        assert!(s.contains(S11));
        assert!(!s.contains(A0));
    }

    #[test]
    fn reserved_covers_args_and_spill_temps() {
        let s = reserved_always_int();
        assert!(s.contains(ZERO));
        assert!(s.contains(RA));
        assert!(s.contains(SP));
        assert!(s.contains(S0));
        assert!(s.contains(T0));
        assert!(s.contains(T1));
        assert!(s.contains(A0));
        assert!(s.contains(A7));
    }

    #[test]
    fn alloca_base_excludes_reserved() {
        let a = alloca_base_int();
        assert!(a.contains(S1));
        assert!(a.contains(T3));
        assert!(!a.contains(ZERO));
        assert!(!a.contains(A0));
        assert!(!a.contains(T0));
        assert!(!a.contains(T2)); // emitter scratch (TEMP2)
        assert!(!a.contains(S0));
    }

    #[test]
    fn arg_regs_ordered() {
        assert_eq!(ARG_REGS[0], A0);
        assert_eq!(ARG_REGS[7], A7);
    }
}
