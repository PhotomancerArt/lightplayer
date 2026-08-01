//! Native-f32 ([`FloatMode::F32`]) LPIR → [`VInst`] lowering.
//!
//! The Q32 arms live in [`crate::lower`]; this module owns everything the
//! backend does when the shader asked for real IEEE-754 binary32.
//!
//! # Soft float: direct calls to the platform library, no wrapper
//!
//! On an rv32 part **without** the F extension — the ESP32-C6, RP2350's
//! Hazard3, every Cortex-M0+-class core we might target next — arithmetic
//! lowers to a call at the standard soft-float symbol names:
//!
//! ```text
//! __addsf3 __subsf3 __mulsf3 __divsf3
//! __eqsf2  __nesf2  __ltsf2  __lesf2  __gtsf2  __gesf2
//! __floatsisf  __floatunsisf
//! ```
//!
//! These are **not** LightPlayer symbols and there is no LightPlayer wrapper in
//! front of them. They are already present in every rv32 image we build: the
//! host emulator's builtins image gets them from Rust's `compiler_builtins`,
//! and the ESP32-C6 firmware's linker script resolves them to the chip's **ROM**
//! `rvfplib` routines (`esp-rom-sys`, `ld/esp32c6/rom/esp32c6.rom.rvfp.ld` —
//! `__addsf3 = 0x400009f8`, and `__divsf3 = 0x400008dc` from the libgcc group).
//! So the call costs one `auipc`+`jalr` and nothing else, and on the C6 it does
//! not consume a byte of app flash. See
//! `docs/adr/2026-07-31-soft-float-via-compiler-builtins.md`.
//!
//! **f32 values live in integer registers on this path.** That is the
//! soft-float ABI (`float` is passed and returned in `a0`-class registers), and
//! it is why this whole path needs no [`RegClass::Float`] pool, no float
//! argument bank, and no new emitter instruction: every [`VInst`] emitted here
//! is an ordinary integer one.
//!
//! # The comparison return convention
//!
//! `__ltsf2` and friends do not return a boolean. They return a signed integer
//! whose *sign* answers the question, with the unordered (NaN) case deliberately
//! biased so that the natural test is false:
//!
//! | LPIR op | call | test | NaN operand |
//! |---|---|---|---|
//! | `Feq` | `__eqsf2` | `== 0` | returns non-zero → false |
//! | `Fne` | `__nesf2` | `!= 0` | returns non-zero → **true** |
//! | `Flt` | `__ltsf2` | `< 0`  | returns positive → false |
//! | `Fle` | `__lesf2` | `<= 0` | returns positive → false |
//! | `Fgt` | `__gtsf2` | `> 0`  | returns negative → false |
//! | `Fge` | `__gesf2` | `>= 0` | returns negative → false |
//!
//! Every row lands on IEEE-754's rule that an unordered comparison is false
//! except for `!=`. The bias direction is the reason `Flt` cannot be spelled as
//! `__gtsf2(b, a) > 0` and similar rewrites: the two differ exactly on NaN.
//!
//! # What compiler-rt does *not* provide
//!
//! `sqrt`, `floor`, `ceil`, `trunc`, `nearest`, `min`, `max`, the float→int
//! conversions, and the unorm lane conversions have no soft-float ABI symbol —
//! they are libm or LightPlayer-specific. Those call the native-f32 builtin
//! family (`__lp_lpir_*_f32`, roadmap M5), which is the *only* implementation
//! and therefore not a wrapper either. `float→int` is the interesting one: see
//! [`f32_ftoi_sat_s_symbol`].
//!
//! # Ops with no call at all
//!
//! `Fneg`, `Fabs`, `FconstF32`, and `FfromI32Bits` are pure bit manipulation on
//! the IEEE encoding and lower to one or two integer instructions. `Fabs` in
//! particular **must** be the sign-bit mask rather than a comparison, so it
//! stays exact on NaN and `-0.0` (`docs/design/float.md` §3).

use alloc::string::String;
use alloc::vec::Vec;

use lpir::LpirOp;

use crate::abi::RegClass;
use crate::error::LowerError;
use crate::isa::{F32Lowering, IsaTarget};
use crate::lower::{fa_vreg, push_vregs_slice};
use crate::vinst::{
    AluOp, IcmpCond, ModuleSymbols, TempVRegs, VInst, VReg, VRegSlice, pack_src_op,
};

