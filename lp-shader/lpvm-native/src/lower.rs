//! LPIR [`LpirOp`] → [`VInst`] lowering (M1 subset).
//!
//! # Q32 op lowering policy
//!
//! For each Q32 LPIR op the backend chooses one of four strategies:
//!
//! * **Inline.** Emit a short [`VInst`] sequence that directly performs
//!   the op. Used for ops where the inline expansion matches the helper
//!   bit-for-bit on the i32 input domain and is competitive with the
//!   call cost (typically <= ~6 RV32 instructions).
//!
//!   Currently inlined: `Fneg` (via [`VInst::Neg`]), `Fabs`, `Fmin`,
//!   `Fmax`, `FtoUnorm16`, `FtoUnorm8`, `Unorm16toF`, `Unorm8toF`.
//!
//! * **Dispatched on [`LowerOpts::q32`].** `Fadd`/`Fsub`/`Fmul`/`Fdiv`
//!   choose between the conservative saturating helper (default) and a
//!   faster non-saturating expansion based on the active [`Q32Options`].
//!   Defaults match the saturating helper bit-for-bit. Wrapping/Reciprocal
//!   modes match `lpvm-wasm`'s `emit/q32.rs` bit-for-bit so the browser
//!   preview agrees with the device.
//!
//!   * `Fadd`/`Fsub`: `Saturating` → [`BuiltinId::LpLpirFaddQ32`] /
//!     [`BuiltinId::LpLpirFsubQ32`] sym_call. `Wrapping` → 1-VInst inline
//!     [`AluOp::Add`] / [`AluOp::Sub`].
//!   * `Fmul`: `Saturating` → [`BuiltinId::LpLpirFmulQ32`] sym_call.
//!     `Wrapping` → 5-VInst `mul`/`mulh`/`srli`/`slli`/`or` sequence
//!     computing `((a * b) >> 16)` mod 2^32.
//!   * `Fdiv`: `Saturating` → [`BuiltinId::LpLpirFdivQ32`] sym_call.
//!     `Reciprocal` → [`BuiltinId::LpLpirFdivRecipQ32`] sym_call (~0.01%
//!     typical error; explicit divisor==0 saturation guard inside the
//!     helper).
//!
//!   See `docs/plans-old/2026-04-18-q32-options-dispatch/00-design.md`.
//!
//! * **`sym_call` (defer for review).** Non-trivial semantics
//!   (saturation, rounding modes, clamping, multi-word arithmetic) that
//!   warrant a dedicated correctness pass before inlining.
//!
//!   Currently call: `ItofS`, `ItofU`, `FtoiSatS`, `FtoiSatU`,
//!   `Ffloor`, `Fceil`, `Ftrunc`, `Fnearest`.
//!
//! * **`sym_call` (permanent).** Operation cost dwarfs the call overhead;
//!   inlining brings no benefit.
//!
//!   Currently call: `Fsqrt`.
//!
//! All Q32 helper functions remain in `lps-builtins` as the **reference
//! implementation** of op semantics. Inline expansions must match the
//! helper's behavior bit-for-bit on the i32 input domain.
//!
//! Zbb (`min`/`max`) is not enabled — ESP32-C6 silicon does not decode
//! it. If/when a Zbb-bearing target is added, `Fmin`/`Fmax` can collapse
//! to a single instruction via `AluOp::MinS` / `AluOp::MaxS`.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use lpir::{CalleeRef, FloatMode, IrFunction, LpirModule, LpirOp};
use lps_builtin_ids::{
    BuiltinId, GlslParamKind, glsl_lpfn_q32_builtin_id, glsl_q32_math_builtin_id,
    lpir_q32_builtin_id, texture_q32_builtin_id, vm_q32_builtin_id,
};

use crate::LowerOpts;
use crate::abi::ModuleAbi;
use crate::error::LowerError;
use crate::imm::fits_imm12;
use crate::region::{REGION_ID_NONE, RegionId, RegionTree};
use crate::vinst::{
    AluImmOp, AluOp, IcmpCond, LabelId, ModuleSymbols, SRC_OP_NONE, TempVRegs, VInst, VReg,
    VRegSlice, pack_src_op,
};
use lps_q32::q32_options::{AddSubMode, DivMode, MulMode};

#[inline]
fn fa_vreg(v: lpir::VReg) -> VReg {
    VReg(v.0 as u16)
}

fn push_vregs_slice(pool: &mut Vec<VReg>, ir: &[lpir::VReg]) -> Result<VRegSlice, LowerError> {
    if ir.len() > u8::MAX as usize {
        return Err(LowerError::UnsupportedOp {
            description: String::from("vreg slice too long for FA backend"),
        });
    }
    let start = u16::try_from(pool.len()).map_err(|_| LowerError::UnsupportedOp {
        description: String::from("vreg pool exhausted (u16)"),
    })?;
    for v in ir {
        pool.push(fa_vreg(*v));
    }
    Ok(VRegSlice {
        start,
        count: ir.len() as u8,
    })
}

/// Emit `dst = src OP imm` for an op that has both an `addi`-class immediate
/// form and an R-form. If `imm` fits a signed 12-bit immediate (the only
/// thing the RV32 `OP-IMM` encoding can hold) emit the I-form; otherwise
/// materialize `imm` into a fresh temp via [`VInst::IConst32`] and emit the
/// R-form. This is the fix for the silent low-12-bit truncation that used
/// to happen when LPIR emitted `IaddImm { imm: 65536 }` (the texture
/// render synth's per-pixel `pos_x += Q_ONE` step).
fn lower_alu_imm12(
    out: &mut Vec<VInst>,
    temps: &mut TempVRegs,
    dst: VReg,
    src: VReg,
    imm: i32,
    imm_op: AluImmOp,
    rrr_op: AluOp,
    src_op: u16,
) {
    if fits_imm12(imm) {
        out.push(VInst::AluRRI {
            op: imm_op,
            dst,
            src,
            imm,
            src_op,
        });
    } else {
        let scratch = temps.mint();
        out.push(VInst::IConst32 {
            dst: scratch,
            val: imm,
            src_op,
        });
        out.push(VInst::AluRRR {
            op: rrr_op,
            dst,
            src1: src,
            src2: scratch,
            src_op,
        });
    }
}

fn sym_call(
    out: &mut Vec<VInst>,
    symbols: &mut ModuleSymbols,
    pool: &mut Vec<VReg>,
    name: &'static str,
    args: &[lpir::VReg],
    rets: &[lpir::VReg],
    src_op: Option<u32>,
) -> Result<(), LowerError> {
    out.push(VInst::Call {
        target: symbols.intern(name),
        args: push_vregs_slice(pool, args)?,
        rets: push_vregs_slice(pool, rets)?,
        callee_uses_sret: false,
        caller_passes_sret_ptr: false,
        caller_sret_vm_abi_swap: false,
        src_op: pack_src_op(src_op),
    });
    Ok(())
}

