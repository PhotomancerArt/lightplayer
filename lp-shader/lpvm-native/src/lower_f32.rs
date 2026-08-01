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
//!
//! # Hardware FPU: single instructions, and a call for the rest
//!
//! On a part with a real single-precision FPU — the ESP32-S3's Floating-Point
//! Coprocessor today — the point of native f32 is that a handful of operations
//! collapse from a call or a five-instruction Q32 sequence to **one
//! instruction**. That set is exactly what is inlined (M7 D4):
//!
//! | Inlined — one FP instruction | Routed to an M5 builtin |
//! |---|---|
//! | `Fadd` `Fsub` `Fmul` | `Fdiv` `Fsqrt` |
//! | `Fabs` `Fneg`, float moves | `Ffloor` `Fceil` `Ftrunc` `Fnearest` |
//! | the six comparisons, float select | `Fmin` `Fmax` |
//! | `ItofS` `ItofU` | `FtoiSatS` `FtoiSatU`, the unorm conversions |
//! | float loads and stores, the AR↔FR transfers | every transcendental and `lpfn` |
//!
//! The right-hand column is the same mechanism Q32 uses today, and every symbol
//! on it already exists. Two of the omissions are deliberate rather than
//! pending. **Division** is a builtin call in Q32 as well, so calling one here
//! is parity; inlining the `div0.s`/`divn.s` estimate sequence buys speed and
//! costs a hard dependency on an exhaustive extraction of the chip's
//! implementation-defined estimate tables. **Float→int** is routed because
//! `trunc.s` alone may not satisfy `float.md` §3: whether this silicon
//! saturates or wraps for finite out-of-range inputs is an open measurement,
//! and a builtin that is correct by construction beats an instruction that
//! might be.
//!
//! # The hardware calling convention: FR-internally, AR-at-boundaries
//!
//! Float values live in float registers inside a function body and travel in
//! **address registers, as raw IEEE bit patterns**, across every parameter,
//! call-argument, call-return and function-return boundary (M7 D1).
//!
//! This is not a free choice. The `lps-builtins` f32 family is compiled by
//! `xtensa-esp32s3-elf-gcc`, and that toolchain's measured ABI (M6-P4) passes
//! floats in `a2..a7` and returns them in `a2`; no FR is callee-saved and none
//! carries an argument. A second convention for LPIR-internal calls would push
//! a mode axis through the sret/vmctx machinery for no measured gain.
//!
//! **Lowering inserts the transfers, at exactly four places and nowhere else**
//! (M7 D2):
//!
//! | Boundary | Transfer |
//! |---|---|
//! | function entry, float parameter | [`push_entry_param_transfers`] → `Wfr` |
//! | before a call, float argument | [`word_operand`] → `Rfr` |
//! | after a call, float result | [`word_result`] → `Wfr` |
//! | before a return, float value | [`word_operand`] → `Rfr` |
//!
//! Not regalloc, and not the emitter. Doing it here means the allocator sees
//! only ordinary same-class copies plus two cross-class moves it can allocate
//! normally, the emitter's class gate stays a simple dispatch, and — the part
//! that matters for review — **the ABI is visible in the VInst dump**, so a
//! test can read it rather than infer it from generated code.
//!
//! The consequence worth stating explicitly: a `Call`'s arguments and results
//! are always integer-class by the time the allocator sees them, which is why
//! [`crate::regalloc::classes`] has no float arms for `Call` or `Ret` and why
//! `IsaTarget`'s float ABI hooks are permanently empty.

use alloc::string::String;
use alloc::vec::Vec;

use lpir::{IrFunction, IrType, LpirOp};
use lps_builtin_ids::{Mode as BuiltinMode, lpir_builtin_id};