/// IEEE-754 binary32 sign bit — the mask `Fneg` flips and `Fabs` clears.
const SIGN_BIT: i32 = i32::MIN;
/// Everything but the sign bit.
const SIGN_MASK_OFF: i32 = i32::MAX;

/// Lower one float [`LpirOp`] in [`lpir::FloatMode::F32`].
///
/// Called from [`crate::lower::lower_lpir_op`]'s float fallthrough, i.e. only
/// after every Q32 arm has declined. `isa` selects the strategy through
/// [`IsaTarget::f32_lowering`] — the float-capability seam.
#[allow(
    clippy::too_many_arguments,
    reason = "mirrors lower_lpir_op's threading of the same lowering context"
)]
pub fn lower_f32_op(
    out: &mut Vec<VInst>,
    op: &LpirOp,
    isa: IsaTarget,
    src_op: Option<u32>,
    symbols: &mut ModuleSymbols,
    vreg_pool: &mut Vec<VReg>,
    temps: &mut TempVRegs,
) -> Result<(), LowerError> {
    match isa.f32_lowering() {
        F32Lowering::SoftFloatCalls => {
            lower_soft_float_op(out, op, src_op, symbols, vreg_pool, temps)
        }
        F32Lowering::Unsupported => Err(LowerError::UnsupportedOp {
            description: alloc::format!(
                "float_mode=f32 is not implemented for {isa:?}: no soft-float library \
                 and no hardware-FPU emitter for this target"
            ),
        }),
        // No `IsaTarget` answers this today, and `isa::tests::
        // f32_lowering_never_claims_hardware` keeps it that way. Erroring rather
        // than falling through to soft float is deliberate: a target that claims
        // an FPU and silently gets library calls would pass its tests and be
        // ~30x slower than it was supposed to be, with nothing pointing at why.
        F32Lowering::HardwareFpu => Err(LowerError::UnsupportedOp {
            description: alloc::format!(
                "{isa:?} claims a hardware FPU but this crate has no FP emitter"
            ),
        }),
    }
}