/// Lower one LPIR op. `src_op` is the index in [`IrFunction::body`].
pub fn lower_lpir_op(
    out: &mut Vec<VInst>,
    op: &LpirOp,
    opts: &LowerOpts<'_>,
    src_op: Option<u32>,
    func: &IrFunction,
    ir: &LpirModule,
    abi: &ModuleAbi,
    symbols: &mut ModuleSymbols,
    vreg_pool: &mut Vec<VReg>,
    temps: &mut TempVRegs,
) -> Result<(), LowerError> {
    let po = pack_src_op(src_op);
    match op {
        LpirOp::Iadd { dst, lhs, rhs } => {
            out.push(VInst::AluRRR {
                op: AluOp::Add,
                dst: fa_vreg(*dst),
                src1: fa_vreg(*lhs),
                src2: fa_vreg(*rhs),
                src_op: po,
            });
            Ok(())
        }
        LpirOp::Isub { dst, lhs, rhs } => {
            out.push(VInst::AluRRR {
                op: AluOp::Sub,
                dst: fa_vreg(*dst),
                src1: fa_vreg(*lhs),
                src2: fa_vreg(*rhs),
                src_op: po,
            });
            Ok(())
        }
        LpirOp::Imul { dst, lhs, rhs } => {
            out.push(VInst::AluRRR {
                op: AluOp::Mul,
                dst: fa_vreg(*dst),
                src1: fa_vreg(*lhs),
                src2: fa_vreg(*rhs),
                src_op: po,
            });
            Ok(())
        }
        LpirOp::IdivS { dst, lhs, rhs } => {
            out.push(VInst::AluRRR {
                op: AluOp::DivS,
                dst: fa_vreg(*dst),
                src1: fa_vreg(*lhs),
                src2: fa_vreg(*rhs),
                src_op: po,
            });
            Ok(())
        }
        LpirOp::IdivU { dst, lhs, rhs } => {
            out.push(VInst::AluRRR {
                op: AluOp::DivU,
                dst: fa_vreg(*dst),
                src1: fa_vreg(*lhs),
                src2: fa_vreg(*rhs),
                src_op: po,
            });
            Ok(())
        }
        LpirOp::IremS { dst, lhs, rhs } => {
            out.push(VInst::AluRRR {
                op: AluOp::RemS,
                dst: fa_vreg(*dst),
                src1: fa_vreg(*lhs),
                src2: fa_vreg(*rhs),
                src_op: po,
            });
            Ok(())
        }
        LpirOp::IremU { dst, lhs, rhs } => {
            out.push(VInst::AluRRR {
                op: AluOp::RemU,
                dst: fa_vreg(*dst),
                src1: fa_vreg(*lhs),
                src2: fa_vreg(*rhs),
                src_op: po,
            });
            Ok(())
        }
        LpirOp::Ineg { dst, src } => {
            out.push(VInst::Neg {
                dst: fa_vreg(*dst),
                src: fa_vreg(*src),
                src_op: po,
            });
            Ok(())
        }
        LpirOp::Ieq { dst, lhs, rhs } => {
            out.push(VInst::Icmp {
                dst: fa_vreg(*dst),
                lhs: fa_vreg(*lhs),
                rhs: fa_vreg(*rhs),
                cond: IcmpCond::Eq,
                src_op: po,
            });
            Ok(())
        }
        LpirOp::Ine { dst, lhs, rhs } => {
            out.push(VInst::Icmp {
                dst: fa_vreg(*dst),
                lhs: fa_vreg(*lhs),
                rhs: fa_vreg(*rhs),
                cond: IcmpCond::Ne,
                src_op: po,
            });
            Ok(())
        }
        LpirOp::IltS { dst, lhs, rhs } => {
            out.push(VInst::Icmp {
                dst: fa_vreg(*dst),
                lhs: fa_vreg(*lhs),
                rhs: fa_vreg(*rhs),
                cond: IcmpCond::LtS,
                src_op: po,
            });
            Ok(())
        }
        LpirOp::IleS { dst, lhs, rhs } => {
            out.push(VInst::Icmp {
                dst: fa_vreg(*dst),
                lhs: fa_vreg(*lhs),
                rhs: fa_vreg(*rhs),
                cond: IcmpCond::LeS,
                src_op: po,
            });
            Ok(())
        }
        LpirOp::IgtS { dst, lhs, rhs } => {
            out.push(VInst::Icmp {
                dst: fa_vreg(*dst),
                lhs: fa_vreg(*lhs),
                rhs: fa_vreg(*rhs),
                cond: IcmpCond::GtS,
                src_op: po,
            });
            Ok(())
        }
        LpirOp::IgeS { dst, lhs, rhs } => {
            out.push(VInst::Icmp {
                dst: fa_vreg(*dst),
                lhs: fa_vreg(*lhs),
                rhs: fa_vreg(*rhs),
                cond: IcmpCond::GeS,
                src_op: po,
            });
            Ok(())
        }
        LpirOp::IltU { dst, lhs, rhs } => {
            out.push(VInst::Icmp {
                dst: fa_vreg(*dst),
                lhs: fa_vreg(*lhs),
                rhs: fa_vreg(*rhs),
                cond: IcmpCond::LtU,
                src_op: po,
            });
            Ok(())
        }
        LpirOp::IleU { dst, lhs, rhs } => {
            out.push(VInst::Icmp {
                dst: fa_vreg(*dst),
                lhs: fa_vreg(*lhs),
                rhs: fa_vreg(*rhs),
                cond: IcmpCond::LeU,
                src_op: po,
            });
            Ok(())
        }
        LpirOp::IgtU { dst, lhs, rhs } => {
            out.push(VInst::Icmp {
                dst: fa_vreg(*dst),
                lhs: fa_vreg(*lhs),
                rhs: fa_vreg(*rhs),
                cond: IcmpCond::GtU,
                src_op: po,
            });
            Ok(())
        }
        LpirOp::IgeU { dst, lhs, rhs } => {
            out.push(VInst::Icmp {
                dst: fa_vreg(*dst),
                lhs: fa_vreg(*lhs),
                rhs: fa_vreg(*rhs),
                cond: IcmpCond::GeU,
                src_op: po,
            });
            Ok(())
        }
        LpirOp::IeqImm { dst, src, imm } => {
            out.push(VInst::IcmpImm {
                dst: fa_vreg(*dst),
                src: fa_vreg(*src),
                imm: *imm,
                cond: IcmpCond::Eq,
                src_op: po,
            });
            Ok(())
        }
        LpirOp::IaddImm { dst, src, imm } => {
            lower_alu_imm12(
                out,
                temps,
                fa_vreg(*dst),
                fa_vreg(*src),
                *imm,
                AluImmOp::Addi,
                AluOp::Add,
                po,
            );
            Ok(())
        }
        LpirOp::IsubImm { dst, src, imm } => {
            // Try to fold into `addi rd, rs, -imm` (matches the wider RV32
            // immediate range; e.g. `imm == 2048` does not fit imm12 but
            // `-imm == -2048` does). Fall back to materializing `imm` and
            // emitting an R-form `sub` when neither form fits, including the
            // `imm == i32::MIN` case where `-imm` overflows.
            let neg = imm.checked_neg().filter(|n| fits_imm12(*n));
            if let Some(neg) = neg {
                out.push(VInst::AluRRI {
                    op: AluImmOp::Addi,
                    dst: fa_vreg(*dst),
                    src: fa_vreg(*src),
                    imm: neg,
                    src_op: po,
                });
            } else {
                let scratch = temps.mint();
                out.push(VInst::IConst32 {
                    dst: scratch,
                    val: *imm,
                    src_op: po,
                });
                out.push(VInst::AluRRR {
                    op: AluOp::Sub,
                    dst: fa_vreg(*dst),
                    src1: fa_vreg(*src),
                    src2: scratch,
                    src_op: po,
                });
            }
            Ok(())
        }
        LpirOp::ImulImm { dst, src, imm } => {
            // RV32M has no `muli`; always materialize then `mul`.
            let scratch = temps.mint();
            out.push(VInst::IConst32 {
                dst: scratch,
                val: *imm,
                src_op: po,
            });
            out.push(VInst::AluRRR {
                op: AluOp::Mul,
                dst: fa_vreg(*dst),
                src1: fa_vreg(*src),
                src2: scratch,
                src_op: po,
            });
            Ok(())
        }
        LpirOp::IshlImm { dst, src, imm } => {
            out.push(VInst::AluRRI {
                op: AluImmOp::Slli,
                dst: fa_vreg(*dst),
                src: fa_vreg(*src),
                imm: *imm,
                src_op: po,
            });
            Ok(())
        }
        LpirOp::IshrSImm { dst, src, imm } => {
            out.push(VInst::AluRRI {
                op: AluImmOp::SraiS,
                dst: fa_vreg(*dst),
                src: fa_vreg(*src),
                imm: *imm,
                src_op: po,
            });
            Ok(())
        }
        LpirOp::IshrUImm { dst, src, imm } => {
            out.push(VInst::AluRRI {
                op: AluImmOp::SrliU,
                dst: fa_vreg(*dst),
                src: fa_vreg(*src),
                imm: *imm,
                src_op: po,
            });
            Ok(())
        }
        LpirOp::Iand { dst, lhs, rhs } => {
            out.push(VInst::AluRRR {
                op: AluOp::And,
                dst: fa_vreg(*dst),
                src1: fa_vreg(*lhs),
                src2: fa_vreg(*rhs),
                src_op: po,
            });
            Ok(())
        }
        LpirOp::Ior { dst, lhs, rhs } => {
            out.push(VInst::AluRRR {
                op: AluOp::Or,
                dst: fa_vreg(*dst),
                src1: fa_vreg(*lhs),
                src2: fa_vreg(*rhs),
                src_op: po,
            });
            Ok(())
        }
        LpirOp::Ixor { dst, lhs, rhs } => {
            out.push(VInst::AluRRR {
                op: AluOp::Xor,
                dst: fa_vreg(*dst),
                src1: fa_vreg(*lhs),
                src2: fa_vreg(*rhs),
                src_op: po,
            });
            Ok(())
        }
        LpirOp::Ibnot { dst, src } => {
            out.push(VInst::Bnot {
                dst: fa_vreg(*dst),
                src: fa_vreg(*src),
                src_op: po,
            });
            Ok(())
        }
        LpirOp::Ishl { dst, lhs, rhs } => {
            out.push(VInst::AluRRR {
                op: AluOp::Sll,
                dst: fa_vreg(*dst),
                src1: fa_vreg(*lhs),
                src2: fa_vreg(*rhs),
                src_op: po,
            });
            Ok(())
        }
        LpirOp::IshrS { dst, lhs, rhs } => {
            out.push(VInst::AluRRR {
                op: AluOp::SraS,
                dst: fa_vreg(*dst),
                src1: fa_vreg(*lhs),
                src2: fa_vreg(*rhs),
                src_op: po,
            });
            Ok(())
        }
        LpirOp::IshrU { dst, lhs, rhs } => {
            out.push(VInst::AluRRR {
                op: AluOp::SrlU,
                dst: fa_vreg(*dst),
                src1: fa_vreg(*lhs),
                src2: fa_vreg(*rhs),
                src_op: po,
            });
            Ok(())
        }
        LpirOp::Select {
            dst,
            cond,
            if_true,
            if_false,
        } => {
            out.push(VInst::Select {
                dst: fa_vreg(*dst),
                cond: fa_vreg(*cond),
                if_true: fa_vreg(*if_true),
                if_false: fa_vreg(*if_false),
                src_op: po,
            });
            Ok(())
        }
        LpirOp::Copy { dst, src } => {
            out.push(VInst::Mov {
                dst: fa_vreg(*dst),
                src: fa_vreg(*src),
                src_op: po,
            });
            Ok(())
        }
        LpirOp::IconstI32 { dst, value } => {
            out.push(VInst::IConst32 {
                dst: fa_vreg(*dst),
                val: *value,
                src_op: po,
            });
            Ok(())
        }

        LpirOp::Load { dst, base, offset } => {
            let off = i32::try_from(*offset).map_err(|_| LowerError::UnsupportedOp {
                description: String::from("Load: offset does not fit i32"),
            })?;
            out.push(VInst::Load32 {
                dst: fa_vreg(*dst),
                base: fa_vreg(*base),
                offset: off,
                src_op: po,
            });
            Ok(())
        }
        LpirOp::Store {
            base,
            offset,
            value,
        } => {
            let off = i32::try_from(*offset).map_err(|_| LowerError::UnsupportedOp {
                description: String::from("Store: offset does not fit i32"),
            })?;
            out.push(VInst::Store32 {
                src: fa_vreg(*value),
                base: fa_vreg(*base),
                offset: off,
                src_op: po,
            });
            Ok(())
        }
        LpirOp::Store8 {
            base,
            offset,
            value,
        } => {
            let off = i32::try_from(*offset).map_err(|_| LowerError::UnsupportedOp {
                description: String::from("Store8: offset does not fit i32"),
            })?;
            out.push(VInst::Store8 {
                src: fa_vreg(*value),
                base: fa_vreg(*base),
                offset: off,
                src_op: po,
            });
            Ok(())
        }
        LpirOp::Store16 {
            base,
            offset,
            value,
        } => {
            let off = i32::try_from(*offset).map_err(|_| LowerError::UnsupportedOp {
                description: String::from("Store16: offset does not fit i32"),
            })?;
            out.push(VInst::Store16 {
                src: fa_vreg(*value),
                base: fa_vreg(*base),
                offset: off,
                src_op: po,
            });
            Ok(())
        }
        LpirOp::Load8U { dst, base, offset } => {
            let off = i32::try_from(*offset).map_err(|_| LowerError::UnsupportedOp {
                description: String::from("Load8U: offset does not fit i32"),
            })?;
            out.push(VInst::Load8U {
                dst: fa_vreg(*dst),
                base: fa_vreg(*base),
                offset: off,
                src_op: po,
            });
            Ok(())
        }
        LpirOp::Load8S { dst, base, offset } => {
            let off = i32::try_from(*offset).map_err(|_| LowerError::UnsupportedOp {
                description: String::from("Load8S: offset does not fit i32"),
            })?;
            out.push(VInst::Load8S {
                dst: fa_vreg(*dst),
                base: fa_vreg(*base),
                offset: off,
                src_op: po,
            });
            Ok(())
        }
        LpirOp::Load16U { dst, base, offset } => {
            let off = i32::try_from(*offset).map_err(|_| LowerError::UnsupportedOp {
                description: String::from("Load16U: offset does not fit i32"),
            })?;
            out.push(VInst::Load16U {
                dst: fa_vreg(*dst),
                base: fa_vreg(*base),
                offset: off,
                src_op: po,
            });
            Ok(())
        }
        LpirOp::Load16S { dst, base, offset } => {
            let off = i32::try_from(*offset).map_err(|_| LowerError::UnsupportedOp {
                description: String::from("Load16S: offset does not fit i32"),
            })?;
            out.push(VInst::Load16S {
                dst: fa_vreg(*dst),
                base: fa_vreg(*base),
                offset: off,
                src_op: po,
            });
            Ok(())
        }
        LpirOp::SlotAddr { dst, slot } => {
            out.push(VInst::SlotAddr {
                dst: fa_vreg(*dst),
                slot: slot.0,
                src_op: po,
            });
            Ok(())
        }
        LpirOp::Memcpy {
            dst_addr,
            src_addr,
            size,
        } => {
            if size % 4 != 0 {
                return Err(LowerError::UnsupportedOp {
                    description: String::from("Memcpy: size must be a multiple of 4"),
                });
            }
            out.push(VInst::MemcpyWords {
                dst_base: fa_vreg(*dst_addr),
                src_base: fa_vreg(*src_addr),
                size: *size,
                src_op: po,
            });
            Ok(())
        }

        LpirOp::Return { values } => {
            let slice = func.pool_slice(*values);
            if slice.len() != values.count as usize {
                return Err(LowerError::UnsupportedOp {
                    description: String::from("Return: vreg_pool slice out of range"),
                });
            }
            out.push(VInst::Ret {
                vals: push_vregs_slice(vreg_pool, slice)?,
                src_op: po,
            });
            Ok(())
        }

        LpirOp::Fadd { dst, lhs, rhs } if opts.float_mode == FloatMode::Q32 => {
            match opts.q32.add_sub {
                AddSubMode::Saturating => sym_call(
                    out,
                    symbols,
                    vreg_pool,
                    BuiltinId::LpLpirFaddQ32.name(),
                    &[*lhs, *rhs],
                    &[*dst],
                    src_op,
                ),
                AddSubMode::Wrapping => {
                    out.push(VInst::AluRRR {
                        op: AluOp::Add,
                        dst: fa_vreg(*dst),
                        src1: fa_vreg(*lhs),
                        src2: fa_vreg(*rhs),
                        src_op: po,
                    });
                    Ok(())
                }
            }
        }
        LpirOp::Fsub { dst, lhs, rhs } if opts.float_mode == FloatMode::Q32 => {
            match opts.q32.add_sub {
                AddSubMode::Saturating => sym_call(
                    out,
                    symbols,
                    vreg_pool,
                    BuiltinId::LpLpirFsubQ32.name(),
                    &[*lhs, *rhs],
                    &[*dst],
                    src_op,
                ),
                AddSubMode::Wrapping => {
                    out.push(VInst::AluRRR {
                        op: AluOp::Sub,
                        dst: fa_vreg(*dst),
                        src1: fa_vreg(*lhs),
                        src2: fa_vreg(*rhs),
                        src_op: po,
                    });
                    Ok(())
                }
            }
        }
        LpirOp::Fmul { dst, lhs, rhs } if opts.float_mode == FloatMode::Q32 => match opts.q32.mul {
            MulMode::Saturating => sym_call(
                out,
                symbols,
                vreg_pool,
                BuiltinId::LpLpirFmulQ32.name(),
                &[*lhs, *rhs],
                &[*dst],
                src_op,
            ),
            MulMode::Wrapping => {
                let a = fa_vreg(*lhs);
                let b = fa_vreg(*rhs);
                let dstv = fa_vreg(*dst);
                let lo = temps.mint();
                let hi = temps.mint();
                out.push(VInst::AluRRR {
                    op: AluOp::Mul,
                    dst: lo,
                    src1: a,
                    src2: b,
                    src_op: po,
                });
                out.push(VInst::AluRRR {
                    op: AluOp::MulH,
                    dst: hi,
                    src1: a,
                    src2: b,
                    src_op: po,
                });
                out.push(VInst::AluRRI {
                    op: AluImmOp::SrliU,
                    dst: lo,
                    src: lo,
                    imm: 16,
                    src_op: po,
                });
                out.push(VInst::AluRRI {
                    op: AluImmOp::Slli,
                    dst: hi,
                    src: hi,
                    imm: 16,
                    src_op: po,
                });
                out.push(VInst::AluRRR {
                    op: AluOp::Or,
                    dst: dstv,
                    src1: lo,
                    src2: hi,
                    src_op: po,
                });
                Ok(())
            }
        },
        LpirOp::Fdiv { dst, lhs, rhs } if opts.float_mode == FloatMode::Q32 => {
            let helper = match opts.q32.div {
                DivMode::Saturating => BuiltinId::LpLpirFdivQ32,
                DivMode::Reciprocal => BuiltinId::LpLpirFdivRecipQ32,
            };
            sym_call(
                out,
                symbols,
                vreg_pool,
                helper.name(),
                &[*lhs, *rhs],
                &[*dst],
                src_op,
            )
        }
        LpirOp::Fneg { dst, src } if opts.float_mode == FloatMode::Q32 => {
            out.push(VInst::Neg {
                dst: fa_vreg(*dst),
                src: fa_vreg(*src),
                src_op: po,
            });
            Ok(())
        }
        LpirOp::ItofS { dst, src } if opts.float_mode == FloatMode::Q32 => sym_call(
            out,
            symbols,
            vreg_pool,
            "__lp_lpir_itof_s_q32",
            &[*src],
            &[*dst],
            src_op,
        ),
        LpirOp::ItofU { dst, src } if opts.float_mode == FloatMode::Q32 => sym_call(
            out,
            symbols,
            vreg_pool,
            "__lp_lpir_itof_u_q32",
            &[*src],
            &[*dst],
            src_op,
        ),

        LpirOp::Feq { dst, lhs, rhs } if opts.float_mode == FloatMode::Q32 => {
            out.push(VInst::Icmp {
                dst: fa_vreg(*dst),
                lhs: fa_vreg(*lhs),
                rhs: fa_vreg(*rhs),
                cond: IcmpCond::Eq,
                src_op: po,
            });
            Ok(())
        }
        LpirOp::Fne { dst, lhs, rhs } if opts.float_mode == FloatMode::Q32 => {
            out.push(VInst::Icmp {
                dst: fa_vreg(*dst),
                lhs: fa_vreg(*lhs),
                rhs: fa_vreg(*rhs),
                cond: IcmpCond::Ne,
                src_op: po,
            });
            Ok(())
        }
        LpirOp::Flt { dst, lhs, rhs } if opts.float_mode == FloatMode::Q32 => {
            out.push(VInst::Icmp {
                dst: fa_vreg(*dst),
                lhs: fa_vreg(*lhs),
                rhs: fa_vreg(*rhs),
                cond: IcmpCond::LtS,
                src_op: po,
            });
            Ok(())
        }
        LpirOp::Fle { dst, lhs, rhs } if opts.float_mode == FloatMode::Q32 => {
            out.push(VInst::Icmp {
                dst: fa_vreg(*dst),
                lhs: fa_vreg(*lhs),
                rhs: fa_vreg(*rhs),
                cond: IcmpCond::LeS,
                src_op: po,
            });
            Ok(())
        }
        LpirOp::Fgt { dst, lhs, rhs } if opts.float_mode == FloatMode::Q32 => {
            out.push(VInst::Icmp {
                dst: fa_vreg(*dst),
                lhs: fa_vreg(*lhs),
                rhs: fa_vreg(*rhs),
                cond: IcmpCond::GtS,
                src_op: po,
            });
            Ok(())
        }
        LpirOp::Fge { dst, lhs, rhs } if opts.float_mode == FloatMode::Q32 => {
            out.push(VInst::Icmp {
                dst: fa_vreg(*dst),
                lhs: fa_vreg(*lhs),
                rhs: fa_vreg(*rhs),
                cond: IcmpCond::GeS,
                src_op: po,
            });
            Ok(())
        }

        // Q32 float constants: convert f32 to Q32 fixed-point (multiply by 65536.0)
        LpirOp::FconstF32 { dst, value } if opts.float_mode == FloatMode::Q32 => {
            let q32_val = ((*value as f64) * 65536.0) as i32;
            out.push(VInst::IConst32 {
                dst: fa_vreg(*dst),
                val: q32_val,
                src_op: po,
            });
            Ok(())
        }

        LpirOp::Fsqrt { dst, src } if opts.float_mode == FloatMode::Q32 => sym_call(
            out,
            symbols,
            vreg_pool,
            "__lp_lpir_fsqrt_q32",
            &[*src],
            &[*dst],
            src_op,
        ),
        LpirOp::Fnearest { dst, src } if opts.float_mode == FloatMode::Q32 => sym_call(
            out,
            symbols,
            vreg_pool,
            "__lp_lpir_fnearest_q32",
            &[*src],
            &[*dst],
            src_op,
        ),
        LpirOp::Fabs { dst, src } if opts.float_mode == FloatMode::Q32 => {
            // Branchless abs matching `__lp_lpir_fabs_q32` / `wrapping_neg` (incl. i32::MIN).
            let mask = temps.mint();
            out.push(VInst::AluRRI {
                op: AluImmOp::SraiS,
                dst: mask,
                src: fa_vreg(*src),
                imm: 31,
                src_op: po,
            });
            let tmp = temps.mint();
            out.push(VInst::AluRRR {
                op: AluOp::Xor,
                dst: tmp,
                src1: fa_vreg(*src),
                src2: mask,
                src_op: po,
            });
            out.push(VInst::AluRRR {
                op: AluOp::Sub,
                dst: fa_vreg(*dst),
                src1: tmp,
                src2: mask,
                src_op: po,
            });
            Ok(())
        }
        LpirOp::Fmin { dst, lhs, rhs } if opts.float_mode == FloatMode::Q32 => {
            let cmp = temps.mint();
            out.push(VInst::Icmp {
                dst: cmp,
                lhs: fa_vreg(*lhs),
                rhs: fa_vreg(*rhs),
                cond: IcmpCond::LtS,
                src_op: po,
            });
            out.push(VInst::Select {
                dst: fa_vreg(*dst),
                cond: cmp,
                if_true: fa_vreg(*lhs),
                if_false: fa_vreg(*rhs),
                src_op: po,
            });
            Ok(())
        }
        LpirOp::Fmax { dst, lhs, rhs } if opts.float_mode == FloatMode::Q32 => {
            let cmp = temps.mint();
            out.push(VInst::Icmp {
                dst: cmp,
                lhs: fa_vreg(*lhs),
                rhs: fa_vreg(*rhs),
                cond: IcmpCond::GtS,
                src_op: po,
            });
            out.push(VInst::Select {
                dst: fa_vreg(*dst),
                cond: cmp,
                if_true: fa_vreg(*lhs),
                if_false: fa_vreg(*rhs),
                src_op: po,
            });
            Ok(())
        }
        LpirOp::Ffloor { dst, src } if opts.float_mode == FloatMode::Q32 => sym_call(
            out,
            symbols,
            vreg_pool,
            "__lp_lpir_ffloor_q32",
            &[*src],
            &[*dst],
            src_op,
        ),
        LpirOp::Fceil { dst, src } if opts.float_mode == FloatMode::Q32 => sym_call(
            out,
            symbols,
            vreg_pool,
            "__lp_lpir_fceil_q32",
            &[*src],
            &[*dst],
            src_op,
        ),
        LpirOp::Ftrunc { dst, src } if opts.float_mode == FloatMode::Q32 => sym_call(
            out,
            symbols,
            vreg_pool,
            "__lp_lpir_ftrunc_q32",
            &[*src],
            &[*dst],
            src_op,
        ),
        LpirOp::FtoiSatS { dst, src } if opts.float_mode == FloatMode::Q32 => sym_call(
            out,
            symbols,
            vreg_pool,
            "__lp_lpir_ftoi_sat_s_q32",
            &[*src],
            &[*dst],
            src_op,
        ),
        LpirOp::FtoiSatU { dst, src } if opts.float_mode == FloatMode::Q32 => sym_call(
            out,
            symbols,
            vreg_pool,
            "__lp_lpir_ftoi_sat_u_q32",
            &[*src],
            &[*dst],
            src_op,
        ),
        LpirOp::FfromI32Bits { dst, src } if opts.float_mode == FloatMode::Q32 => {
            out.push(VInst::Mov {
                dst: fa_vreg(*dst),
                src: fa_vreg(*src),
                src_op: po,
            });
            Ok(())
        }
        LpirOp::FtoUnorm16 { dst, src } if opts.float_mode == FloatMode::Q32 => {
            let zero = temps.mint();
            out.push(VInst::IConst32 {
                dst: zero,
                val: 0,
                src_op: po,
            });

            let cmp_lo = temps.mint();
            out.push(VInst::Icmp {
                dst: cmp_lo,
                lhs: fa_vreg(*src),
                rhs: zero,
                cond: IcmpCond::LtS,
                src_op: po,
            });
            let lo = temps.mint();
            out.push(VInst::Select {
                dst: lo,
                cond: cmp_lo,
                if_true: zero,
                if_false: fa_vreg(*src),
                src_op: po,
            });

            let cap = temps.mint();
            out.push(VInst::IConst32 {
                dst: cap,
                val: 65535,
                src_op: po,
            });
            let cmp_hi = temps.mint();
            out.push(VInst::Icmp {
                dst: cmp_hi,
                lhs: lo,
                rhs: cap,
                cond: IcmpCond::GtS,
                src_op: po,
            });
            out.push(VInst::Select {
                dst: fa_vreg(*dst),
                cond: cmp_hi,
                if_true: cap,
                if_false: lo,
                src_op: po,
            });
            Ok(())
        }
        LpirOp::FtoUnorm8 { dst, src } if opts.float_mode == FloatMode::Q32 => {
            let shifted = temps.mint();
            out.push(VInst::AluRRI {
                op: AluImmOp::SraiS,
                dst: shifted,
                src: fa_vreg(*src),
                imm: 8,
                src_op: po,
            });

            let zero = temps.mint();
            out.push(VInst::IConst32 {
                dst: zero,
                val: 0,
                src_op: po,
            });

            let cmp_lo = temps.mint();
            out.push(VInst::Icmp {
                dst: cmp_lo,
                lhs: shifted,
                rhs: zero,
                cond: IcmpCond::LtS,
                src_op: po,
            });
            let lo = temps.mint();
            out.push(VInst::Select {
                dst: lo,
                cond: cmp_lo,
                if_true: zero,
                if_false: shifted,
                src_op: po,
            });

            let cap = temps.mint();
            out.push(VInst::IConst32 {
                dst: cap,
                val: 255,
                src_op: po,
            });
            let cmp_hi = temps.mint();
            out.push(VInst::Icmp {
                dst: cmp_hi,
                lhs: lo,
                rhs: cap,
                cond: IcmpCond::GtS,
                src_op: po,
            });
            out.push(VInst::Select {
                dst: fa_vreg(*dst),
                cond: cmp_hi,
                if_true: cap,
                if_false: lo,
                src_op: po,
            });
            Ok(())
        }
        LpirOp::Unorm16toF { dst, src } if opts.float_mode == FloatMode::Q32 => {
            let mask = temps.mint();
            out.push(VInst::IConst32 {
                dst: mask,
                val: 0xFFFF,
                src_op: po,
            });
            out.push(VInst::AluRRR {
                op: AluOp::And,
                dst: fa_vreg(*dst),
                src1: fa_vreg(*src),
                src2: mask,
                src_op: po,
            });
            Ok(())
        }
        LpirOp::Unorm8toF { dst, src } if opts.float_mode == FloatMode::Q32 => {
            let masked = temps.mint();
            out.push(VInst::AluRRI {
                op: AluImmOp::Andi,
                dst: masked,
                src: fa_vreg(*src),
                imm: 0xFF,
                src_op: po,
            });
            out.push(VInst::AluRRI {
                op: AluImmOp::Slli,
                dst: fa_vreg(*dst),
                src: masked,
                imm: 8,
                src_op: po,
            });
            Ok(())
        }

        LpirOp::Fadd { .. }
        | LpirOp::Fsub { .. }
        | LpirOp::Fmul { .. }
        | LpirOp::Fdiv { .. }
        | LpirOp::Fneg { .. }
        | LpirOp::FconstF32 { .. }
        | LpirOp::Fsqrt { .. }
        | LpirOp::Fnearest { .. }
        | LpirOp::Fabs { .. }
        | LpirOp::Fmin { .. }
        | LpirOp::Fmax { .. }
        | LpirOp::Ffloor { .. }
        | LpirOp::Fceil { .. }
        | LpirOp::Ftrunc { .. }
        | LpirOp::Feq { .. }
        | LpirOp::Fne { .. }
        | LpirOp::Flt { .. }
        | LpirOp::Fle { .. }
        | LpirOp::Fgt { .. }
        | LpirOp::Fge { .. }
        | LpirOp::ItofS { .. }
        | LpirOp::ItofU { .. }
        | LpirOp::FtoiSatS { .. }
        | LpirOp::FtoiSatU { .. }
        | LpirOp::FfromI32Bits { .. }
        | LpirOp::FtoUnorm16 { .. }
        | LpirOp::FtoUnorm8 { .. }
        | LpirOp::Unorm16toF { .. }
        | LpirOp::Unorm8toF { .. } => Err(LowerError::UnsupportedOp {
            description: String::from("float op requires Q32 mode (F32 not supported on rv32c)"),
        }),

        LpirOp::IfStart { .. }
        | LpirOp::Else
        | LpirOp::End
        | LpirOp::LoopStart { .. }
        | LpirOp::Block { .. }
        | LpirOp::ExitBlock => Err(LowerError::UnsupportedOp {
            description: String::from(
                "structural control-flow op must be lowered via lower_ops (IfStart/LoopStart/Block/Else/End/ExitBlock)",
            ),
        }),
        LpirOp::Break | LpirOp::Continue | LpirOp::BrIfNot { .. } => {
            Err(LowerError::UnsupportedOp {
                description: String::from(
                    "break/continue/br_if_not must be lowered via lower_ops with loop context",
                ),
            })
        }

        LpirOp::Call {
            callee,
            args,
            results,
        } => {
            let name =
                resolve_callee_name(ir, *callee).ok_or_else(|| LowerError::UnsupportedOp {
                    description: format!("Call: callee index out of range ({callee:?})"),
                })?;
            let args_slice = func.pool_slice(*args);
            if args_slice.len() != args.count as usize {
                return Err(LowerError::UnsupportedOp {
                    description: String::from("Call: args vreg_pool slice out of range"),
                });
            }
            let results_slice = func.pool_slice(*results);
            if results_slice.len() != results.count as usize {
                return Err(LowerError::UnsupportedOp {
                    description: String::from("Call: results vreg_pool slice out of range"),
                });
            }
            let callee_uses_sret = callee_return_uses_sret(ir, abi, *callee);
            let caller_passes_sret_ptr = callee_sret_ptr_in_lpir_args(ir, *callee);
            let caller_sret_vm_abi_swap =
                caller_passes_sret_ptr && callee_sret_vm_abi_swap(ir, *callee);
            out.push(VInst::Call {
                target: symbols.intern(name),
                args: push_vregs_slice(vreg_pool, args_slice)?,
                rets: push_vregs_slice(vreg_pool, results_slice)?,
                callee_uses_sret,
                caller_passes_sret_ptr,
                caller_sret_vm_abi_swap,
                src_op: po,
            });
            Ok(())
        }

        other => Err(LowerError::UnsupportedOp {
            description: format!("{other:?}"),
        }),
    }
}