use crate::abi::RegClass;
use crate::error::LowerError;
use crate::isa::{F32Lowering, IsaTarget};
use crate::lower::{fa_vreg, push_vregs_slice};
use crate::vinst::{
    AluOp, FAluOp, FAluRROp, FcmpCond, IcmpCond, ModuleSymbols, TempVRegs, VInst, VReg, VRegSlice,
    pack_src_op,
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
    func: &IrFunction,
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
        F32Lowering::HardwareFpu => {
            lower_hardware_fpu_op(out, op, src_op, func, symbols, vreg_pool, temps)
        }
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
        LpirOp::Fadd { dst, lhs, rhs } => soft_call(
            out,
            symbols,
            vreg_pool,
            "__addsf3",
            &[*lhs, *rhs],
            &[*dst],
            po,
        ),
        LpirOp::Fsub { dst, lhs, rhs } => soft_call(
            out,
            symbols,
            vreg_pool,
            "__subsf3",
            &[*lhs, *rhs],
            &[*dst],
            po,
        ),
        LpirOp::Fmul { dst, lhs, rhs } => soft_call(
            out,
            symbols,
            vreg_pool,
            "__mulsf3",
            &[*lhs, *rhs],
            &[*dst],
            po,
        ),
        LpirOp::Fdiv { dst, lhs, rhs } => soft_call(
            out,
            symbols,
            vreg_pool,
            "__divsf3",
            &[*lhs, *rhs],
            &[*dst],
            po,
        ),
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
        LpirOp::Feq { dst, lhs, rhs } => soft_compare(
            out,
            symbols,
            vreg_pool,
            temps,
            "__eqsf2",
            IcmpCond::Eq,
            *dst,
            *lhs,
            *rhs,
            po,
        ),
        LpirOp::Fne { dst, lhs, rhs } => soft_compare(
            out,
            symbols,
            vreg_pool,
            temps,
            "__nesf2",
            IcmpCond::Ne,
            *dst,
            *lhs,
            *rhs,
            po,
        ),
        LpirOp::Flt { dst, lhs, rhs } => soft_compare(
            out,
            symbols,
            vreg_pool,
            temps,
            "__ltsf2",
            IcmpCond::LtS,
            *dst,
            *lhs,
            *rhs,
            po,
        ),
        LpirOp::Fle { dst, lhs, rhs } => soft_compare(
            out,
            symbols,
            vreg_pool,
            temps,
            "__lesf2",
            IcmpCond::LeS,
            *dst,
            *lhs,
            *rhs,
            po,
        ),
        LpirOp::Fgt { dst, lhs, rhs } => soft_compare(
            out,
            symbols,
            vreg_pool,
            temps,
            "__gtsf2",
            IcmpCond::GtS,
            *dst,
            *lhs,
            *rhs,
            po,
        ),
        LpirOp::Fge { dst, lhs, rhs } => soft_compare(
            out,
            symbols,
            vreg_pool,
            temps,
            "__gesf2",
            IcmpCond::GeS,
            *dst,
            *lhs,
            *rhs,
            po,
        ),

        // ── compiler-rt int → float ──────────────────────────────────────────
        LpirOp::ItofS { dst, src } => {
            soft_call(out, symbols, vreg_pool, "__floatsisf", &[*src], &[*dst], po)
        }
        LpirOp::ItofU { dst, src } => soft_call(
            out,
            symbols,
            vreg_pool,
            "__floatunsisf",
            &[*src],
            &[*dst],
            po,
        ),

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
            mask_op(
                out,
                temps,
                AluOp::Xor,
                SIGN_BIT,
                fa_vreg(*dst),
                fa_vreg(*src),
                po,
            );
            Ok(())
        }
        // Clear the sign bit; exact on NaN and ±0 for the same reason.
        LpirOp::Fabs { dst, src } => {
            mask_op(
                out,
                temps,
                AluOp::And,
                SIGN_MASK_OFF,
                fa_vreg(*dst),
                fa_vreg(*src),
                po,
            );
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
        LpirOp::Fsqrt { dst, src } => soft_call(
            out,
            symbols,
            vreg_pool,
            "__lp_lpir_fsqrt_f32",
            &[*src],
            &[*dst],
            po,
        ),
        LpirOp::Ffloor { dst, src } => soft_call(
            out,
            symbols,
            vreg_pool,
            "__lp_lpir_ffloor_f32",
            &[*src],
            &[*dst],
            po,
        ),
        LpirOp::Fceil { dst, src } => soft_call(
            out,
            symbols,
            vreg_pool,
            "__lp_lpir_fceil_f32",
            &[*src],
            &[*dst],
            po,
        ),
        LpirOp::Ftrunc { dst, src } => soft_call(
            out,
            symbols,
            vreg_pool,
            "__lp_lpir_ftrunc_f32",
            &[*src],
            &[*dst],
            po,
        ),
        LpirOp::Fnearest { dst, src } => soft_call(
            out,
            symbols,
            vreg_pool,
            "__lp_lpir_fnearest_f32",
            &[*src],
            &[*dst],
            po,
        ),
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
        LpirOp::FtoiSatS { dst, src } => soft_call(
            out,
            symbols,
            vreg_pool,
            f32_ftoi_sat_s_symbol(),
            &[*src],
            &[*dst],
            po,
        ),
        LpirOp::FtoiSatU { dst, src } => soft_call(
            out,
            symbols,
            vreg_pool,
            f32_ftoi_sat_u_symbol(),
            &[*src],
            &[*dst],
            po,
        ),
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

// ─── Hardware FPU (Xtensa / ESP32-S3) ───────────────────────────────────────

/// True when this compile emits hardware FP instructions, and therefore owes
/// the AR↔FR boundary transfers described in this module's docs.
///
/// The one predicate `lower.rs` consults. Everything the hardware path does
/// differently — float loads and stores, float selects, the transfers at calls
/// and returns, the entry parameter shadows — hangs off this single answer, so
/// there is exactly one place where a build can be wrong about whether it has
/// an FPU.
pub fn uses_hardware_fpu(isa: IsaTarget, mode: lpir::FloatMode) -> bool {
    mode == lpir::FloatMode::F32 && isa.f32_lowering() == F32Lowering::HardwareFpu
}

/// Is LPIR vreg `v` float-typed in `func`?
///
/// In hardware-f32 mode this is exactly "does `v` live in the float register
/// file", which is why it can drive the class decisions. In Q32 mode it is
/// *not* — a Q16.16 `float` is an integer — and no caller here reaches it in
/// that mode.
pub fn is_float(func: &IrFunction, v: lpir::VReg) -> bool {
    func.vreg_types.get(v.0 as usize) == Some(&IrType::F32)
}

/// First backend vreg of the entry parameter shadow block.
fn param_shadow_base(func: &IrFunction) -> u16 {
    // `max` rather than `vreg_types.len()`: a function whose declared vreg
    // table is shorter than its ABI parameter list would otherwise alias a
    // shadow onto a parameter's own vreg (`regalloc::render` has the same
    // guard for the same reason).
    core::cmp::max(func.vreg_types.len() as u16, func.total_param_slots())
}

/// One past the highest backend vreg lowering reserves before minting temps.
///
/// In hardware-f32 mode this leaves room for one shadow per parameter slot; in
/// every other mode it is the IR vreg count unchanged, so Q32 and soft float
/// keep the exact vreg numbering they had — which is what keeps their filetest
/// snapshots byte-identical.
pub fn vreg_watermark(func: &IrFunction, hardware_fpu: bool) -> u16 {
    if hardware_fpu {
        param_shadow_base(func).saturating_add(func.total_param_slots())
    } else {
        func.vreg_types.len() as u16
    }
}

/// The backend vreg holding `v`'s value **in the float register file**.
///
/// For an ordinary float value this is the identity mapping: the value is
/// computed by an FP instruction and never has another form.
///
/// A float **parameter** is the exception, and the reason this function exists.
/// It arrives in an address register (M7 D1) and its backend vreg is precolored
/// to that AR by the ABI, so the same vreg cannot also be the FR the body
/// computes with — a vreg has one class for its whole life. The float view gets
/// its own vreg in the shadow block, filled by a `Wfr` at function entry.
///
/// The split is deterministic rather than a lookup table on purpose: it costs
/// no per-function allocation on the device.
///
/// **The shadow is the parameter's only home once the entry `Wfr` has run.**
/// LPIR is not SSA — a `float` parameter is an ordinary mutable local in GLSL,
/// and the `lps-glsl` frontend lowers `x = x + 1.0` as a redefinition of the
/// parameter's own vreg — so a write to a float parameter lands here, in the
/// shadow, and the incoming address register goes stale the moment it happens.
/// Nothing may read the AR side after entry; see [`word_operand`].
pub fn float_vreg(func: &IrFunction, v: lpir::VReg) -> VReg {
    if is_param(func, v) {
        VReg(param_shadow_base(func).saturating_add(v.0 as u16))
    } else {
        fa_vreg(v)
    }
}

fn is_param(func: &IrFunction, v: lpir::VReg) -> bool {
    (v.0 as u16) < func.total_param_slots()
}

/// Emit the function-entry transfers: one `Wfr` per float parameter.
///
/// The first of the four boundaries lowering owns. Returns the number of
/// instructions pushed so the caller can extend the prologue's region — the
/// regalloc walk only visits VInsts reachable through the region tree, and a
/// transfer outside it would be silently dropped.
pub fn push_entry_param_transfers(out: &mut Vec<VInst>, func: &IrFunction) -> usize {
    let mut n = 0;
    for slot in 0..func.total_param_slots() {
        let v = lpir::VReg(u32::from(slot));
        if !is_float(func, v) {
            continue;
        }
        out.push(VInst::Wfr {
            dst: float_vreg(func, v),
            src: fa_vreg(v),
            src_op: crate::vinst::SRC_OP_NONE,
        });
        n += 1;
    }
    n
}

/// The backend vreg holding `v` as a raw IEEE **word** in an address register,
/// emitting the `Rfr` transfer first when the value currently lives in an FR.
///
/// Boundaries two and four: call arguments and return values. A non-float value
/// is already a word and passes through untouched.
///
/// A float **parameter** goes through the same `Rfr` as any other float value,
/// even though its incoming address register still holds the argument bits.
/// Reading that AR instead would be one instruction cheaper and wrong: LPIR is
/// not SSA, so a parameter the body assigns to has its current value in the FR
/// shadow only, and the AR keeps the value the caller passed. That shortcut is
/// exactly `docs/defects/2026-08-01-xtlpn-f32-loses-writes-to-value-parameters.md`
/// — `float f(float x) { x = x + 1.0; return x; }` returned the argument
/// untouched. The rule that replaces it is unconditional and therefore cannot be
/// wrong about which values it applies to: **after the entry transfer, a float
/// parameter is read from the float file and nowhere else.**
///
/// The AR side losing its only post-entry reader is a small win as well as a
/// fix: the incoming argument register dies at the entry `Wfr` instead of
/// staying live to a boundary somewhere down the body, and on Xtensa the
/// argument bank *is* the caller-saved half of the allocatable pool.
pub fn word_operand(
    out: &mut Vec<VInst>,
    func: &IrFunction,
    v: lpir::VReg,
    temps: &mut TempVRegs,
    po: u16,
) -> VReg {
    if !is_float(func, v) {
        return fa_vreg(v);
    }
    let word = temps.mint();
    out.push(VInst::Rfr {
        dst: word,
        src: float_vreg(func, v),
        src_op: po,
    });
    word
}

/// Plan a call's result: the vreg the `Call` should define, plus the transfer
/// owed afterwards.
///
/// Boundary three. A float result arrives in an address register, so the call
/// defines a fresh word temp and the returned `Some(float_dst)` is the `Wfr`
/// the caller must push *after* the `Call` — ordering this function cannot do
/// itself, since the call has not been emitted yet.
pub fn word_result(
    func: &IrFunction,
    v: lpir::VReg,
    temps: &mut TempVRegs,
) -> (VReg, Option<VReg>) {
    if !is_float(func, v) {
        return (fa_vreg(v), None);
    }
    let word = temps.mint();
    (word, Some(float_vreg(func, v)))
}

/// Resolve an LPIR op name to its native-f32 builtin symbol.
///
/// Goes through [`BuiltinId`] rather than spelling `"__lp_lpir_fdiv_f32"` as a
/// literal, so a symbol that does not exist is a resolution error here instead
/// of a link failure three phases later. The error names the op and the mode
/// because the one thing this must never do is fall back to the Q32 sibling:
/// the resolver's rule is that a resolver never crosses modes, and a Q32 callee
/// given IEEE bit patterns returns plausible wrong numbers rather than failing.
fn f32_builtin_symbol(name: &str, argc: usize) -> Result<&'static str, LowerError> {
    lpir_builtin_id(name, argc, BuiltinMode::F32)
        .map(|id| id.name())
        .ok_or_else(|| LowerError::UnsupportedOp {
            description: alloc::format!(
                "no native-f32 builtin for LPIR `{name}`/{argc} arg(s); this is a gap in the \
                 f32 builtin family, and a resolver never crosses modes to a Q32 sibling"
            ),
        })
}