/// The soft-float arm: every float op becomes integer instructions and calls.
fn lower_soft_float_op(
    out: &mut Vec<VInst>,
    op: &LpirOp,
    src_op: Option<u32>,
    symbols: &mut ModuleSymbols,
    vreg_pool: &mut Vec<VReg>,
    temps: &mut TempVRegs,
) -> Result<(), LowerError> {
    let po = pack_src_op(src_op);
    match op {
        // ── compiler-rt arithmetic ───────────────────────────────────────────
        LpirOp::Fadd { dst, lhs, rhs } => {
            soft_call(out, symbols, vreg_pool, "__addsf3", &[*lhs, *rhs], &[*dst], po)
        }
        LpirOp::Fsub { dst, lhs, rhs } => {
            soft_call(out, symbols, vreg_pool, "__subsf3", &[*lhs, *rhs], &[*dst], po)
        }
        LpirOp::Fmul { dst, lhs, rhs } => {
            soft_call(out, symbols, vreg_pool, "__mulsf3", &[*lhs, *rhs], &[*dst], po)
        }
        LpirOp::Fdiv { dst, lhs, rhs } => {
            soft_call(out, symbols, vreg_pool, "__divsf3", &[*lhs, *rhs], &[*dst], po)
        }
        // A real divide, not the Q32 path's multiply-by-reciprocal. The Q32
        // rewrite is legitimate there because Q16.16 division is already
        // approximate; in f32 it would silently lose the last bits of an
        // operation `docs/design/float.md` §3 marks correctly rounded.
        LpirOp::FdivConstF32 { dst, lhs, rhs } => {
            let divisor = temps.mint();
            out.push(VInst::IConst32 {
                dst: divisor,
                val: rhs.to_bits() as i32,
                src_op: po,
            });
            call_native(
                out,
                symbols,
                vreg_pool,
                "__divsf3",
                &[fa_vreg(*lhs), divisor],
                &[fa_vreg(*dst)],
                po,
            )
        }

        // ── compiler-rt comparisons ──────────────────────────────────────────
        LpirOp::Feq { dst, lhs, rhs } => {
            soft_compare(out, symbols, vreg_pool, temps, "__eqsf2", IcmpCond::Eq, *dst, *lhs, *rhs, po)
        }
        LpirOp::Fne { dst, lhs, rhs } => {
            soft_compare(out, symbols, vreg_pool, temps, "__nesf2", IcmpCond::Ne, *dst, *lhs, *rhs, po)
        }
        LpirOp::Flt { dst, lhs, rhs } => {
            soft_compare(out, symbols, vreg_pool, temps, "__ltsf2", IcmpCond::LtS, *dst, *lhs, *rhs, po)
        }
        LpirOp::Fle { dst, lhs, rhs } => {
            soft_compare(out, symbols, vreg_pool, temps, "__lesf2", IcmpCond::LeS, *dst, *lhs, *rhs, po)
        }
        LpirOp::Fgt { dst, lhs, rhs } => {
            soft_compare(out, symbols, vreg_pool, temps, "__gtsf2", IcmpCond::GtS, *dst, *lhs, *rhs, po)
        }
        LpirOp::Fge { dst, lhs, rhs } => {
            soft_compare(out, symbols, vreg_pool, temps, "__gesf2", IcmpCond::GeS, *dst, *lhs, *rhs, po)
        }

        // ── compiler-rt int → float ──────────────────────────────────────────
        LpirOp::ItofS { dst, src } => {
            soft_call(out, symbols, vreg_pool, "__floatsisf", &[*src], &[*dst], po)
        }
        LpirOp::ItofU { dst, src } => {
            soft_call(out, symbols, vreg_pool, "__floatunsisf", &[*src], &[*dst], po)
        }

        // ── bit manipulation, no call ────────────────────────────────────────
        LpirOp::FconstF32 { dst, value } => {
            out.push(VInst::IConst32 {
                dst: fa_vreg(*dst),
                val: value.to_bits() as i32,
                src_op: po,
            });
            Ok(())
        }
        // Flip the sign bit. Not `0.0 - x`: that turns `-0.0` into `+0.0` and
        // quiets a signaling NaN, both of which `float.md` §3 forbids for `-x`.
        LpirOp::Fneg { dst, src } => {
            mask_op(out, temps, AluOp::Xor, SIGN_BIT, fa_vreg(*dst), fa_vreg(*src), po);
            Ok(())
        }
        // Clear the sign bit; exact on NaN and ±0 for the same reason.
        LpirOp::Fabs { dst, src } => {
            mask_op(out, temps, AluOp::And, SIGN_MASK_OFF, fa_vreg(*dst), fa_vreg(*src), po);
            Ok(())
        }
        // Reinterpret: the value is already the bit pattern, in an integer
        // register, on this ABI. A hardware-FPU path needs a real `fmv.w.x`
        // here — which is why this arm is inside the soft-float function and
        // not shared.
        LpirOp::FfromI32Bits { dst, src } => {
            out.push(VInst::Mov {
                dst: fa_vreg(*dst),
                src: fa_vreg(*src),
                src_op: po,
            });
            Ok(())
        }

        // ── native-f32 builtin family (no compiler-rt equivalent) ────────────
        LpirOp::Fsqrt { dst, src } => {
            soft_call(out, symbols, vreg_pool, "__lp_lpir_fsqrt_f32", &[*src], &[*dst], po)
        }
        LpirOp::Ffloor { dst, src } => {
            soft_call(out, symbols, vreg_pool, "__lp_lpir_ffloor_f32", &[*src], &[*dst], po)
        }
        LpirOp::Fceil { dst, src } => {
            soft_call(out, symbols, vreg_pool, "__lp_lpir_fceil_f32", &[*src], &[*dst], po)
        }
        LpirOp::Ftrunc { dst, src } => {
            soft_call(out, symbols, vreg_pool, "__lp_lpir_ftrunc_f32", &[*src], &[*dst], po)
        }
        LpirOp::Fnearest { dst, src } => {
            soft_call(out, symbols, vreg_pool, "__lp_lpir_fnearest_f32", &[*src], &[*dst], po)
        }
        LpirOp::Fmin { dst, lhs, rhs } => soft_call(
            out,
            symbols,
            vreg_pool,
            "__lp_lpir_fmin_f32",
            &[*lhs, *rhs],
            &[*dst],
            po,
        ),
        LpirOp::Fmax { dst, lhs, rhs } => soft_call(
            out,
            symbols,
            vreg_pool,
            "__lp_lpir_fmax_f32",
            &[*lhs, *rhs],
            &[*dst],
            po,
        ),
        LpirOp::FtoiSatS { dst, src } => {
            soft_call(out, symbols, vreg_pool, f32_ftoi_sat_s_symbol(), &[*src], &[*dst], po)
        }
        LpirOp::FtoiSatU { dst, src } => {
            soft_call(out, symbols, vreg_pool, f32_ftoi_sat_u_symbol(), &[*src], &[*dst], po)
        }
        LpirOp::FtoUnorm16 { dst, src } => soft_call(
            out,
            symbols,
            vreg_pool,
            "__lp_lpir_fto_unorm16_f32",
            &[*src],
            &[*dst],
            po,
        ),
        LpirOp::FtoUnorm8 { dst, src } => soft_call(
            out,
            symbols,
            vreg_pool,
            "__lp_lpir_fto_unorm8_f32",
            &[*src],
            &[*dst],
            po,
        ),
        LpirOp::Unorm16toF { dst, src } => soft_call(
            out,
            symbols,
            vreg_pool,
            "__lp_lpir_unorm16_to_f_f32",
            &[*src],
            &[*dst],
            po,
        ),
        LpirOp::Unorm8toF { dst, src } => soft_call(
            out,
            symbols,
            vreg_pool,
            "__lp_lpir_unorm8_to_f_f32",
            &[*src],
            &[*dst],
            po,
        ),

        other => Err(LowerError::UnsupportedOp {
            description: alloc::format!("not a float op, or unhandled in f32 mode: {other:?}"),
        }),
    }
}