/// Loop frame for tracking loop control flow targets.
struct LoopFrame {
    /// Label for the continue block (target of `Continue`).
    continuing: LabelId,
    /// Label after the loop (target of `Break` and exit-false of `BrIfNot`).
    exit: LabelId,
}

/// A loop region in the linearised VInst stream: `[header_idx, backedge_idx]`.
#[derive(Clone, Debug)]
pub struct LoopRegion {
    /// VInst index of the `Label(header)`.
    pub header_idx: usize,
    /// VInst index of the `Br { target: header }` back-edge.
    pub backedge_idx: usize,
}

/// Result of lowering: the VInst stream plus loop boundary metadata.
pub struct LoweredFunction {
    pub vinsts: Vec<VInst>,
    pub vreg_pool: Vec<VReg>,
    pub symbols: ModuleSymbols,
    pub loop_regions: Vec<LoopRegion>,
    pub region_tree: RegionTree,
    /// LPIR slot sizes: `(slot_id, size_in_bytes)` for frame layout.
    pub lpir_slots: Vec<(u32, u32)>,
}

struct LowerCtx<'a> {
    func: &'a IrFunction,
    ir: &'a LpirModule,
    abi: &'a ModuleAbi,
    lower_opts: &'a LowerOpts<'a>,
    out: Vec<VInst>,
    vreg_pool: Vec<VReg>,
    symbols: ModuleSymbols,
    temps: TempVRegs,
    next_label: LabelId,
    loop_stack: Vec<LoopFrame>,
    epilogue_label: LabelId,
    loop_regions: Vec<LoopRegion>,
    region_tree: RegionTree,
    /// Target labels for in-flight [`LpirOp::Block`] regions (for [`LpirOp::ExitBlock`]).
    block_exit_stack: Vec<LabelId>,
}