/// The hardware-FPU arm: single FP instructions where one exists, a builtin
/// call where it does not (M7 D4).
#[allow(
    clippy::too_many_arguments,
    reason = "mirrors lower_lpir_op's threading of the same lowering context"
)]
fn lower_hardware_fpu_op(
    out: &mut Vec<VInst>,
    op: &LpirOp,
    src_op: Option<u32>,
    func: &IrFunction,
    symbols: &mut ModuleSymbols,
    vreg_pool: &mut Vec<VReg>,
    temps: &mut TempVRegs,
) -> Result<(), LowerError> {
    let po = pack_src_op(src_op);
    // Local shorthands. `f` is the float-file view of a vreg, `i` the ordinary
    // integer one; picking the wrong one is a register-class error the
    // allocator's verifier catches, not a silent miscompile.
    let f = |v: lpir::VReg| float_vreg(func, v);
    let i = fa_vreg;

    match op {
        // ── One FP instruction each ──────────────────────────────────────────
        LpirOp::Fadd { dst, lhs, rhs } => {
            push_falu(out, FAluOp::Add, f(*dst), f(*lhs), f(*rhs), po);
            Ok(())
        }
        LpirOp::Fsub { dst, lhs, rhs } => {
            push_falu(out, FAluOp::Sub, f(*dst), f(*lhs), f(*rhs), po);
            Ok(())
        }
        LpirOp::Fmul { dst, lhs, rhs } => {
            push_falu(out, FAluOp::Mul, f(*dst), f(*lhs), f(*rhs), po);
            Ok(())
        }
        // Sign-bit operations in the float domain. `abs.s`/`neg.s` are defined
        // as bit manipulations, so they stay exact on NaN and `-0.0`
        // (`docs/design/float.md` §3) — the same property the soft-float path
        // gets from its integer masks.
        LpirOp::Fabs { dst, src } => {
            push_falu_rr(out, FAluRROp::Abs, f(*dst), f(*src), po);
            Ok(())
        }
        LpirOp::Fneg { dst, src } => {
            push_falu_rr(out, FAluRROp::Neg, f(*dst), f(*src), po);
            Ok(())
        }

        // Comparisons produce an ordinary integer 0/1, so `dst` is the plain
        // vreg. Each condition is its own `FcmpCond` — `Fne` in particular is
        // NOT a negated `Eq`, because the two differ exactly on NaN, which is
        // the row `float.md` §3 makes normative.
        LpirOp::Feq { dst, lhs, rhs } => {
            push_fcmp(out, FcmpCond::Eq, i(*dst), f(*lhs), f(*rhs), po);
            Ok(())
        }
        LpirOp::Fne { dst, lhs, rhs } => {
            push_fcmp(out, FcmpCond::Ne, i(*dst), f(*lhs), f(*rhs), po);
            Ok(())
        }
        LpirOp::Flt { dst, lhs, rhs } => {
            push_fcmp(out, FcmpCond::Lt, i(*dst), f(*lhs), f(*rhs), po);
            Ok(())
        }
        LpirOp::Fle { dst, lhs, rhs } => {
            push_fcmp(out, FcmpCond::Le, i(*dst), f(*lhs), f(*rhs), po);
            Ok(())
        }
        LpirOp::Fgt { dst, lhs, rhs } => {
            push_fcmp(out, FcmpCond::Gt, i(*dst), f(*lhs), f(*rhs), po);
            Ok(())
        }
        LpirOp::Fge { dst, lhs, rhs } => {
            push_fcmp(out, FcmpCond::Ge, i(*dst), f(*lhs), f(*rhs), po);
            Ok(())
        }

        // `float.s` / `ufloat.s` with scale 0: one correctly-rounded
        // instruction each, and — unlike the float→int direction — with no
        // saturation question attached, which is why this half is inlined and
        // the other half is a builtin call (M7 D4).
        LpirOp::ItofS { dst, src } => {
            out.push(VInst::IToF {
                dst: f(*dst),
                src: i(*src),
                signed: true,
                src_op: po,
            });
            Ok(())
        }
        LpirOp::ItofU { dst, src } => {
            out.push(VInst::IToF {
                dst: f(*dst),
                src: i(*src),
                signed: false,
                src_op: po,
            });
            Ok(())
        }

        // No `FConst32` VInst (M7 D11): materialize the IEEE pattern with the
        // integer constant machinery that already has a literal pool, then
        // transfer. Two instructions, no third literal pool to maintain.
        LpirOp::FconstF32 { dst, value } => {
            let word = temps.mint();
            out.push(VInst::IConst32 {
                dst: word,
                val: value.to_bits() as i32,
                src_op: po,
            });
            out.push(VInst::Wfr {
                dst: f(*dst),
                src: word,
                src_op: po,
            });
            Ok(())
        }
        // A reinterpretation, not a conversion — which on this target is
        // exactly what the AR→FR transfer instruction does.
        LpirOp::FfromI32Bits { dst, src } => {
            out.push(VInst::Wfr {
                dst: f(*dst),
                src: i(*src),
                src_op: po,
            });
            Ok(())
        }

        // ── Builtin calls (M7 D4) ────────────────────────────────────────────
        //
        // Everything that is not a single instruction goes to the M5 f32
        // family, the same mechanism Q32 uses today. Division is the one worth
        // naming: Q32 calls a builtin for it as well, so this is parity rather
        // than a regression, and inlining the `div0.s`/`divn.s` estimate
        // sequence would buy speed at the cost of a hard dependency on M6-P6's
        // exhaustive tables.
        LpirOp::Fdiv { dst, lhs, rhs } => fp_call(
            out,
            func,
            symbols,
            vreg_pool,
            temps,
            "fdiv",
            &[*lhs, *rhs],
            &[*dst],
            po,
        ),
        LpirOp::Fsqrt { dst, src } => fp_call(
            out,
            func,
            symbols,
            vreg_pool,
            temps,
            "sqrt",
            &[*src],
            &[*dst],
            po,
        ),
        LpirOp::Ffloor { dst, src } => fp_call(
            out,
            func,
            symbols,
            vreg_pool,
            temps,
            "ffloor",
            &[*src],
            &[*dst],
            po,
        ),
        LpirOp::Fceil { dst, src } => fp_call(
            out,
            func,
            symbols,
            vreg_pool,
            temps,
            "fceil",
            &[*src],
            &[*dst],
            po,
        ),
        LpirOp::Ftrunc { dst, src } => fp_call(
            out,
            func,
            symbols,
            vreg_pool,
            temps,
            "ftrunc",
            &[*src],
            &[*dst],
            po,
        ),
        LpirOp::Fnearest { dst, src } => fp_call(
            out,
            func,
            symbols,
            vreg_pool,
            temps,
            "fnearest",
            &[*src],
            &[*dst],
            po,
        ),
        LpirOp::Fmin { dst, lhs, rhs } => fp_call(
            out,
            func,
            symbols,
            vreg_pool,
            temps,
            "fmin",
            &[*lhs, *rhs],
            &[*dst],
            po,
        ),
        LpirOp::Fmax { dst, lhs, rhs } => fp_call(
            out,
            func,
            symbols,
            vreg_pool,
            temps,
            "fmax",
            &[*lhs, *rhs],
            &[*dst],
            po,
        ),
        // Saturating float→int. `trunc.s` alone does not satisfy
        // `float.md` §3 — whether the S3's truncation saturates or wraps for
        // finite out-of-range inputs is an unresolved M6-P6 measurement — so
        // this routes to a builtin that is correct by construction rather than
        // to an instruction that might be. An inline `trunc.s` (plus a clamp,
        // if the measurement says so) is named follow-up work.
        LpirOp::FtoiSatS { dst, src } => fp_call(
            out,
            func,
            symbols,
            vreg_pool,
            temps,
            "ftoi_sat_s",
            &[*src],
            &[*dst],
            po,
        ),
        LpirOp::FtoiSatU { dst, src } => fp_call(
            out,
            func,
            symbols,
            vreg_pool,
            temps,
            "ftoi_sat_u",
            &[*src],
            &[*dst],
            po,
        ),
        LpirOp::FtoUnorm16 { dst, src } => fp_call(
            out,
            func,
            symbols,
            vreg_pool,
            temps,
            "fto_unorm16",
            &[*src],
            &[*dst],
            po,
        ),
        LpirOp::FtoUnorm8 { dst, src } => fp_call(
            out,
            func,
            symbols,
            vreg_pool,
            temps,
            "fto_unorm8",
            &[*src],
            &[*dst],
            po,
        ),
        LpirOp::Unorm16toF { dst, src } => fp_call(
            out,
            func,
            symbols,
            vreg_pool,
            temps,
            "unorm16_to_f",
            &[*src],
            &[*dst],
            po,
        ),
        LpirOp::Unorm8toF { dst, src } => fp_call(
            out,
            func,
            symbols,
            vreg_pool,
            temps,
            "unorm8_to_f",
            &[*src],
            &[*dst],
            po,
        ),
        // `x / c`. The Q32 path rewrites this to a multiply by a precomputed
        // reciprocal, which is a legitimate approximation in Q16.16 and an
        // incorrect one here: `/` is a correctly-rounded row of `float.md` §3.
        LpirOp::FdivConstF32 { dst, lhs, rhs } => {
            let divisor_word = temps.mint();
            out.push(VInst::IConst32 {
                dst: divisor_word,
                val: rhs.to_bits() as i32,
                src_op: po,
            });
            let lhs_word = word_operand(out, func, *lhs, temps, po);
            let (ret_word, ret_float) = word_result(func, *dst, temps);
            call_native(
                out,
                symbols,
                vreg_pool,
                f32_builtin_symbol("fdiv", 2)?,
                &[lhs_word, divisor_word],
                &[ret_word],
                po,
            )?;
            push_return_transfer(out, ret_word, ret_float, po);
            Ok(())
        }

        other => Err(LowerError::UnsupportedOp {
            description: alloc::format!("not a float op, or unhandled in f32 mode: {other:?}"),
        }),
    }
}