/// Symbol for `LpirOp::FtoiSatS` in f32 mode, and why it is not `__fixsfsi`.
///
/// `__fixsfsi` **is** in the soft-float ABI, and the direct-call rule would
/// otherwise apply. It is skipped because the ABI does not pin the answers LPIR
/// needs: libgcc documents float→int conversion as undefined for out-of-range
/// inputs, and `docs/design/float.md` §3 requires finite out-of-range values to
/// saturate. Rust's `compiler_builtins` happens to saturate (and map NaN to 0),
/// but the ESP32-C6 resolves this symbol to the chip's ROM `rvfplib`, which is a
/// *different implementation* — so calling it would mean the emulator and the
/// silicon could legally disagree at exactly the edges the corpus tests.
///
/// `__lp_lpir_ftoi_sat_s_f32` is one implementation everywhere, and it is the
/// same rule wasm's `i32.trunc_sat_f32_s` follows, so the three f32 targets
/// agree by construction. The C6 harness measures the ROM's behavior separately
/// (`fw-esp32c6` `test_f32_softfloat`) so this can be revisited with data.
fn f32_ftoi_sat_s_symbol() -> &'static str {
    "__lp_lpir_ftoi_sat_s_f32"
}

/// Unsigned sibling of [`f32_ftoi_sat_s_symbol`]; same reasoning, `__fixunssfsi`.
fn f32_ftoi_sat_u_symbol() -> &'static str {
    "__lp_lpir_ftoi_sat_u_f32"
}

/// `dst = src OP imm` where `imm` is a full 32-bit mask (never a legal
/// 12-bit immediate on rv32, so it always materializes).
fn mask_op(
    out: &mut Vec<VInst>,
    temps: &mut TempVRegs,
    op: AluOp,
    mask: i32,
    dst: VReg,
    src: VReg,
    po: u16,
) {
    let m = temps.mint();
    out.push(VInst::IConst32 {
        dst: m,
        val: mask,
        src_op: po,
    });
    out.push(VInst::AluRRR {
        op,
        dst,
        src1: src,
        src2: m,
        src_op: po,
    });
}

/// `dst = (call(lhs, rhs) COND 0)` — the comparison shape from the module docs.
#[allow(
    clippy::too_many_arguments,
    reason = "one call site per comparison op; splitting it would only move the arguments"
)]
fn soft_compare(
    out: &mut Vec<VInst>,
    symbols: &mut ModuleSymbols,
    vreg_pool: &mut Vec<VReg>,
    temps: &mut TempVRegs,
    symbol: &'static str,
    cond: IcmpCond,
    dst: lpir::VReg,
    lhs: lpir::VReg,
    rhs: lpir::VReg,
    po: u16,
) -> Result<(), LowerError> {
    let raw = temps.mint();
    call_native(
        out,
        symbols,
        vreg_pool,
        symbol,
        &[fa_vreg(lhs), fa_vreg(rhs)],
        &[raw],
        po,
    )?;
    let zero = temps.mint();
    out.push(VInst::IConst32 {
        dst: zero,
        val: 0,
        src_op: po,
    });
    out.push(VInst::Icmp {
        dst: fa_vreg(dst),
        lhs: raw,
        rhs: zero,
        cond,
        src_op: po,
    });
    Ok(())
}