impl<'a> LowerCtx<'a> {
    fn alloc_label(&mut self) -> LabelId {
        let id = self.next_label;
        self.next_label = self.next_label.wrapping_add(1);
        id
    }

    /// Lower a range of LPIR ops, building VInsts and returning a RegionId.
    /// The returned region covers all VInsts emitted during this call.
    fn lower_range(&mut self, start: usize, end: usize) -> Result<RegionId, LowerError> {
        use crate::region::Region;
        use alloc::vec::Vec;

        let mut i = start;
        let mut seq: Vec<RegionId> = Vec::new();
        let mut current_linear_start: Option<u16> = None;

        while i < end {
            // Record start position before processing this op
            let vinst_start = self.out.len() as u16;

            // Helper: flush current linear region
            let flush_linear = |seq: &mut Vec<RegionId>,
                                tree: &mut RegionTree,
                                start: &mut Option<u16>,
                                force_end: u16| {
                if let Some(s) = start.take() {
                    let e = force_end;
                    if s < e {
                        let id = tree.push(Region::Linear { start: s, end: e });
                        seq.push(id);
                    }
                }
            };

            match &self.func.body[i] {
                LpirOp::IfStart {
                    cond,
                    else_offset,
                    end_offset,
                } => {
                    // Flush any accumulated linear region before the if
                    flush_linear(
                        &mut seq,
                        &mut self.region_tree,
                        &mut current_linear_start,
                        vinst_start,
                    );

                    let eo = *else_offset as usize;
                    let merge_after = *end_offset as usize;
                    let else_is_empty = matches!(self.func.body.get(eo), Some(LpirOp::End));

                    if else_is_empty {
                        // if (cond) { then } ; merge label (same VInst order as lpvm-native)
                        let merge = self.alloc_label();

                        let head_start = self.out.len() as u16;
                        self.out.push(VInst::BrIf {
                            cond: fa_vreg(*cond),
                            target: merge,
                            invert: true,
                            src_op: pack_src_op(Some(i as u32)),
                        });
                        let head_end = self.out.len() as u16;
                        let head = self.region_tree.push(Region::Linear {
                            start: head_start,
                            end: head_end,
                        });

                        let then_inner = self.lower_range(i + 1, eo)?;
                        let br_merge_s = self.out.len() as u16;
                        self.out.push(VInst::Br {
                            target: merge,
                            src_op: pack_src_op(Some(i as u32)),
                        });
                        let br_merge_e = self.out.len() as u16;
                        let br_merge = self.region_tree.push(Region::Linear {
                            start: br_merge_s,
                            end: br_merge_e,
                        });
                        let merge_lbl_s = self.out.len() as u16;
                        self.out
                            .push(VInst::Label(merge, pack_src_op(Some(eo as u32))));
                        let merge_lbl_e = self.out.len() as u16;
                        let merge_lbl = self.region_tree.push(Region::Linear {
                            start: merge_lbl_s,
                            end: merge_lbl_e,
                        });
                        let then_body = self
                            .region_tree
                            .push_seq(&[then_inner, br_merge, merge_lbl]);
                        let else_body = REGION_ID_NONE;

                        let if_region = self.region_tree.push(Region::IfThenElse {
                            head,
                            then_body,
                            else_body,
                            else_label: merge,
                            merge_label: merge,
                        });
                        seq.push(if_region);
                    } else {
                        // if (cond) { then } else { else } ; end label
                        let else_label = self.alloc_label();
                        let end_label = self.alloc_label();

                        let head_start = self.out.len() as u16;
                        self.out.push(VInst::BrIf {
                            cond: fa_vreg(*cond),
                            target: else_label,
                            invert: true,
                            src_op: pack_src_op(Some(i as u32)),
                        });
                        let head_end = self.out.len() as u16;
                        let head = self.region_tree.push(Region::Linear {
                            start: head_start,
                            end: head_end,
                        });

                        let then_inner = self.lower_range(i + 1, eo)?;
                        let br_end_s = self.out.len() as u16;
                        self.out.push(VInst::Br {
                            target: end_label,
                            src_op: pack_src_op(Some(i as u32)),
                        });
                        let br_end_e = self.out.len() as u16;
                        let br_end = self.region_tree.push(Region::Linear {
                            start: br_end_s,
                            end: br_end_e,
                        });
                        let then_body = self.region_tree.push_seq(&[then_inner, br_end]);

                        let else_lbl_s = self.out.len() as u16;
                        self.out
                            .push(VInst::Label(else_label, pack_src_op(Some(*else_offset))));
                        let else_lbl_e = self.out.len() as u16;
                        let else_lbl = self.region_tree.push(Region::Linear {
                            start: else_lbl_s,
                            end: else_lbl_e,
                        });
                        let else_inner = self.lower_range(eo + 1, merge_after)?;
                        let end_lbl_s = self.out.len() as u16;
                        let end_idx = merge_after.saturating_sub(1);
                        self.out
                            .push(VInst::Label(end_label, pack_src_op(Some(end_idx as u32))));
                        let end_lbl_e = self.out.len() as u16;
                        let end_lbl = self.region_tree.push(Region::Linear {
                            start: end_lbl_s,
                            end: end_lbl_e,
                        });
                        let else_body = self.region_tree.push_seq(&[else_lbl, else_inner, end_lbl]);

                        let if_region = self.region_tree.push(Region::IfThenElse {
                            head,
                            then_body,
                            else_body,
                            else_label,
                            merge_label: end_label,
                        });
                        seq.push(if_region);
                    }
                    i = merge_after;
                    current_linear_start = Some(self.out.len() as u16);
                }
                LpirOp::LoopStart {
                    continuing_offset,
                    end_offset,
                } => {
                    // Flush any accumulated linear region before the loop
                    flush_linear(
                        &mut seq,
                        &mut self.region_tree,
                        &mut current_linear_start,
                        vinst_start,
                    );

                    // Same VInst / label layout as lpvm-native (emitter resolves branches).
                    let header_label = self.alloc_label();
                    let continuing = self.alloc_label();
                    let exit = self.alloc_label();

                    let pre_start = self.out.len() as u16;
                    self.out.push(VInst::Br {
                        target: header_label,
                        src_op: pack_src_op(Some(i as u32)),
                    });
                    let header_lbl_idx = self.out.len() as u16;
                    self.out.push(VInst::Label(
                        header_label,
                        pack_src_op(Some((i + 1) as u32)),
                    ));
                    let pre_end = self.out.len() as u16;
                    let header = self.region_tree.push(Region::Linear {
                        start: pre_start,
                        end: pre_end,
                    });

                    self.loop_stack.push(LoopFrame { continuing, exit });
                    let co = *continuing_offset as usize;
                    let eo = *end_offset as usize;

                    let main_body = self.lower_range(i + 1, co)?;

                    let cont_seq = if co < eo {
                        let ls = self.out.len() as u16;
                        self.out.push(VInst::Label(
                            continuing,
                            pack_src_op(Some(*continuing_offset)),
                        ));
                        let le = self.out.len() as u16;
                        let label_r = self.region_tree.push(Region::Linear { start: ls, end: le });
                        let cont_inner = self.lower_range(co, eo.saturating_sub(1))?;
                        Some(self.region_tree.push_seq(&[label_r, cont_inner]))
                    } else {
                        None
                    };

                    let backedge_idx = self.out.len() as u16;
                    self.out.push(VInst::Br {
                        target: header_label,
                        src_op: pack_src_op(Some((eo.saturating_sub(1)) as u32)),
                    });
                    let backedge_end = self.out.len() as u16;
                    let backedge = self.region_tree.push(Region::Linear {
                        start: backedge_idx,
                        end: backedge_end,
                    });

                    let exit_ls = self.out.len() as u16;
                    self.out
                        .push(VInst::Label(exit, pack_src_op(Some(*end_offset))));
                    let exit_le = self.out.len() as u16;
                    let exit_lin = self.region_tree.push(Region::Linear {
                        start: exit_ls,
                        end: exit_le,
                    });

                    self.loop_regions.push(LoopRegion {
                        header_idx: header_lbl_idx as usize,
                        backedge_idx: backedge_idx as usize,
                    });
                    self.loop_stack.pop();

                    let mut inner: Vec<RegionId> = Vec::new();
                    if main_body != REGION_ID_NONE {
                        inner.push(main_body);
                    }
                    if let Some(c) = cont_seq {
                        inner.push(c);
                    }
                    inner.push(backedge);
                    let body = if inner.is_empty() {
                        backedge
                    } else if inner.len() == 1 {
                        inner[0]
                    } else {
                        self.region_tree.push_seq(&inner)
                    };

                    let loop_region = self.region_tree.push(Region::Loop {
                        header,
                        body,
                        header_label,
                        exit_label: exit,
                    });
                    seq.push(loop_region);
                    seq.push(exit_lin);

                    i = eo;
                    current_linear_start = Some(self.out.len() as u16);
                }
                LpirOp::Block { end_offset } => {
                    flush_linear(
                        &mut seq,
                        &mut self.region_tree,
                        &mut current_linear_start,
                        vinst_start,
                    );
                    let eo = *end_offset as usize;
                    let exit_lbl = self.alloc_label();
                    self.block_exit_stack.push(exit_lbl);
                    let body_inner = self.lower_range(i + 1, eo)?;
                    let popped = self.block_exit_stack.pop();
                    debug_assert_eq!(popped, Some(exit_lbl));

                    let exit_ls = self.out.len() as u16;
                    self.out
                        .push(VInst::Label(exit_lbl, pack_src_op(Some(eo as u32))));
                    let exit_le = self.out.len() as u16;
                    let exit_lin = self.region_tree.push(Region::Linear {
                        start: exit_ls,
                        end: exit_le,
                    });
                    let block_reg = self.region_tree.push(Region::Block {
                        body: body_inner,
                        exit_label: exit_lbl,
                    });
                    seq.push(block_reg);
                    seq.push(exit_lin);
                    i = eo;
                    current_linear_start = Some(self.out.len() as u16);
                }
                LpirOp::ExitBlock => {
                    let exit_lbl =
                        *self
                            .block_exit_stack
                            .last()
                            .ok_or_else(|| LowerError::UnsupportedOp {
                                description: String::from("exit_block outside block"),
                            })?;
                    if current_linear_start.is_none() {
                        current_linear_start = Some(self.out.len() as u16);
                    }
                    self.out.push(VInst::Br {
                        target: exit_lbl,
                        src_op: pack_src_op(Some(i as u32)),
                    });
                    i += 1;
                }
                LpirOp::Break => {
                    let frame =
                        self.loop_stack
                            .last()
                            .ok_or_else(|| LowerError::UnsupportedOp {
                                description: String::from("break outside loop"),
                            })?;
                    if current_linear_start.is_none() {
                        current_linear_start = Some(self.out.len() as u16);
                    }
                    self.out.push(VInst::Br {
                        target: frame.exit,
                        src_op: pack_src_op(Some(i as u32)),
                    });
                    i += 1;
                }
                LpirOp::Continue => {
                    let frame =
                        self.loop_stack
                            .last()
                            .ok_or_else(|| LowerError::UnsupportedOp {
                                description: String::from("continue outside loop"),
                            })?;
                    if current_linear_start.is_none() {
                        current_linear_start = Some(self.out.len() as u16);
                    }
                    self.out.push(VInst::Br {
                        target: frame.continuing,
                        src_op: pack_src_op(Some(i as u32)),
                    });
                    i += 1;
                }
                LpirOp::BrIfNot { cond } => {
                    let frame =
                        self.loop_stack
                            .last()
                            .ok_or_else(|| LowerError::UnsupportedOp {
                                description: String::from("br_if_not outside loop"),
                            })?;
                    if current_linear_start.is_none() {
                        current_linear_start = Some(self.out.len() as u16);
                    }
                    self.out.push(VInst::BrIf {
                        cond: fa_vreg(*cond),
                        target: frame.exit,
                        invert: true,
                        src_op: pack_src_op(Some(i as u32)),
                    });
                    i += 1;
                }
                LpirOp::Else | LpirOp::End => {
                    i += 1;
                }
                other => {
                    if let LpirOp::Copy { dst, src } = other {
                        if dst == src {
                            i += 1;
                            continue;
                        }
                    }

                    // Start a linear region if not already tracking
                    if current_linear_start.is_none() {
                        current_linear_start = Some(self.out.len() as u16);
                    }

                    let is_return = matches!(other, LpirOp::Return { .. });
                    lower_lpir_op(
                        &mut self.out,
                        other,
                        self.lower_opts,
                        Some(i as u32),
                        self.func,
                        self.ir,
                        self.abi,
                        &mut self.symbols,
                        &mut self.vreg_pool,
                        &mut self.temps,
                    )?;
                    if is_return {
                        self.out.push(VInst::Br {
                            target: self.epilogue_label,
                            src_op: pack_src_op(Some(i as u32)),
                        });
                    }
                    i += 1;
                }
            }
        }

        // Flush final linear region
        if let Some(start) = current_linear_start.take() {
            let end = self.out.len() as u16;
            if start < end {
                let id = self.region_tree.push(Region::Linear { start, end });
                seq.push(id);
            }
        }

        // Return result
        if seq.is_empty() {
            Ok(REGION_ID_NONE)
        } else if seq.len() == 1 {
            Ok(seq[0])
        } else {
            Ok(self.region_tree.push_seq(&seq))
        }
    }
}