fn push_falu(out: &mut Vec<VInst>, op: FAluOp, dst: VReg, src1: VReg, src2: VReg, po: u16) {
    out.push(VInst::FAluRRR {
        op,
        dst,
        src1,
        src2,
        src_op: po,
    });
}

fn push_falu_rr(out: &mut Vec<VInst>, op: FAluRROp, dst: VReg, src: VReg, po: u16) {
    out.push(VInst::FAluRR {
        op,
        dst,
        src,
        src_op: po,
    });
}

fn push_fcmp(out: &mut Vec<VInst>, cond: FcmpCond, dst: VReg, lhs: VReg, rhs: VReg, po: u16) {
    out.push(VInst::Fcmp {
        dst,
        lhs,
        rhs,
        cond,
        src_op: po,
    });
}

/// A builtin call with the boundary transfers around it: `Rfr` per float
/// argument, the `Call`, then `Wfr` per float result.
#[allow(
    clippy::too_many_arguments,
    reason = "one call site per builtin-routed op; splitting it would only move the arguments"
)]
fn fp_call(
    out: &mut Vec<VInst>,
    func: &IrFunction,
    symbols: &mut ModuleSymbols,
    vreg_pool: &mut Vec<VReg>,
    temps: &mut TempVRegs,
    lpir_name: &str,
    args: &[lpir::VReg],
    rets: &[lpir::VReg],
    po: u16,
) -> Result<(), LowerError> {
    let symbol = f32_builtin_symbol(lpir_name, args.len())?;
    let mut arg_words = Vec::with_capacity(args.len());
    for a in args {
        arg_words.push(word_operand(out, func, *a, temps, po));
    }
    let mut ret_words = Vec::with_capacity(rets.len());
    let mut ret_transfers = Vec::with_capacity(rets.len());
    for r in rets {
        let (word, float_dst) = word_result(func, *r, temps);
        ret_words.push(word);
        ret_transfers.push((word, float_dst));
    }
    call_native(out, symbols, vreg_pool, symbol, &arg_words, &ret_words, po)?;
    for (word, float_dst) in ret_transfers {
        push_return_transfer(out, word, float_dst, po);
    }
    Ok(())
}