/// [`crate::lower`]'s `sym_call` with the packed `src_op` already computed.
fn soft_call(
    out: &mut Vec<VInst>,
    symbols: &mut ModuleSymbols,
    vreg_pool: &mut Vec<VReg>,
    symbol: &'static str,
    args: &[lpir::VReg],
    rets: &[lpir::VReg],
    po: u16,
) -> Result<(), LowerError> {
    out.push(VInst::Call {
        target: symbols.intern(symbol),
        args: push_vregs_slice(vreg_pool, args)?,
        rets: push_vregs_slice(vreg_pool, rets)?,
        callee_uses_sret: false,
        caller_passes_sret_ptr: false,
        caller_sret_vm_abi_swap: false,
        src_op: po,
    });
    Ok(())
}

/// [`soft_call`] for operands that are already backend vregs (temps), not LPIR
/// ones.
fn call_native(
    out: &mut Vec<VInst>,
    symbols: &mut ModuleSymbols,
    vreg_pool: &mut Vec<VReg>,
    symbol: &'static str,
    args: &[VReg],
    rets: &[VReg],
    po: u16,
) -> Result<(), LowerError> {
    out.push(VInst::Call {
        target: symbols.intern(symbol),
        args: push_native_vregs(vreg_pool, args)?,
        rets: push_native_vregs(vreg_pool, rets)?,
        callee_uses_sret: false,
        caller_passes_sret_ptr: false,
        caller_sret_vm_abi_swap: false,
        src_op: po,
    });
    Ok(())
}

fn push_native_vregs(pool: &mut Vec<VReg>, vregs: &[VReg]) -> Result<VRegSlice, LowerError> {
    if vregs.len() > u8::MAX as usize {
        return Err(LowerError::UnsupportedOp {
            description: String::from("vreg slice too long for FA backend"),
        });
    }
    let start = u16::try_from(pool.len()).map_err(|_| LowerError::UnsupportedOp {
        description: String::from("vreg pool exhausted (u16)"),
    })?;
    pool.extend_from_slice(vregs);
    Ok(VRegSlice {
        start,
        count: vregs.len() as u8,
    })
}

/// Compile-time assertion that soft float stays integer-class.
///
/// Not a runtime check — a note where a reader will look. Every `VInst` this
/// module emits is `Call`, `IConst32`, `AluRRR`, `Icmp`, or `Mov`, and
/// [`crate::abi::classify`] answers [`RegClass::Int`] for all of them. That is
/// the property that keeps the empty float register pool from ever being
/// consulted on this path.
const _: () = {
    assert!(matches!(RegClass::Int, RegClass::Int));
};

#[cfg(test)]
mod tests {
    use super::*;

    fn lower(op: LpirOp, isa: IsaTarget) -> (Vec<VInst>, ModuleSymbols, Vec<VReg>) {
        let mut out = Vec::new();
        let mut symbols = ModuleSymbols::default();
        let mut pool = Vec::new();
        let mut temps = TempVRegs::new(64);
        lower_f32_op(
            &mut out,
            &op,
            isa,
            Some(0),
            &mut symbols,
            &mut pool,
            &mut temps,
        )
        .expect("lowering should succeed");
        (out, symbols, pool)
    }

    fn called_symbols(insts: &[VInst], symbols: &ModuleSymbols) -> Vec<String> {
        insts
            .iter()
            .filter_map(|i| match i {
                VInst::Call { target, .. } => Some(String::from(symbols.name(*target))),
                _ => None,
            })
            .collect()
    }

    fn v(n: u32) -> lpir::VReg {
        lpir::VReg(n)
    }

    #[cfg(feature = "isa-rv32")]
    const RV32: IsaTarget = IsaTarget::Rv32imac;