/// Lower full function body (including if/else and loop control flow).
pub fn lower_ops(
    func: &IrFunction,
    ir: &LpirModule,
    abi: &ModuleAbi,
    opts: &LowerOpts<'_>,
) -> Result<LoweredFunction, LowerError> {
    // Pre-size vectors to reduce allocation overhead during lowering.
    // Estimate: ~2 vinsts per LPIR op, vreg pool from IR plus headroom for temps.
    let mut ctx = LowerCtx {
        func,
        ir,
        abi,
        lower_opts: opts,
        out: Vec::with_capacity(func.body.len().saturating_mul(2)),
        vreg_pool: Vec::with_capacity(func.vreg_pool.len().saturating_add(64)),
        symbols: ModuleSymbols::default(),
        temps: TempVRegs::new(func.vreg_types.len() as u16),
        next_label: 0,
        loop_stack: Vec::new(),
        epilogue_label: 0,
        loop_regions: Vec::new(),
        region_tree: RegionTree::with_capacity(func.body.len()),
        block_exit_stack: Vec::new(),
    };
    ctx.epilogue_label = ctx.alloc_label();
    let root = ctx.lower_range(0, func.body.len())?;
    ctx.out.push(VInst::Label(ctx.epilogue_label, SRC_OP_NONE));
    ctx.region_tree.root = root;
    // Collect LPIR slot sizes for frame layout
    let lpir_slots: Vec<(u32, u32)> = ctx
        .func
        .slots
        .iter()
        .enumerate()
        .map(|(id, decl)| (id as u32, decl.size))
        .collect();

    Ok(LoweredFunction {
        vinsts: ctx.out,
        vreg_pool: ctx.vreg_pool,
        symbols: ctx.symbols,
        loop_regions: ctx.loop_regions,
        region_tree: ctx.region_tree,
        lpir_slots,
    })
}

fn resolve_callee_name(ir: &LpirModule, callee: CalleeRef) -> Option<String> {
    if let Some(idx) = ir.callee_as_import(callee) {
        ir.imports.get(idx).map(|imp| {
            if let Some(bid) = resolve_import_to_builtin(imp) {
                String::from(bid.name())
            } else {
                imp.func_name.clone()
            }
        })
    } else if let Some(f) = ir.callee_as_function(callee) {
        Some(f.name.clone())
    } else {
        None
    }
}

/// Map an LPIR import declaration to a BuiltinId to get the C ABI symbol name.
/// Mirrors Cranelift's resolve_import in lpvm-cranelift/src/builtins.rs
fn resolve_import_to_builtin(decl: &lpir::lpir_module::ImportDecl) -> Option<BuiltinId> {
    match decl.module_name.as_str() {
        "glsl" => {
            let ac = decl.param_types.len();
            glsl_q32_math_builtin_id(&decl.func_name, ac)
        }
        "lpir" => {
            let ac = decl.param_types.len();
            lpir_q32_builtin_id(&decl.func_name, ac)
        }
        "lpfn" => {
            // LPFX builtins are named like "lpfn_psrdnoise_34" - strip the suffix
            let base = lpfn_strip_suffix(&decl.func_name)?;
            // Get GLSL kinds from lpfn_glsl_params CSV or fall back to IR types
            let kinds = lpfn_glsl_kinds_from_decl(decl);
            glsl_lpfn_q32_builtin_id(base, &kinds)
        }
        "vm" => {
            let ac = decl.param_types.len();
            vm_q32_builtin_id(&decl.func_name, ac)
        }
        "texture" => {
            let base = texture_strip_suffix(&decl.func_name);
            let ac = decl.param_types.len();
            texture_q32_builtin_id(base, ac)
        }
        _ => None,
    }
}

/// Strip the numeric suffix from LPFX import names (e.g., "lpfn_psrdnoise_34" → "lpfn_psrdnoise").
fn lpfn_strip_suffix(func_name: &str) -> Option<&str> {
    let (base, tail) = func_name.rsplit_once('_')?;
    tail.parse::<u32>().ok()?;
    Some(base)
}

fn texture_strip_suffix(func_name: &str) -> &str {
    let Some((base, tail)) = func_name.rsplit_once('_') else {
        return func_name;
    };
    if tail.parse::<u32>().is_ok() {
        base
    } else {
        func_name
    }
}

/// Get GLSL parameter kinds from lpfn_glsl_params CSV or infer from IR types.
fn lpfn_glsl_kinds_from_decl(decl: &lpir::lpir_module::ImportDecl) -> Vec<GlslParamKind> {
    if let Some(ref enc) = decl.lpfn_glsl_params {
        parse_lpfn_glsl_params_csv(enc)
            .unwrap_or_else(|_| ir_params_to_glsl_kinds(&decl.param_types))
    } else {
        ir_params_to_glsl_kinds(&decl.param_types)
    }
}

/// Parse LPFX glsl params CSV (e.g., "Vec2,Vec2,Float,Vec2,UInt").
fn parse_lpfn_glsl_params_csv(enc: &str) -> Result<Vec<GlslParamKind>, String> {
    if enc.is_empty() {
        return Ok(Vec::new());
    }
    enc.split(',')
        .map(|t| match t.trim() {
            "Float" => Ok(GlslParamKind::Float),
            "Int" => Ok(GlslParamKind::Int),
            "UInt" => Ok(GlslParamKind::UInt),
            "Vec2" => Ok(GlslParamKind::Vec2),
            "Vec3" => Ok(GlslParamKind::Vec3),
            "Vec4" => Ok(GlslParamKind::Vec4),
            "IVec2" => Ok(GlslParamKind::IVec2),
            "IVec3" => Ok(GlslParamKind::IVec3),
            "IVec4" => Ok(GlslParamKind::IVec4),
            "UVec2" => Ok(GlslParamKind::UVec2),
            "UVec3" => Ok(GlslParamKind::UVec3),
            "UVec4" => Ok(GlslParamKind::UVec4),
            "BVec2" => Ok(GlslParamKind::BVec2),
            "BVec3" => Ok(GlslParamKind::BVec3),
            "BVec4" => Ok(GlslParamKind::BVec4),
            other => Err(format!("unknown LPFX glsl param tag `{other}`")),
        })
        .collect()
}

/// Convert LPIR parameter types to GLSL parameter kinds for LPFX overload resolution.
fn ir_params_to_glsl_kinds(params: &[lpir::IrType]) -> Vec<GlslParamKind> {
    use lpir::IrType;
    params
        .iter()
        .map(|t| match t {
            IrType::F32 => GlslParamKind::Float,
            IrType::I32 | IrType::Pointer => GlslParamKind::UInt,
        })
        .collect()
}

fn callee_return_uses_sret(ir: &LpirModule, abi: &ModuleAbi, callee: CalleeRef) -> bool {
    let isa = abi.isa();
    if let Some(imp_idx) = ir.callee_as_import(callee) {
        let imp = &ir.imports[imp_idx];
        return imp.sret || isa.sret_uses_buffer_for(imp.return_types.len() as u32);
    }
    let Some(f) = ir.callee_as_function(callee) else {
        return false;
    };
    if f.sret_arg.is_some() {
        return true;
    }
    if let Some(fa) = abi.func_abi(f.name.as_str()) {
        fa.is_sret()
    } else {
        isa.sret_uses_buffer_for(f.return_types.len() as u32)
    }
}

fn callee_sret_ptr_in_lpir_args(ir: &LpirModule, callee: CalleeRef) -> bool {
    if let Some(imp_idx) = ir.callee_as_import(callee) {
        ir.imports[imp_idx].sret
    } else if let Some(f) = ir.callee_as_function(callee) {
        f.sret_arg.is_some()
    } else {
        false
    }
}

/// When the caller passes an explicit sret pointer in [`LpirOp::Call`], RV32 can map it two ways.
/// Returns true when LPIR passes `vmctx` before the output pointer (`ImportDecl::needs_vmctx`),
/// and for user functions with `IrFunction::sret_arg`.
/// `@texture::*` imports omit vmctx (`needs_vmctx == false`); the first arg is the sret destination
/// and maps to `a0` without swapping.
fn callee_sret_vm_abi_swap(ir: &LpirModule, callee: CalleeRef) -> bool {
    if let Some(imp_idx) = ir.callee_as_import(callee) {
        return ir.imports[imp_idx].needs_vmctx;
    }
    ir.callee_as_function(callee)
        .is_some_and(|f| f.sret_arg.is_some())
}

#[cfg(test)]
mod tests {
    use alloc::string::String;
    use alloc::vec;

    use super::*;
    use crate::error::LowerError;
    use crate::vinst::{
        AluImmOp, IcmpCond, ModuleSymbols, TempVRegs, VReg as FaVReg, unpack_src_op,
    };
    use lps_q32::q32_options::Q32Options;

    fn call_lower_op_with_q32(
        op: &LpirOp,
        float_mode: FloatMode,
        q32: &Q32Options,
        src_op: Option<u32>,
        f: &IrFunction,
        ir: &LpirModule,
        abi: &ModuleAbi,
    ) -> Result<Vec<VInst>, LowerError> {
        let opts = LowerOpts { float_mode, q32 };
        let mut out = Vec::new();
        let mut symbols = ModuleSymbols::default();
        let mut pool = Vec::new();
        let mut temps = TempVRegs::new(f.vreg_types.len() as u16);
        super::lower_lpir_op(
            &mut out,
            op,
            &opts,
            src_op,
            f,
            ir,
            abi,
            &mut symbols,
            &mut pool,
            &mut temps,
        )?;
        Ok(out)
    }

    fn call_lower_op(
        op: &LpirOp,
        float_mode: FloatMode,
        src_op: Option<u32>,
        f: &IrFunction,
        ir: &LpirModule,
        abi: &ModuleAbi,
    ) -> Result<Vec<VInst>, LowerError> {
        let q32 = Q32Options::default();
        call_lower_op_with_q32(op, float_mode, &q32, src_op, f, ir, abi)
    }

    fn call_lower_op_full_q32(
        op: &LpirOp,
        float_mode: FloatMode,
        q32: &Q32Options,
        src_op: Option<u32>,
        f: &IrFunction,
        ir: &LpirModule,
        abi: &ModuleAbi,
    ) -> Result<(Vec<VInst>, ModuleSymbols, Vec<FaVReg>), LowerError> {
        let opts = LowerOpts { float_mode, q32 };
        let mut out = Vec::new();
        let mut symbols = ModuleSymbols::default();
        let mut pool = Vec::new();
        let mut temps = TempVRegs::new(f.vreg_types.len() as u16);
        super::lower_lpir_op(
            &mut out,
            op,
            &opts,
            src_op,
            f,
            ir,
            abi,
            &mut symbols,
            &mut pool,
            &mut temps,
        )?;
        Ok((out, symbols, pool))
    }

    fn call_lower_op_full(
        op: &LpirOp,
        float_mode: FloatMode,
        src_op: Option<u32>,
        f: &IrFunction,
        ir: &LpirModule,
        abi: &ModuleAbi,
    ) -> Result<(Vec<VInst>, ModuleSymbols, Vec<FaVReg>), LowerError> {
        let q32 = Q32Options::default();
        call_lower_op_full_q32(op, float_mode, &q32, src_op, f, ir, abi)
    }
    use lpir::types::{SlotId, VRegRange};
    use lpir::{IrType, LpirModule, VReg as IrVReg};
    use lps_shared::LpsModuleSig;

    fn empty_ir_abi() -> (LpirModule, ModuleAbi) {
        let ir = LpirModule::default();
        let abi = ModuleAbi::from_ir_and_sig(
            crate::isa::IsaTarget::Rv32imac,
            &ir,
            &LpsModuleSig::default(),
        );
        (ir, abi)
    }

    fn v(n: u32) -> IrVReg {
        IrVReg(n)
    }

    fn empty_func() -> IrFunction {
        IrFunction {
            name: String::new(),
            is_entry: true,
            vmctx_vreg: IrVReg(0),
            param_count: 0,
            return_types: vec![],
            sret_arg: None,
            vreg_types: vec![],
            slots: vec![],
            body: vec![],
            vreg_pool: vec![],
        }
    }

    #[test]
    fn lower_iadd() {
        let op = LpirOp::Iadd {
            dst: v(2),
            lhs: v(0),
            rhs: v(1),
        };
        let f = empty_func();
        let (ir, abi) = empty_ir_abi();
        let v = call_lower_op(&op, FloatMode::Q32, Some(0), &f, &ir, &abi).expect("ok");
        assert_eq!(v.len(), 1, "single VInst (sym_call or trivial inline)");
        assert!(matches!(
            &v[0],
            VInst::AluRRR { op: AluOp::Add,
                dst: FaVReg(2),
                src1: FaVReg(0),
                src2: FaVReg(1),
                src_op,
            } if unpack_src_op(*src_op) == Some(0)
        ));
    }