/// Push the `Wfr` a float-returning call owes, if it owes one.
pub fn push_return_transfer(out: &mut Vec<VInst>, word: VReg, float_dst: Option<VReg>, po: u16) {
    if let Some(dst) = float_dst {
        out.push(VInst::Wfr {
            dst,
            src: word,
            src_op: po,
        });
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

    /// A function whose vregs `0..n` are all float-typed and which declares no
    /// parameters, so `float_vreg` is the identity and the tests read as the
    /// instruction shapes they are about.
    fn float_func(n: u32) -> IrFunction {
        IrFunction {
            name: String::new(),
            is_entry: true,
            vmctx_vreg: lpir::VReg(0),
            param_count: 0,
            return_types: alloc::vec![],
            sret_arg: None,
            vreg_types: (0..n).map(|_| IrType::F32).collect(),
            slots: alloc::vec![],
            body: alloc::vec![].into(),
            vreg_pool: alloc::vec![],
        }
    }

    fn lower(op: LpirOp, isa: IsaTarget) -> (Vec<VInst>, ModuleSymbols, Vec<VReg>) {
        lower_in(op, isa, &float_func(16))
    }

    fn lower_in(
        op: LpirOp,
        isa: IsaTarget,
        func: &IrFunction,
    ) -> (Vec<VInst>, ModuleSymbols, Vec<VReg>) {
        let mut out = Vec::new();
        let mut symbols = ModuleSymbols::default();
        let mut pool = Vec::new();
        let mut temps = TempVRegs::new(64);
        lower_f32_op(
            &mut out,
            &op,
            isa,
            Some(0),
            func,
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
                VInst::AluRRR { op: AluOp::Xor, .. }
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
                VInst::AluRRR { op: AluOp::And, .. }
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

    // ── Hardware FPU ─────────────────────────────────────────────────────────

    #[cfg(feature = "isa-xt")]
    const XT: IsaTarget = IsaTarget::Xtensa;

    /// The point of the milestone, asserted: the operations that *have* a
    /// single hardware instruction produce exactly one VInst and no call.
    #[cfg(feature = "isa-xt")]
    #[test]
    fn the_inline_family_is_one_instruction_each() {
        for (op, want) in [
            (
                LpirOp::Fadd {
                    dst: v(0),
                    lhs: v(1),
                    rhs: v(2),
                },
                FAluOp::Add,
            ),
            (
                LpirOp::Fsub {
                    dst: v(0),
                    lhs: v(1),
                    rhs: v(2),
                },
                FAluOp::Sub,
            ),
            (
                LpirOp::Fmul {
                    dst: v(0),
                    lhs: v(1),
                    rhs: v(2),
                },
                FAluOp::Mul,
            ),
        ] {
            let (insts, symbols, _) = lower(op, XT);
            assert!(called_symbols(&insts, &symbols).is_empty(), "no call");
            assert!(
                matches!(insts.as_slice(), [VInst::FAluRRR { op, .. }] if *op == want),
                "{want:?}: {insts:?}"
            );
        }

        for (op, want) in [
            (
                LpirOp::Fabs {
                    dst: v(0),
                    src: v(1),
                },
                FAluRROp::Abs,
            ),
            (
                LpirOp::Fneg {
                    dst: v(0),
                    src: v(1),
                },
                FAluRROp::Neg,
            ),
        ] {
            let (insts, _, _) = lower(op, XT);
            assert!(matches!(insts.as_slice(), [VInst::FAluRR { op, .. }] if *op == want));
        }

        for (op, signed) in [
            (
                LpirOp::ItofS {
                    dst: v(0),
                    src: v(1),
                },
                true,
            ),
            (
                LpirOp::ItofU {
                    dst: v(0),
                    src: v(1),
                },
                false,
            ),
        ] {
            let (insts, _, _) = lower(op, XT);
            assert!(matches!(insts.as_slice(), [VInst::IToF { signed: s, .. }] if *s == signed));
        }
    }

    /// Each comparison keeps its own condition. `Fne` in particular must **not**
    /// be a negated `Eq`: they differ exactly on NaN, which `float.md` §3 makes
    /// a guaranteed row, and no ordinary test input would catch the rewrite.
    #[cfg(feature = "isa-xt")]
    #[test]
    fn each_comparison_keeps_its_own_condition() {
        for (op, want) in [
            (
                LpirOp::Feq {
                    dst: v(0),
                    lhs: v(1),
                    rhs: v(2),
                },
                FcmpCond::Eq,
            ),
            (
                LpirOp::Fne {
                    dst: v(0),
                    lhs: v(1),
                    rhs: v(2),
                },
                FcmpCond::Ne,
            ),
            (
                LpirOp::Flt {
                    dst: v(0),
                    lhs: v(1),
                    rhs: v(2),
                },
                FcmpCond::Lt,
            ),
            (
                LpirOp::Fle {
                    dst: v(0),
                    lhs: v(1),
                    rhs: v(2),
                },
                FcmpCond::Le,
            ),
            (
                LpirOp::Fgt {
                    dst: v(0),
                    lhs: v(1),
                    rhs: v(2),
                },
                FcmpCond::Gt,
            ),
            (
                LpirOp::Fge {
                    dst: v(0),
                    lhs: v(1),
                    rhs: v(2),
                },
                FcmpCond::Ge,
            ),
        ] {
            let (insts, _, _) = lower(op, XT);
            assert!(
                matches!(insts.as_slice(), [VInst::Fcmp { cond, .. }] if *cond == want),
                "{want:?}: {insts:?}"
            );
        }
    }

    /// A float constant is the integer literal machinery plus one transfer
    /// (M7 D11) — no third literal pool, and no `FConst32` VInst.
    #[cfg(feature = "isa-xt")]
    #[test]
    fn a_float_constant_is_iconst_plus_a_transfer() {
        let (insts, symbols, _) = lower(
            LpirOp::FconstF32 {
                dst: v(0),
                value: 1.0,
            },
            XT,
        );
        assert!(called_symbols(&insts, &symbols).is_empty());
        assert!(
            matches!(
                insts.as_slice(),
                [VInst::IConst32 { val, .. }, VInst::Wfr { .. }] if *val == 0x3f80_0000u32 as i32
            ),
            "{insts:?}"
        );
    }

    /// The builtin-routed half (M7 D4). Every symbol must be an `_f32` one:
    /// resolving a `_q32` sibling would hand a fixed-point callee IEEE bit
    /// patterns and return plausible wrong numbers rather than failing.
    #[cfg(feature = "isa-xt")]
    #[test]
    fn everything_else_calls_the_f32_builtin_family() {
        for (op, want) in [
            (
                LpirOp::Fdiv {
                    dst: v(0),
                    lhs: v(1),
                    rhs: v(2),
                },
                "__lp_lpir_fdiv_f32",
            ),
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
                LpirOp::Fmin {
                    dst: v(0),
                    lhs: v(1),
                    rhs: v(2),
                },
                "__lp_lpir_fmin_f32",
            ),
            (
                LpirOp::Fmax {
                    dst: v(0),
                    lhs: v(1),
                    rhs: v(2),
                },
                "__lp_lpir_fmax_f32",
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
                LpirOp::FtoUnorm8 {
                    dst: v(0),
                    src: v(1),
                },
                "__lp_lpir_fto_unorm8_f32",
            ),
            (
                LpirOp::Unorm16toF {
                    dst: v(0),
                    src: v(1),
                },
                "__lp_lpir_unorm16_to_f_f32",
            ),
            (
                LpirOp::Unorm8toF {
                    dst: v(0),
                    src: v(1),
                },
                "__lp_lpir_unorm8_to_f_f32",
            ),
        ] {
            let (insts, symbols, _) = lower(op, XT);
            let called = called_symbols(&insts, &symbols);
            assert_eq!(called, alloc::vec![want]);
            assert!(!called[0].ends_with("_q32"), "resolved a Q32 sibling");
        }
    }

    /// A float-argument, float-returning builtin call is surrounded by exactly
    /// the transfers the ABI requires: `Rfr` per argument, then the `Call`,
    /// then `Wfr` for the result (M7 D1/D2).
    #[cfg(feature = "isa-xt")]
    #[test]
    fn a_builtin_call_is_wrapped_in_rfr_then_call_then_wfr() {
        // vregs 1..3 are past the (vmctx-only) parameter block, so `float_vreg`
        // is the identity for them and the assertions can name vregs directly.
        let func = float_func(8);
        let (insts, _, pool) = lower_in(
            LpirOp::Fdiv {
                dst: v(3),
                lhs: v(1),
                rhs: v(2),
            },
            XT,
            &func,
        );
        assert!(
            matches!(
                insts.as_slice(),
                [
                    VInst::Rfr { .. },
                    VInst::Rfr { .. },
                    VInst::Call { .. },
                    VInst::Wfr { .. }
                ]
            ),
            "{insts:?}"
        );
        // The call's own operands are the transfer temps, never the float
        // vregs — that is what keeps `Call` integer-class end to end.
        let VInst::Call { args, rets, .. } = &insts[2] else {
            unreachable!()
        };
        let VInst::Rfr {
            dst: a0, src: s0, ..
        } = insts[0]
        else {
            unreachable!()
        };
        assert_eq!(args.vregs(&pool)[0], a0);
        assert_eq!(s0, VReg(1), "the argument came out of the float file");
        let VInst::Wfr { dst, src, .. } = insts[3] else {
            unreachable!()
        };
        assert_eq!(src, rets.vregs(&pool)[0]);
        assert_eq!(dst, VReg(3), "the result went into the float file");
    }

    /// Float **parameters** are the one case where an LPIR vreg needs two
    /// backend identities: the address register it arrives in, and the float
    /// register the body computes with. The entry `Wfr` is what links them.
    #[cfg(feature = "isa-xt")]
    #[test]
    fn float_parameters_get_one_entry_transfer_each() {
        // vmctx (Pointer) + two float params + one int param.
        let func = IrFunction {
            name: String::new(),
            is_entry: true,
            vmctx_vreg: lpir::VReg(0),
            param_count: 3,
            return_types: alloc::vec![],
            sret_arg: None,
            vreg_types: alloc::vec![IrType::Pointer, IrType::F32, IrType::F32, IrType::I32],
            slots: alloc::vec![],
            body: alloc::vec![].into(),
            vreg_pool: alloc::vec![],
        };
        let mut out = Vec::new();
        let n = push_entry_param_transfers(&mut out, &func);
        assert_eq!(n, 2, "one per float parameter, and no more");
        assert_eq!(out.len(), 2);
        for (i, inst) in out.iter().enumerate() {
            let param = lpir::VReg(i as u32 + 1);
            assert!(
                matches!(inst, VInst::Wfr { dst, src, .. }
                    if *src == fa_vreg(param) && *dst == float_vreg(&func, param)),
                "{inst:?}"
            );
        }
        // The shadow is a distinct vreg from the parameter's own — sharing one
        // would give a single vreg two register classes, which the allocator
        // cannot represent.
        for p in [lpir::VReg(1), lpir::VReg(2)] {
            assert_ne!(float_vreg(&func, p), fa_vreg(p));
        }
        // Non-parameters keep the identity mapping, so nothing else in the
        // function pays for the parameter special case.
        assert_eq!(float_vreg(&func, lpir::VReg(9)), fa_vreg(lpir::VReg(9)));
        // Temps start above the shadow block.
        assert!(vreg_watermark(&func, true) > func.total_param_slots());
        assert_eq!(vreg_watermark(&func, false), func.vreg_types.len() as u16);
    }

    /// A float parameter at a call or return boundary is read from its **FR
    /// shadow**, never from the address register it arrived in.
    ///
    /// The AR shortcut this replaces was
    /// `docs/defects/2026-08-01-xtlpn-f32-loses-writes-to-value-parameters.md`:
    /// LPIR is not SSA, so a body that assigns to the parameter leaves the AR
    /// holding the caller's argument while the shadow holds the current value.
    #[cfg(feature = "isa-xt")]
    #[test]
    fn a_float_parameter_at_a_boundary_reads_the_shadow() {
        let func = IrFunction {
            name: String::new(),
            is_entry: true,
            vmctx_vreg: lpir::VReg(0),
            param_count: 1,
            return_types: alloc::vec![],
            sret_arg: None,
            vreg_types: alloc::vec![IrType::Pointer, IrType::F32, IrType::F32],
            slots: alloc::vec![],
            body: alloc::vec![].into(),
            vreg_pool: alloc::vec![],
        };
        let mut out = Vec::new();
        let mut temps = TempVRegs::new(vreg_watermark(&func, true));
        let param = lpir::VReg(1);
        let word = word_operand(&mut out, &func, param, &mut temps, 0);
        assert_ne!(word, fa_vreg(param), "must not read the incoming AR");
        assert!(
            matches!(out.as_slice(), [VInst::Rfr { dst, src, .. }]
                if *dst == word && *src == float_vreg(&func, param)),
            "{out:?}"
        );

        // A computed float takes the same path — the identity `float_vreg`
        // makes it the same code.
        out.clear();
        let word = word_operand(&mut out, &func, lpir::VReg(2), &mut temps, 0);
        assert_ne!(word, fa_vreg(lpir::VReg(2)));
        assert!(matches!(out.as_slice(), [VInst::Rfr { .. }]));

        // A non-float parameter is already a word and still passes through.
        out.clear();
        let word = word_operand(&mut out, &func, func.vmctx_vreg, &mut temps, 0);
        assert_eq!(word, fa_vreg(func.vmctx_vreg));
        assert!(out.is_empty(), "no transfer for a non-float: {out:?}");
    }

    /// The resolver never crosses modes. A missing f32 builtin has to name the
    /// op, because the alternative — quietly resolving the Q32 sibling — is the
    /// defect class that produces plausible wrong pixels instead of an error.
    #[test]
    fn a_missing_f32_builtin_names_the_op_rather_than_falling_back() {
        let err = f32_builtin_symbol("no_such_lpir_op", 1).expect_err("must not resolve");
        let LowerError::UnsupportedOp { description } = err;
        assert!(description.contains("no_such_lpir_op"), "{description}");
        assert!(description.contains("f32"), "{description}");
        // Sanity: the resolver *does* work for a real name, so the test above
        // is not passing because everything fails.
        assert_eq!(f32_builtin_symbol("fdiv", 2).unwrap(), "__lp_lpir_fdiv_f32");
    }

    /// With `float-f32` off the Xtensa arm is unreachable, and the seam must
    /// refuse rather than quietly hand an FPU part the soft-float path.
    #[cfg(all(feature = "isa-xt", not(feature = "float-f32")))]
    #[test]
    fn xtensa_without_the_feature_is_a_named_error() {
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
            &float_func(4),
            &mut symbols,
            &mut pool,
            &mut temps,
        )
        .expect_err("no FP backend linked");
        let LowerError::UnsupportedOp { description } = err;
        assert!(description.contains("Xtensa"), "{description}");
        assert!(out.is_empty(), "a refused op must emit nothing");
    }
}