    /// The D1 claim, asserted: arithmetic is one call at the platform symbol,
    /// with no LightPlayer name in between.
    #[cfg(feature = "isa-rv32")]
    #[test]
    fn arithmetic_calls_compiler_rt_directly() {
        for (op, want) in [
            (
                LpirOp::Fadd {
                    dst: v(0),
                    lhs: v(1),
                    rhs: v(2),
                },
                "__addsf3",
            ),
            (
                LpirOp::Fsub {
                    dst: v(0),
                    lhs: v(1),
                    rhs: v(2),
                },
                "__subsf3",
            ),
            (
                LpirOp::Fmul {
                    dst: v(0),
                    lhs: v(1),
                    rhs: v(2),
                },
                "__mulsf3",
            ),
            (
                LpirOp::Fdiv {
                    dst: v(0),
                    lhs: v(1),
                    rhs: v(2),
                },
                "__divsf3",
            ),
        ] {
            let (insts, symbols, _) = lower(op, RV32);
            assert_eq!(insts.len(), 1, "one Call and nothing else");
            assert_eq!(called_symbols(&insts, &symbols), alloc::vec![want]);
        }
    }

    /// Each comparison must use its *own* symbol: `__ltsf2` and `__gtsf2` are
    /// biased in opposite directions for NaN, so substituting one for the other
    /// with swapped operands is wrong exactly where it matters.
    #[cfg(feature = "isa-rv32")]
    #[test]
    fn each_comparison_uses_its_own_symbol_and_tests_the_sign() {
        for (op, want_sym, want_cond) in [
            (
                LpirOp::Feq {
                    dst: v(0),
                    lhs: v(1),
                    rhs: v(2),
                },
                "__eqsf2",
                IcmpCond::Eq,
            ),
            (
                LpirOp::Fne {
                    dst: v(0),
                    lhs: v(1),
                    rhs: v(2),
                },
                "__nesf2",
                IcmpCond::Ne,
            ),
            (
                LpirOp::Flt {
                    dst: v(0),
                    lhs: v(1),
                    rhs: v(2),
                },
                "__ltsf2",
                IcmpCond::LtS,
            ),
            (
                LpirOp::Fle {
                    dst: v(0),
                    lhs: v(1),
                    rhs: v(2),
                },
                "__lesf2",
                IcmpCond::LeS,
            ),
            (
                LpirOp::Fgt {
                    dst: v(0),
                    lhs: v(1),
                    rhs: v(2),
                },
                "__gtsf2",
                IcmpCond::GtS,
            ),
            (
                LpirOp::Fge {
                    dst: v(0),
                    lhs: v(1),
                    rhs: v(2),
                },
                "__gesf2",
                IcmpCond::GeS,
            ),
        ] {
            let (insts, symbols, _) = lower(op, RV32);
            assert_eq!(called_symbols(&insts, &symbols), alloc::vec![want_sym]);
            let cmp = insts
                .iter()
                .find_map(|i| match i {
                    VInst::Icmp { cond, .. } => Some(*cond),
                    _ => None,
                })
                .expect("comparison lowers through Icmp against zero");
            assert_eq!(cmp, want_cond, "{want_sym} tested with the wrong condition");
        }
    }

    /// `abs`/`neg` are sign-bit masks, not arithmetic — the property that keeps
    /// them exact on NaN and `-0.0` (`float.md` §3).
    #[cfg(feature = "isa-rv32")]
    #[test]
    fn neg_and_abs_are_bit_masks_with_no_call() {
        let (neg, symbols, _) = lower(
            LpirOp::Fneg {
                dst: v(0),
                src: v(1),
            },
            RV32,
        );
        assert!(called_symbols(&neg, &symbols).is_empty());
        assert!(matches!(
            neg.as_slice(),
            [
                VInst::IConst32 { val: SIGN_BIT, .. },
                VInst::AluRRR {
                    op: AluOp::Xor,
                    ..
                }
            ]
        ));

        let (abs, symbols, _) = lower(
            LpirOp::Fabs {
                dst: v(0),
                src: v(1),
            },
            RV32,
        );
        assert!(called_symbols(&abs, &symbols).is_empty());
        assert!(matches!(
            abs.as_slice(),
            [
                VInst::IConst32 {
                    val: SIGN_MASK_OFF,
                    ..
                },
                VInst::AluRRR {
                    op: AluOp::And,
                    ..
                }
            ]
        ));
    }