    #[test]
    fn lower_load_store_slot_memcpy() {
        let f = empty_func();
        let (ir, abi) = empty_ir_abi();
        let load = LpirOp::Load {
            dst: v(3),
            base: v(2),
            offset: 4,
        };
        {
            let v = call_lower_op(&load, FloatMode::Q32, None, &f, &ir, &abi).expect("load");
            assert_eq!(v.len(), 1, "single VInst (sym_call or trivial inline)");
            assert!(matches!(
                &v[0],
                VInst::Load32 {
                    dst: FaVReg(3),
                    base: FaVReg(2),
                    offset: 4,
                    ..
                }
            ));
        }
        let store = LpirOp::Store {
            base: v(2),
            offset: 8,
            value: v(3),
        };
        {
            let v = call_lower_op(&store, FloatMode::Q32, None, &f, &ir, &abi).expect("store");
            assert_eq!(v.len(), 1, "single VInst (sym_call or trivial inline)");
            assert!(matches!(
                &v[0],
                VInst::Store32 {
                    src: FaVReg(3),
                    base: FaVReg(2),
                    offset: 8,
                    ..
                }
            ));
        }
        let st8 = LpirOp::Store8 {
            base: v(2),
            offset: 1,
            value: v(3),
        };
        {
            let v = call_lower_op(&st8, FloatMode::Q32, None, &f, &ir, &abi).expect("store8");
            assert_eq!(v.len(), 1, "single VInst (sym_call or trivial inline)");
            assert!(matches!(
                &v[0],
                VInst::Store8 {
                    src: FaVReg(3),
                    base: FaVReg(2),
                    offset: 1,
                    ..
                }
            ));
        }
        let l8u = LpirOp::Load8U {
            dst: v(3),
            base: v(2),
            offset: 5,
        };
        {
            let v = call_lower_op(&l8u, FloatMode::Q32, None, &f, &ir, &abi).expect("load8u");
            assert_eq!(v.len(), 1, "single VInst (sym_call or trivial inline)");
            assert!(matches!(
                &v[0],
                VInst::Load8U {
                    dst: FaVReg(3),
                    base: FaVReg(2),
                    offset: 5,
                    ..
                }
            ));
        }
        let st16 = LpirOp::Store16 {
            base: v(2),
            offset: 2,
            value: v(3),
        };
        {
            let v = call_lower_op(&st16, FloatMode::Q32, None, &f, &ir, &abi).expect("store16");
            assert_eq!(v.len(), 1, "single VInst (sym_call or trivial inline)");
            assert!(matches!(
                &v[0],
                VInst::Store16 {
                    src: FaVReg(3),
                    base: FaVReg(2),
                    offset: 2,
                    ..
                }
            ));
        }
        let l8s = LpirOp::Load8S {
            dst: v(3),
            base: v(2),
            offset: 6,
        };
        {
            let v = call_lower_op(&l8s, FloatMode::Q32, None, &f, &ir, &abi).expect("load8s");
            assert_eq!(v.len(), 1, "single VInst (sym_call or trivial inline)");
            assert!(matches!(
                &v[0],
                VInst::Load8S {
                    dst: FaVReg(3),
                    base: FaVReg(2),
                    offset: 6,
                    ..
                }
            ));
        }
        let l16u = LpirOp::Load16U {
            dst: v(3),
            base: v(2),
            offset: 7,
        };
        {
            let v = call_lower_op(&l16u, FloatMode::Q32, None, &f, &ir, &abi).expect("load16u");
            assert_eq!(v.len(), 1, "single VInst (sym_call or trivial inline)");
            assert!(matches!(
                &v[0],
                VInst::Load16U {
                    dst: FaVReg(3),
                    base: FaVReg(2),
                    offset: 7,
                    ..
                }
            ));
        }
        let l16s = LpirOp::Load16S {
            dst: v(3),
            base: v(2),
            offset: 9,
        };
        {
            let v = call_lower_op(&l16s, FloatMode::Q32, None, &f, &ir, &abi).expect("load16s");
            assert_eq!(v.len(), 1, "single VInst (sym_call or trivial inline)");
            assert!(matches!(
                &v[0],
                VInst::Load16S {
                    dst: FaVReg(3),
                    base: FaVReg(2),
                    offset: 9,
                    ..
                }
            ));
        }
        let sa = LpirOp::SlotAddr {
            dst: v(1),
            slot: SlotId(0),
        };
        {
            let v = call_lower_op(&sa, FloatMode::Q32, None, &f, &ir, &abi).expect("slot_addr");
            assert_eq!(v.len(), 1, "single VInst (sym_call or trivial inline)");
            assert!(matches!(
                &v[0],
                VInst::SlotAddr {
                    dst: FaVReg(1),
                    slot: 0,
                    ..
                }
            ));
        }
        let mc = LpirOp::Memcpy {
            dst_addr: v(4),
            src_addr: v(5),
            size: 12,
        };
        {
            let v = call_lower_op(&mc, FloatMode::Q32, None, &f, &ir, &abi).expect("memcpy");
            assert_eq!(v.len(), 1, "single VInst (sym_call or trivial inline)");
            assert!(matches!(
                &v[0],
                VInst::MemcpyWords {
                    dst_base: FaVReg(4),
                    src_base: FaVReg(5),
                    size: 12,
                    ..
                }
            ));
        }
    }

    #[test]
    fn lower_q32_fadd_to_call() {
        let op = LpirOp::Fadd {
            dst: v(2),
            lhs: v(0),
            rhs: v(1),
        };
        let f = empty_func();
        let (ir, abi) = empty_ir_abi();
        let (v, symbols, pool) =
            call_lower_op_full(&op, FloatMode::Q32, Some(3), &f, &ir, &abi).expect("ok");
        assert_eq!(v.len(), 1, "single VInst (sym_call or trivial inline)");
        match &v[0] {
            VInst::Call {
                target,
                args,
                rets,
                callee_uses_sret,
                caller_passes_sret_ptr,
                caller_sret_vm_abi_swap,
                src_op,
            } => {
                assert_eq!(symbols.name(*target), "__lp_lpir_fadd_q32");
                assert_eq!(args.vregs(&pool), &[FaVReg(0), FaVReg(1)]);
                assert_eq!(rets.vregs(&pool), &[FaVReg(2)]);
                assert!(!callee_uses_sret);
                assert!(!caller_passes_sret_ptr);
                assert!(!caller_sret_vm_abi_swap);
                assert_eq!(unpack_src_op(*src_op), Some(3));
            }
            other => panic!("expected Call, got {other:?}"),
        }
    }

    #[test]
    fn lower_q32_fdiv_to_call() {
        let op = LpirOp::Fdiv {
            dst: v(2),
            lhs: v(0),
            rhs: v(1),
        };
        let f = empty_func();
        let (ir, abi) = empty_ir_abi();
        let (v, symbols, pool) =
            call_lower_op_full(&op, FloatMode::Q32, Some(0), &f, &ir, &abi).expect("ok");
        assert_eq!(v.len(), 1, "single VInst (sym_call or trivial inline)");
        match &v[0] {
            VInst::Call {
                target,
                args,
                rets,
                callee_uses_sret,
                caller_passes_sret_ptr,
                caller_sret_vm_abi_swap,
                src_op,
            } => {
                assert_eq!(symbols.name(*target), "__lp_lpir_fdiv_q32");
                assert_eq!(args.vregs(&pool), &[FaVReg(0), FaVReg(1)]);
                assert_eq!(rets.vregs(&pool), &[FaVReg(2)]);
                assert!(!callee_uses_sret);
                assert!(!caller_passes_sret_ptr);
                assert!(!caller_sret_vm_abi_swap);
                assert_eq!(unpack_src_op(*src_op), Some(0));
            }
            other => panic!("expected Call, got {other:?}"),
        }
    }

    fn func_three_q32_vregs() -> IrFunction {
        IrFunction {
            name: String::new(),
            is_entry: true,
            vmctx_vreg: IrVReg(0),
            param_count: 0,
            return_types: vec![],
            sret_arg: None,
            vreg_types: vec![IrType::I32, IrType::I32, IrType::I32],
            slots: vec![],
            body: vec![],
            vreg_pool: vec![],
        }
    }

    #[test]
    fn fadd_q32_wrapping_emits_inline_add() {
        let op = LpirOp::Fadd {
            dst: v(2),
            lhs: v(0),
            rhs: v(1),
        };
        let f = func_three_q32_vregs();
        let (ir, abi) = empty_ir_abi();
        let q32 = Q32Options {
            add_sub: lps_q32::q32_options::AddSubMode::Wrapping,
            ..Default::default()
        };
        let v = call_lower_op_with_q32(&op, FloatMode::Q32, &q32, None, &f, &ir, &abi).expect("ok");
        assert_eq!(v.len(), 1);
        assert!(matches!(
            &v[0],
            VInst::AluRRR {
                op: AluOp::Add,
                dst: FaVReg(2),
                src1: FaVReg(0),
                src2: FaVReg(1),
                ..
            }
        ));
    }

    #[test]
    fn fadd_q32_saturating_emits_sym_call() {
        let op = LpirOp::Fadd {
            dst: v(2),
            lhs: v(0),
            rhs: v(1),
        };
        let f = empty_func();
        let (ir, abi) = empty_ir_abi();
        let (v, symbols, _pool) = call_lower_op_full_q32(
            &op,
            FloatMode::Q32,
            &Q32Options::default(),
            None,
            &f,
            &ir,
            &abi,
        )
        .expect("ok");
        assert_eq!(v.len(), 1);
        let VInst::Call { target, .. } = &v[0] else {
            panic!("expected sym_call");
        };
        assert_eq!(symbols.name(*target), BuiltinId::LpLpirFaddQ32.name());
    }

    #[test]
    fn fsub_q32_wrapping_emits_inline_sub() {
        let op = LpirOp::Fsub {
            dst: v(2),
            lhs: v(0),
            rhs: v(1),
        };
        let f = func_three_q32_vregs();
        let (ir, abi) = empty_ir_abi();
        let q32 = Q32Options {
            add_sub: lps_q32::q32_options::AddSubMode::Wrapping,
            ..Default::default()
        };
        let v = call_lower_op_with_q32(&op, FloatMode::Q32, &q32, None, &f, &ir, &abi).expect("ok");
        assert_eq!(v.len(), 1);
        assert!(matches!(
            &v[0],
            VInst::AluRRR {
                op: AluOp::Sub,
                dst: FaVReg(2),
                src1: FaVReg(0),
                src2: FaVReg(1),
                ..
            }
        ));
    }

    #[test]
    fn fsub_q32_saturating_emits_sym_call() {
        let op = LpirOp::Fsub {
            dst: v(2),
            lhs: v(0),
            rhs: v(1),
        };
        let f = empty_func();
        let (ir, abi) = empty_ir_abi();
        let (v, symbols, _pool) = call_lower_op_full_q32(
            &op,
            FloatMode::Q32,
            &Q32Options::default(),
            None,
            &f,
            &ir,
            &abi,
        )
        .expect("ok");
        assert_eq!(v.len(), 1);
        let VInst::Call { target, .. } = &v[0] else {
            panic!("expected sym_call");
        };
        assert_eq!(symbols.name(*target), BuiltinId::LpLpirFsubQ32.name());
    }

    #[test]
    fn fmul_q32_wrapping_emits_5_vinst_sequence() {
        let op = LpirOp::Fmul {
            dst: v(2),
            lhs: v(0),
            rhs: v(1),
        };
        let f = func_three_q32_vregs();
        let (ir, abi) = empty_ir_abi();
        let q32 = Q32Options {
            mul: lps_q32::q32_options::MulMode::Wrapping,
            ..Default::default()
        };
        let out =
            call_lower_op_with_q32(&op, FloatMode::Q32, &q32, None, &f, &ir, &abi).expect("ok");
        let kinds: Vec<&str> = out
            .iter()
            .map(|i| match i {
                VInst::AluRRR { op: AluOp::Mul, .. } => "mul",
                VInst::AluRRR {
                    op: AluOp::MulH, ..
                } => "mulh",
                VInst::AluRRI {
                    op: AluImmOp::SrliU,
                    imm: 16,
                    ..
                } => "srli16",
                VInst::AluRRI {
                    op: AluImmOp::Slli,
                    imm: 16,
                    ..
                } => "slli16",
                VInst::AluRRR { op: AluOp::Or, .. } => "or",
                other => panic!("unexpected vinst {other:?}"),
            })
            .collect();
        assert_eq!(kinds, &["mul", "mulh", "srli16", "slli16", "or"]);
        let dbg = format!("{out:?}");
        assert!(
            dbg.contains("MulH") && dbg.contains("SrliU"),
            "debug sample: {dbg}"
        );
    }

    #[test]
    fn fmul_q32_saturating_emits_sym_call() {
        let op = LpirOp::Fmul {
            dst: v(2),
            lhs: v(0),
            rhs: v(1),
        };
        let f = empty_func();
        let (ir, abi) = empty_ir_abi();
        let (v, symbols, _pool) = call_lower_op_full_q32(
            &op,
            FloatMode::Q32,
            &Q32Options::default(),
            None,
            &f,
            &ir,
            &abi,
        )
        .expect("ok");
        assert_eq!(v.len(), 1);
        let VInst::Call { target, .. } = &v[0] else {
            panic!("expected sym_call");
        };
        assert_eq!(symbols.name(*target), BuiltinId::LpLpirFmulQ32.name());
    }

    #[test]
    fn fdiv_q32_reciprocal_emits_sym_call_to_recip_helper() {
        let op = LpirOp::Fdiv {
            dst: v(2),
            lhs: v(0),
            rhs: v(1),
        };
        let f = empty_func();
        let (ir, abi) = empty_ir_abi();
        let q32 = Q32Options {
            div: lps_q32::q32_options::DivMode::Reciprocal,
            ..Default::default()
        };
        let (v, symbols, _pool) =
            call_lower_op_full_q32(&op, FloatMode::Q32, &q32, None, &f, &ir, &abi).expect("ok");
        assert_eq!(v.len(), 1);
        let VInst::Call { target, .. } = &v[0] else {
            panic!("expected sym_call");
        };
        assert_eq!(symbols.name(*target), BuiltinId::LpLpirFdivRecipQ32.name());
    }

    #[test]
    fn fdiv_q32_saturating_emits_sym_call_to_default() {
        let op = LpirOp::Fdiv {
            dst: v(2),
            lhs: v(0),
            rhs: v(1),
        };
        let f = empty_func();
        let (ir, abi) = empty_ir_abi();
        let (v, symbols, _pool) = call_lower_op_full_q32(
            &op,
            FloatMode::Q32,
            &Q32Options::default(),
            None,
            &f,
            &ir,
            &abi,
        )
        .expect("ok");
        assert_eq!(v.len(), 1);
        let VInst::Call { target, .. } = &v[0] else {
            panic!("expected sym_call");
        };
        assert_eq!(symbols.name(*target), BuiltinId::LpLpirFdivQ32.name());
    }

    #[test]
    fn lower_q32_fneg_to_neg32() {
        let op = LpirOp::Fneg {
            dst: v(1),
            src: v(0),
        };
        let f = empty_func();
        let (ir, abi) = empty_ir_abi();
        let v = call_lower_op(&op, FloatMode::Q32, Some(0), &f, &ir, &abi).expect("ok");
        assert_eq!(v.len(), 1, "single VInst (sym_call or trivial inline)");
        assert!(matches!(
            &v[0],
            VInst::Neg {
                dst: FaVReg(1),
                src: FaVReg(0),
                src_op,
            } if unpack_src_op(*src_op) == Some(0)
        ));
    }

    #[test]
    fn lower_q32_fabs_inlines_srai_xor_sub() {
        let op = LpirOp::Fabs {
            dst: v(1),
            src: v(0),
        };
        let f = IrFunction {
            name: String::new(),
            is_entry: true,
            vmctx_vreg: IrVReg(0),
            param_count: 0,
            return_types: vec![],
            sret_arg: None,
            vreg_types: vec![IrType::I32, IrType::I32],
            slots: vec![],
            body: vec![],
            vreg_pool: vec![],
        };
        let (ir, abi) = empty_ir_abi();
        let v = call_lower_op(&op, FloatMode::Q32, Some(7), &f, &ir, &abi).expect("ok");
        assert_eq!(v.len(), 3, "Fabs Q32 = SraiS + Xor + Sub");
        assert!(
            matches!(
                &v[0],
                VInst::AluRRI {
                    op: AluImmOp::SraiS,
                    imm: 31,
                    src: FaVReg(0),
                    src_op,
                    ..
                } if unpack_src_op(*src_op) == Some(7)
            ),
            "first inst should be sra by 31, got: {:?}",
            v[0]
        );
        let mask = match &v[0] {
            VInst::AluRRI { dst, .. } => *dst,
            _ => unreachable!(),
        };
        assert!(
            matches!(
                &v[1],
                VInst::AluRRR {
                    op: AluOp::Xor,
                    src1: FaVReg(0),
                    src2,
                    src_op,
                    ..
                } if *src2 == mask && unpack_src_op(*src_op) == Some(7)
            ),
            "expected Xor(src, mask), got: {:?}",
            v[1]
        );
        let tmp = match &v[1] {
            VInst::AluRRR { dst, .. } => *dst,
            _ => unreachable!(),
        };
        assert!(
            matches!(
                &v[2],
                VInst::AluRRR {
                    op: AluOp::Sub,
                    src1,
                    src2,
                    dst: FaVReg(1),
                    src_op,
                } if *src1 == tmp && *src2 == mask && unpack_src_op(*src_op) == Some(7)
            ),
            "expected Sub(tmp, mask) -> dst, got: {:?}",
            v[2]
        );
    }

    #[test]
    fn lower_q32_fmin_inlines_icmp_select() {
        let op = LpirOp::Fmin {
            dst: v(2),
            lhs: v(0),
            rhs: v(1),
        };
        let f = IrFunction {
            name: String::new(),
            is_entry: true,
            vmctx_vreg: IrVReg(0),
            param_count: 0,
            return_types: vec![],
            sret_arg: None,
            vreg_types: vec![IrType::I32; 3],
            slots: vec![],
            body: vec![],
            vreg_pool: vec![],
        };
        let (ir, abi) = empty_ir_abi();
        let v = call_lower_op(&op, FloatMode::Q32, Some(0), &f, &ir, &abi).expect("ok");
        assert_eq!(v.len(), 2, "Fmin Q32 = Icmp + Select");
        assert!(matches!(
            &v[0],
            VInst::Icmp {
                cond: IcmpCond::LtS,
                ..
            }
        ));
        assert!(matches!(&v[1], VInst::Select { .. }));
    }

    #[test]
    fn lower_q32_fmax_inlines_icmp_select() {
        let op = LpirOp::Fmax {
            dst: v(2),
            lhs: v(0),
            rhs: v(1),
        };
        let f = IrFunction {
            name: String::new(),
            is_entry: true,
            vmctx_vreg: IrVReg(0),
            param_count: 0,
            return_types: vec![],
            sret_arg: None,
            vreg_types: vec![IrType::I32; 3],
            slots: vec![],
            body: vec![],
            vreg_pool: vec![],
        };
        let (ir, abi) = empty_ir_abi();
        let v = call_lower_op(&op, FloatMode::Q32, Some(0), &f, &ir, &abi).expect("ok");
        assert_eq!(v.len(), 2, "Fmax Q32 = Icmp + Select");
        assert!(matches!(
            &v[0],
            VInst::Icmp {
                cond: IcmpCond::GtS,
                ..
            }
        ));
        assert!(matches!(&v[1], VInst::Select { .. }));
    }

    #[test]
    fn lower_q32_fto_unorm16_inlines_clamp() {
        let op = LpirOp::FtoUnorm16 {
            dst: v(1),
            src: v(0),
        };
        let f = IrFunction {
            name: String::new(),
            is_entry: true,
            vmctx_vreg: IrVReg(0),
            param_count: 0,
            return_types: vec![],
            sret_arg: None,
            vreg_types: vec![IrType::I32; 2],
            slots: vec![],
            body: vec![],
            vreg_pool: vec![],
        };
        let (ir, abi) = empty_ir_abi();
        let v = call_lower_op(&op, FloatMode::Q32, Some(0), &f, &ir, &abi).expect("ok");
        assert_eq!(
            v.len(),
            6,
            "FtoUnorm16 Q32 = IConst32+Icmp+Select+IConst32+Icmp+Select"
        );
        assert!(matches!(&v[0], VInst::IConst32 { val: 0, .. }));
        assert!(matches!(
            &v[1],
            VInst::Icmp {
                cond: IcmpCond::LtS,
                ..
            }
        ));
        assert!(matches!(&v[2], VInst::Select { .. }));
        assert!(matches!(&v[3], VInst::IConst32 { val: 65535, .. }));
        assert!(matches!(
            &v[4],
            VInst::Icmp {
                cond: IcmpCond::GtS,
                ..
            }
        ));
        assert!(matches!(&v[5], VInst::Select { .. }));
    }

    #[test]
    fn lower_q32_fto_unorm8_inlines_shift_clamp() {
        let op = LpirOp::FtoUnorm8 {
            dst: v(1),
            src: v(0),
        };
        let f = IrFunction {
            name: String::new(),
            is_entry: true,
            vmctx_vreg: IrVReg(0),
            param_count: 0,
            return_types: vec![],
            sret_arg: None,
            vreg_types: vec![IrType::I32; 2],
            slots: vec![],
            body: vec![],
            vreg_pool: vec![],
        };
        let (ir, abi) = empty_ir_abi();
        let v = call_lower_op(&op, FloatMode::Q32, Some(0), &f, &ir, &abi).expect("ok");
        assert_eq!(
            v.len(),
            7,
            "FtoUnorm8 Q32 = SraiS + IConst32+Icmp+Select+IConst32+Icmp+Select"
        );
        assert!(matches!(
            &v[0],
            VInst::AluRRI {
                op: AluImmOp::SraiS,
                imm: 8,
                ..
            }
        ));
        assert!(matches!(&v[1], VInst::IConst32 { val: 0, .. }));
        assert!(matches!(
            &v[2],
            VInst::Icmp {
                cond: IcmpCond::LtS,
                ..
            }
        ));
        assert!(matches!(&v[3], VInst::Select { .. }));
        assert!(matches!(&v[4], VInst::IConst32 { val: 255, .. }));
        assert!(matches!(
            &v[5],
            VInst::Icmp {
                cond: IcmpCond::GtS,
                ..
            }
        ));
        assert!(matches!(&v[6], VInst::Select { .. }));
    }

    #[test]
    fn lower_q32_unorm16_to_f_inlines_mask() {
        let op = LpirOp::Unorm16toF {
            dst: v(1),
            src: v(0),
        };
        let f = IrFunction {
            name: String::new(),
            is_entry: true,
            vmctx_vreg: IrVReg(0),
            param_count: 0,
            return_types: vec![],
            sret_arg: None,
            vreg_types: vec![IrType::I32; 2],
            slots: vec![],
            body: vec![],
            vreg_pool: vec![],
        };
        let (ir, abi) = empty_ir_abi();
        let v = call_lower_op(&op, FloatMode::Q32, Some(0), &f, &ir, &abi).expect("ok");
        assert_eq!(v.len(), 2, "Unorm16toF Q32 = IConst32 + And");
        assert!(matches!(&v[0], VInst::IConst32 { val: 0xFFFF, .. }));
        assert!(matches!(&v[1], VInst::AluRRR { op: AluOp::And, .. }));
    }

    #[test]
    fn lower_q32_unorm8_to_f_inlines_andi_slli() {
        let op = LpirOp::Unorm8toF {
            dst: v(1),
            src: v(0),
        };
        let f = IrFunction {
            name: String::new(),
            is_entry: true,
            vmctx_vreg: IrVReg(0),
            param_count: 0,
            return_types: vec![],
            sret_arg: None,
            vreg_types: vec![IrType::I32; 2],
            slots: vec![],
            body: vec![],
            vreg_pool: vec![],
        };
        let (ir, abi) = empty_ir_abi();
        let v = call_lower_op(&op, FloatMode::Q32, Some(0), &f, &ir, &abi).expect("ok");
        assert_eq!(v.len(), 2, "Unorm8toF Q32 = Andi + Slli");
        assert!(matches!(
            &v[0],
            VInst::AluRRI {
                op: AluImmOp::Andi,
                imm: 0xFF,
                ..
            }
        ));
        assert!(matches!(
            &v[1],
            VInst::AluRRI {
                op: AluImmOp::Slli,
                imm: 8,
                ..
            }
        ));
    }

    #[test]
    fn lower_q32_itof_s_to_call() {
        let op = LpirOp::ItofS {
            dst: v(1),
            src: v(0),
        };
        let f = empty_func();
        let (ir, abi) = empty_ir_abi();
        let (v, symbols, pool) =
            call_lower_op_full(&op, FloatMode::Q32, Some(0), &f, &ir, &abi).expect("ok");
        assert_eq!(v.len(), 1, "single VInst (sym_call or trivial inline)");
        match &v[0] {
            VInst::Call {
                target,
                args,
                rets,
                callee_uses_sret,
                caller_passes_sret_ptr,
                caller_sret_vm_abi_swap,
                src_op,
            } => {
                assert_eq!(symbols.name(*target), "__lp_lpir_itof_s_q32");
                assert_eq!(args.vregs(&pool), &[FaVReg(0)]);
                assert_eq!(rets.vregs(&pool), &[FaVReg(1)]);
                assert!(!callee_uses_sret);
                assert!(!caller_passes_sret_ptr);
                assert!(!caller_sret_vm_abi_swap);
                assert_eq!(unpack_src_op(*src_op), Some(0));
            }
            other => panic!("expected Call, got {other:?}"),
        }
    }

    #[test]
    fn lower_q32_itof_u_to_call() {
        let op = LpirOp::ItofU {
            dst: v(1),
            src: v(0),
        };
        let f = empty_func();
        let (ir, abi) = empty_ir_abi();
        let (v, symbols, pool) =
            call_lower_op_full(&op, FloatMode::Q32, Some(0), &f, &ir, &abi).expect("ok");
        assert_eq!(v.len(), 1, "single VInst (sym_call or trivial inline)");
        match &v[0] {
            VInst::Call {
                target,
                args,
                rets,
                callee_uses_sret,
                caller_passes_sret_ptr,
                caller_sret_vm_abi_swap,
                src_op,
            } => {
                assert_eq!(symbols.name(*target), "__lp_lpir_itof_u_q32");
                assert_eq!(args.vregs(&pool), &[FaVReg(0)]);
                assert_eq!(rets.vregs(&pool), &[FaVReg(1)]);
                assert!(!callee_uses_sret);
                assert!(!caller_passes_sret_ptr);
                assert!(!caller_sret_vm_abi_swap);
                assert_eq!(unpack_src_op(*src_op), Some(0));
            }
            other => panic!("expected Call, got {other:?}"),
        }
    }

    #[test]
    fn lower_q32_fsqrt_to_call() {
        let op = LpirOp::Fsqrt {
            dst: v(1),
            src: v(0),
        };
        let f = empty_func();
        let (ir, abi) = empty_ir_abi();
        let (v, symbols, pool) =
            call_lower_op_full(&op, FloatMode::Q32, Some(0), &f, &ir, &abi).expect("ok");
        assert_eq!(v.len(), 1, "single VInst (sym_call or trivial inline)");
        match &v[0] {
            VInst::Call {
                target,
                args,
                rets,
                callee_uses_sret,
                caller_passes_sret_ptr,
                caller_sret_vm_abi_swap,
                src_op,
            } => {
                assert_eq!(symbols.name(*target), "__lp_lpir_fsqrt_q32");
                assert_eq!(args.vregs(&pool), &[FaVReg(0)]);
                assert_eq!(rets.vregs(&pool), &[FaVReg(1)]);
                assert!(!callee_uses_sret);
                assert!(!caller_passes_sret_ptr);
                assert!(!caller_sret_vm_abi_swap);
                assert_eq!(unpack_src_op(*src_op), Some(0));
            }
            other => panic!("expected Call, got {other:?}"),
        }
    }