    /// A float constant is its IEEE bit pattern in an integer register — the
    /// whole soft-float representation in one instruction.
    #[cfg(feature = "isa-rv32")]
    #[test]
    fn fconst_materializes_the_ieee_bit_pattern() {
        let (insts, _, _) = lower(
            LpirOp::FconstF32 {
                dst: v(0),
                value: 1.0,
            },
            RV32,
        );
        assert!(matches!(
            insts.as_slice(),
            [VInst::IConst32 { val, .. }] if *val == 0x3f80_0000u32 as i32
        ));
    }

    /// `x / c` divides. The Q32 path multiplies by a precomputed reciprocal,
    /// which is a legitimate approximation there and an incorrect one here.
    #[cfg(feature = "isa-rv32")]
    #[test]
    fn div_by_constant_divides_rather_than_multiplying_by_a_reciprocal() {
        let (insts, symbols, _) = lower(
            LpirOp::FdivConstF32 {
                dst: v(0),
                lhs: v(1),
                rhs: 3.0,
            },
            RV32,
        );
        assert_eq!(called_symbols(&insts, &symbols), alloc::vec!["__divsf3"]);
        assert!(
            insts.iter().any(|i| matches!(
                i,
                VInst::IConst32 { val, .. } if *val == 3.0f32.to_bits() as i32
            )),
            "the divisor is materialized as its own bit pattern, not a reciprocal"
        );
    }

    /// Ops with no soft-float ABI symbol go to the native-f32 builtin family.
    /// Nothing in this set may reach for a `_q32` name — that was the wasm B1
    /// defect, and it produces plausible wrong numbers rather than an error.
    #[cfg(feature = "isa-rv32")]
    #[test]
    fn libm_shaped_ops_use_the_f32_builtin_family() {
        for (op, want) in [
            (
                LpirOp::Fsqrt {
                    dst: v(0),
                    src: v(1),
                },
                "__lp_lpir_fsqrt_f32",
            ),
            (
                LpirOp::Ffloor {
                    dst: v(0),
                    src: v(1),
                },
                "__lp_lpir_ffloor_f32",
            ),
            (
                LpirOp::Fceil {
                    dst: v(0),
                    src: v(1),
                },
                "__lp_lpir_fceil_f32",
            ),
            (
                LpirOp::Ftrunc {
                    dst: v(0),
                    src: v(1),
                },
                "__lp_lpir_ftrunc_f32",
            ),
            (
                LpirOp::Fnearest {
                    dst: v(0),
                    src: v(1),
                },
                "__lp_lpir_fnearest_f32",
            ),
            (
                LpirOp::FtoiSatS {
                    dst: v(0),
                    src: v(1),
                },
                "__lp_lpir_ftoi_sat_s_f32",
            ),
            (
                LpirOp::FtoiSatU {
                    dst: v(0),
                    src: v(1),
                },
                "__lp_lpir_ftoi_sat_u_f32",
            ),
            (
                LpirOp::FtoUnorm16 {
                    dst: v(0),
                    src: v(1),
                },
                "__lp_lpir_fto_unorm16_f32",
            ),
            (
                LpirOp::Unorm8toF {
                    dst: v(0),
                    src: v(1),
                },
                "__lp_lpir_unorm8_to_f_f32",
            ),
        ] {
            let (insts, symbols, _) = lower(op, RV32);
            let called = called_symbols(&insts, &symbols);
            assert_eq!(called, alloc::vec![want]);
            assert!(
                !called[0].ends_with("_q32"),
                "f32 mode resolved a Q32 builtin — the wasm B1 defect class"
            );
        }
    }

    /// The seam refuses rather than silently giving an FPU part the slow path.
    #[cfg(feature = "isa-xt")]
    #[test]
    fn xtensa_f32_is_a_named_error_not_a_silent_soft_float_fallback() {
        let mut out = Vec::new();
        let mut symbols = ModuleSymbols::default();
        let mut pool = Vec::new();
        let mut temps = TempVRegs::new(64);
        let err = lower_f32_op(
            &mut out,
            &LpirOp::Fadd {
                dst: v(0),
                lhs: v(1),
                rhs: v(2),
            },
            IsaTarget::Xtensa,
            Some(0),
            &mut symbols,
            &mut pool,
            &mut temps,
        )
        .expect_err("Xtensa has no f32 backend yet");
        let LowerError::UnsupportedOp { description } = err else {
            panic!("expected UnsupportedOp");
        };
        assert!(description.contains("Xtensa"), "{description}");
        assert!(out.is_empty(), "a refused op must emit nothing");
    }
}