    #[test]
    fn lower_q32_ffloor_to_call() {
        let op = LpirOp::Ffloor {
            dst: v(1),
            src: v(0),
        };
        let f = empty_func();
        let (ir, abi) = empty_ir_abi();
        let (v, symbols, pool) =
            call_lower_op_full(&op, FloatMode::Q32, Some(0), &f, &ir, &abi).expect("ok");
        assert_eq!(v.len(), 1, "single VInst (sym_call or trivial inline)");
        match &v[0] {
            VInst::Call {
                target,
                args,
                rets,
                src_op,
                ..
            } => {
                assert_eq!(symbols.name(*target), "__lp_lpir_ffloor_q32");
                assert_eq!(args.vregs(&pool), &[FaVReg(0)]);
                assert_eq!(rets.vregs(&pool), &[FaVReg(1)]);
                assert_eq!(unpack_src_op(*src_op), Some(0));
            }
            other => panic!("expected Call, got {other:?}"),
        }
    }

    #[test]
    fn lower_q32_ffrom_i32_bits_to_mov32() {
        let op = LpirOp::FfromI32Bits {
            dst: v(1),
            src: v(0),
        };
        let f = empty_func();
        let (ir, abi) = empty_ir_abi();
        let v = call_lower_op(&op, FloatMode::Q32, Some(2), &f, &ir, &abi).expect("ok");
        assert_eq!(v.len(), 1, "single VInst (sym_call or trivial inline)");
        assert!(matches!(
            &v[0],
            VInst::Mov {
                dst: FaVReg(1),
                src: FaVReg(0),
                src_op,
            } if unpack_src_op(*src_op) == Some(2)
        ));
    }

    #[test]
    fn lower_q32_float_comparisons_to_signed_icmp() {
        let cases = [
            (
                LpirOp::Feq {
                    dst: v(2),
                    lhs: v(0),
                    rhs: v(1),
                },
                IcmpCond::Eq,
            ),
            (
                LpirOp::Fne {
                    dst: v(2),
                    lhs: v(0),
                    rhs: v(1),
                },
                IcmpCond::Ne,
            ),
            (
                LpirOp::Flt {
                    dst: v(2),
                    lhs: v(0),
                    rhs: v(1),
                },
                IcmpCond::LtS,
            ),
            (
                LpirOp::Fle {
                    dst: v(2),
                    lhs: v(0),
                    rhs: v(1),
                },
                IcmpCond::LeS,
            ),
            (
                LpirOp::Fgt {
                    dst: v(2),
                    lhs: v(0),
                    rhs: v(1),
                },
                IcmpCond::GtS,
            ),
            (
                LpirOp::Fge {
                    dst: v(2),
                    lhs: v(0),
                    rhs: v(1),
                },
                IcmpCond::GeS,
            ),
        ];
        for (op, want) in cases {
            assert_q32_fcmp(want, op);
        }
    }

    #[test]
    fn lower_f32_float_unsupported() {
        let op = LpirOp::Fadd {
            dst: v(0),
            lhs: v(1),
            rhs: v(2),
        };
        let f = empty_func();
        let (ir, abi) = empty_ir_abi();
        let err = call_lower_op(&op, FloatMode::F32, None, &f, &ir, &abi).expect_err("F32 float");
        match err {
            LowerError::UnsupportedOp { description } => {
                assert!(
                    description.contains("Q32"),
                    "expected Q32 hint in {description:?}"
                );
            }
        }
        let div = LpirOp::Fdiv {
            dst: v(0),
            lhs: v(1),
            rhs: v(2),
        };
        assert!(matches!(
            call_lower_op(&div, FloatMode::F32, None, &f, &ir, &abi),
            Err(LowerError::UnsupportedOp { .. })
        ));
    }

    #[test]
    fn lower_ineg() {
        let op = LpirOp::Ineg {
            dst: v(1),
            src: v(0),
        };
        let f = empty_func();
        let (ir, abi) = empty_ir_abi();
        let v = call_lower_op(&op, FloatMode::Q32, Some(0), &f, &ir, &abi).expect("ok");
        assert_eq!(v.len(), 1, "single VInst (sym_call or trivial inline)");
        assert!(matches!(
            &v[0],
            VInst::Neg {
                dst: FaVReg(1),
                src: FaVReg(0),
                src_op,
            } if unpack_src_op(*src_op) == Some(0)
        ));
    }

    #[test]
    fn lower_ieq_imm() {
        let op = LpirOp::IeqImm {
            dst: v(1),
            src: v(0),
            imm: 0,
        };
        let f = empty_func();
        let (ir, abi) = empty_ir_abi();
        let v = call_lower_op(&op, FloatMode::Q32, Some(0), &f, &ir, &abi).expect("ok");
        assert_eq!(v.len(), 1, "single VInst (sym_call or trivial inline)");
        assert!(matches!(
            &v[0],
            VInst::IcmpImm {
                dst: FaVReg(1),
                src: FaVReg(0),
                imm: 0,
                cond: IcmpCond::Eq,
                src_op,
            } if unpack_src_op(*src_op) == Some(0)
        ));
    }

    #[test]
    fn lower_iand() {
        let op = LpirOp::Iand {
            dst: v(2),
            lhs: v(0),
            rhs: v(1),
        };
        let f = empty_func();
        let (ir, abi) = empty_ir_abi();
        let v = call_lower_op(&op, FloatMode::Q32, Some(0), &f, &ir, &abi).expect("ok");
        assert_eq!(v.len(), 1, "single VInst (sym_call or trivial inline)");
        assert!(matches!(
            &v[0],
            VInst::AluRRR { op: AluOp::And,
                dst: FaVReg(2),
                src1: FaVReg(0),
                src2: FaVReg(1),
                src_op,
            } if unpack_src_op(*src_op) == Some(0)
        ));
    }

    #[test]
    fn lower_ibnot() {
        let op = LpirOp::Ibnot {
            dst: v(1),
            src: v(0),
        };
        let f = empty_func();
        let (ir, abi) = empty_ir_abi();
        let v = call_lower_op(&op, FloatMode::Q32, Some(0), &f, &ir, &abi).expect("ok");
        assert_eq!(v.len(), 1, "single VInst (sym_call or trivial inline)");
        assert!(matches!(
            &v[0],
            VInst::Bnot {
                dst: FaVReg(1),
                src: FaVReg(0),
                src_op,
            } if unpack_src_op(*src_op) == Some(0)
        ));
    }

    #[test]
    fn lower_idivs() {
        let op = LpirOp::IdivS {
            dst: v(2),
            lhs: v(0),
            rhs: v(1),
        };
        let f = empty_func();
        let (ir, abi) = empty_ir_abi();
        let v = call_lower_op(&op, FloatMode::Q32, Some(0), &f, &ir, &abi).expect("ok");
        assert_eq!(v.len(), 1, "single VInst (sym_call or trivial inline)");
        assert!(matches!(
            &v[0],
            VInst::AluRRR {
                op: AluOp::DivS,
                dst: FaVReg(2),
                src1: FaVReg(0),
                src2: FaVReg(1),
                src_op,
            } if unpack_src_op(*src_op) == Some(0)
        ));
    }

    #[test]
    fn lower_ieq_to_icmp() {
        let op = LpirOp::Ieq {
            dst: v(2),
            lhs: v(0),
            rhs: v(1),
        };
        let f = empty_func();
        let (ir, abi) = empty_ir_abi();
        let v = call_lower_op(&op, FloatMode::Q32, Some(0), &f, &ir, &abi).expect("ok");
        assert_eq!(v.len(), 1, "single VInst (sym_call or trivial inline)");
        match &v[0] {
            VInst::Icmp { cond, .. } => assert_eq!(*cond, IcmpCond::Eq),
            other => panic!("expected Icmp, got {other:?}"),
        }
    }

    #[test]
    fn lower_iltu_to_icmp() {
        let op = LpirOp::IltU {
            dst: v(2),
            lhs: v(0),
            rhs: v(1),
        };
        let f = empty_func();
        let (ir, abi) = empty_ir_abi();
        let v = call_lower_op(&op, FloatMode::Q32, Some(0), &f, &ir, &abi).expect("ok");
        assert_eq!(v.len(), 1, "single VInst (sym_call or trivial inline)");
        match &v[0] {
            VInst::Icmp { cond, .. } => assert_eq!(*cond, IcmpCond::LtU),
            other => panic!("expected Icmp, got {other:?}"),
        }
    }

    #[test]
    fn lower_select() {
        let op = LpirOp::Select {
            dst: v(3),
            cond: v(0),
            if_true: v(1),
            if_false: v(2),
        };
        let f = empty_func();
        let (ir, abi) = empty_ir_abi();
        let v = call_lower_op(&op, FloatMode::Q32, Some(0), &f, &ir, &abi).expect("ok");
        assert_eq!(v.len(), 1, "single VInst (sym_call or trivial inline)");
        match &v[0] {
            VInst::Select {
                dst,
                cond,
                if_true,
                if_false,
                src_op,
            } => {
                assert_eq!(*dst, FaVReg(3));
                assert_eq!(*cond, FaVReg(0));
                assert_eq!(*if_true, FaVReg(1));
                assert_eq!(*if_false, FaVReg(2));
                assert_eq!(unpack_src_op(*src_op), Some(0));
            }
            other => panic!("expected Select, got {other:?}"),
        }
    }

    #[test]
    fn lower_return_uses_vreg_pool() {
        let f = IrFunction {
            name: String::from("f"),
            is_entry: true,
            vmctx_vreg: IrVReg(0),
            param_count: 0,
            return_types: vec![IrType::I32],
            sret_arg: None,
            vreg_types: vec![],
            slots: vec![],
            body: vec![],
            vreg_pool: vec![v(10), v(11)],
        };
        let op = LpirOp::Return {
            values: VRegRange { start: 0, count: 2 },
        };
        let (ir, abi) = empty_ir_abi();
        let (v, _symbols, pool) =
            call_lower_op_full(&op, FloatMode::Q32, Some(1), &f, &ir, &abi).expect("ok");
        assert_eq!(v.len(), 1, "single VInst (sym_call or trivial inline)");
        match &v[0] {
            VInst::Ret { vals, src_op } => {
                assert_eq!(vals.vregs(&pool), &[FaVReg(10), FaVReg(11)]);
                assert_eq!(unpack_src_op(*src_op), Some(1));
            }
            other => panic!("expected Ret, got {other:?}"),
        }
    }

    fn assert_q32_fcmp(want: IcmpCond, op: LpirOp) {
        let f = empty_func();
        let (ir, abi) = empty_ir_abi();
        let v = call_lower_op(&op, FloatMode::Q32, None, &f, &ir, &abi).expect("ok");
        assert_eq!(v.len(), 1, "single VInst (sym_call or trivial inline)");
        match &v[0] {
            VInst::Icmp {
                cond,
                dst: FaVReg(2),
                lhs: FaVReg(0),
                rhs: FaVReg(1),
                ..
            } => assert_eq!(*cond, want),
            other => panic!("expected Icmp, got {other:?}"),
        }
    }

    #[test]
    fn lower_ops_populates_region_tree() {
        use crate::region::{REGION_ID_NONE, Region};
        use alloc::collections::BTreeMap;
        use lpir::FuncId;
        use lpir::types::VRegRange;

        // Build a simple function: return 42
        // v0 = vmctx
        // v1 = iconst 42
        // ret v1
        let func = IrFunction {
            name: String::from("test"),
            is_entry: true,
            vmctx_vreg: IrVReg(0),
            param_count: 0,
            return_types: vec![IrType::I32],
            sret_arg: None,
            vreg_types: vec![IrType::I32, IrType::I32], // v0=vmctx, v1=our value
            slots: vec![],
            body: vec![
                LpirOp::IconstI32 {
                    dst: v(1),
                    value: 42,
                },
                LpirOp::Return {
                    values: VRegRange { start: 1, count: 1 },
                },
            ],
            vreg_pool: vec![v(0), v(1)], // vreg_pool must contain the vregs being returned
        };

        let ir = LpirModule {
            imports: vec![],
            functions: BTreeMap::from([(FuncId(0), func.clone())]),
        };
        let sig = LpsModuleSig::default();
        let abi = ModuleAbi::from_ir_and_sig(crate::isa::IsaTarget::Rv32imac, &ir, &sig);

        let q32 = Q32Options::default();
        let lower_opts = LowerOpts {
            float_mode: FloatMode::Q32,
            q32: &q32,
        };
        let lowered = lower_ops(&func, &ir, &abi, &lower_opts).expect("lower ok");

        // Verify region tree is populated
        assert_ne!(lowered.region_tree.root, REGION_ID_NONE);
        assert!(!lowered.region_tree.nodes.is_empty());

        // For a simple function, we should have a Linear region
        let root_id = lowered.region_tree.root;
        let root_node = &lowered.region_tree.nodes[root_id as usize];
        assert!(
            matches!(root_node, Region::Linear { .. }),
            "Expected Linear region for simple function, got {root_node:?}",
        );

        // The linear region should cover the main VInsts (IConst32 + Ret + Br)
        // Note: The epilogue Label is added after lower_range returns, so end might
        // be less than vinsts.len() (which includes the final label)
        if let Region::Linear { start, end } = root_node {
            assert!(*start < *end, "Linear region should have non-empty range");
            // Region covers instructions up to but not including the final epilogue label
            assert!(
                *end as usize <= lowered.vinsts.len(),
                "Linear end should not exceed vinsts len"
            );
        }
    }
}
