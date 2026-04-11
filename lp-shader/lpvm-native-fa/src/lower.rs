//! LPIR [`LpirOp`] → [`VInst`] lowering (M1 subset).

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use lpir::{CalleeRef, FloatMode, IrFunction, LpirModule, LpirOp};
use lps_builtin_ids::{
    BuiltinId, GlslParamKind, glsl_lpfx_q32_builtin_id, glsl_q32_math_builtin_id,
    lpir_q32_builtin_id, vm_q32_builtin_id,
};

use crate::abi::ModuleAbi;
use crate::error::LowerError;
use crate::rv32::abi::SRET_SCALAR_THRESHOLD;
use crate::region::RegionTree;
use crate::vinst::{
    IcmpCond, LabelId, ModuleSymbols, SRC_OP_NONE, VInst, VReg, VRegSlice, pack_src_op,
};

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

fn sym_call(
    symbols: &mut ModuleSymbols,
    pool: &mut Vec<VReg>,
    name: &'static str,
    args: &[lpir::VReg],
    rets: &[lpir::VReg],
    src_op: Option<u32>,
) -> Result<VInst, LowerError> {
    Ok(VInst::Call {
        target: symbols.intern(name),
        args: push_vregs_slice(pool, args)?,
        rets: push_vregs_slice(pool, rets)?,
        callee_uses_sret: false,
        src_op: pack_src_op(src_op),
    })
}

/// Lower one LPIR op. `src_op` is the index in [`IrFunction::body`].
pub fn lower_lpir_op(
    op: &LpirOp,
    float_mode: FloatMode,
    src_op: Option<u32>,
    func: &IrFunction,
    ir: &LpirModule,
    abi: &ModuleAbi,
    symbols: &mut ModuleSymbols,
    vreg_pool: &mut Vec<VReg>,
) -> Result<VInst, LowerError> {
    let po = pack_src_op(src_op);
    match op {
        LpirOp::Iadd { dst, lhs, rhs } => Ok(VInst::Add32 {
            dst: fa_vreg(*dst),
            src1: fa_vreg(*lhs),
            src2: fa_vreg(*rhs),
            src_op: po,
        }),
        LpirOp::Isub { dst, lhs, rhs } => Ok(VInst::Sub32 {
            dst: fa_vreg(*dst),
            src1: fa_vreg(*lhs),
            src2: fa_vreg(*rhs),
            src_op: po,
        }),
        LpirOp::Imul { dst, lhs, rhs } => Ok(VInst::Mul32 {
            dst: fa_vreg(*dst),
            src1: fa_vreg(*lhs),
            src2: fa_vreg(*rhs),
            src_op: po,
        }),
        LpirOp::IdivS { dst, lhs, rhs } => Ok(VInst::DivS32 {
            dst: fa_vreg(*dst),
            lhs: fa_vreg(*lhs),
            rhs: fa_vreg(*rhs),
            src_op: po,
        }),
        LpirOp::IdivU { dst, lhs, rhs } => Ok(VInst::DivU32 {
            dst: fa_vreg(*dst),
            lhs: fa_vreg(*lhs),
            rhs: fa_vreg(*rhs),
            src_op: po,
        }),
        LpirOp::IremS { dst, lhs, rhs } => Ok(VInst::RemS32 {
            dst: fa_vreg(*dst),
            lhs: fa_vreg(*lhs),
            rhs: fa_vreg(*rhs),
            src_op: po,
        }),
        LpirOp::IremU { dst, lhs, rhs } => Ok(VInst::RemU32 {
            dst: fa_vreg(*dst),
            lhs: fa_vreg(*lhs),
            rhs: fa_vreg(*rhs),
            src_op: po,
        }),
        LpirOp::Ineg { dst, src } => Ok(VInst::Neg32 {
            dst: fa_vreg(*dst),
            src: fa_vreg(*src),
            src_op: po,
        }),
        LpirOp::Ieq { dst, lhs, rhs } => Ok(VInst::Icmp32 {
            dst: fa_vreg(*dst),
            lhs: fa_vreg(*lhs),
            rhs: fa_vreg(*rhs),
            cond: IcmpCond::Eq,
            src_op: po,
        }),
        LpirOp::Ine { dst, lhs, rhs } => Ok(VInst::Icmp32 {
            dst: fa_vreg(*dst),
            lhs: fa_vreg(*lhs),
            rhs: fa_vreg(*rhs),
            cond: IcmpCond::Ne,
            src_op: po,
        }),
        LpirOp::IltS { dst, lhs, rhs } => Ok(VInst::Icmp32 {
            dst: fa_vreg(*dst),
            lhs: fa_vreg(*lhs),
            rhs: fa_vreg(*rhs),
            cond: IcmpCond::LtS,
            src_op: po,
        }),
        LpirOp::IleS { dst, lhs, rhs } => Ok(VInst::Icmp32 {
            dst: fa_vreg(*dst),
            lhs: fa_vreg(*lhs),
            rhs: fa_vreg(*rhs),
            cond: IcmpCond::LeS,
            src_op: po,
        }),
        LpirOp::IgtS { dst, lhs, rhs } => Ok(VInst::Icmp32 {
            dst: fa_vreg(*dst),
            lhs: fa_vreg(*lhs),
            rhs: fa_vreg(*rhs),
            cond: IcmpCond::GtS,
            src_op: po,
        }),
        LpirOp::IgeS { dst, lhs, rhs } => Ok(VInst::Icmp32 {
            dst: fa_vreg(*dst),
            lhs: fa_vreg(*lhs),
            rhs: fa_vreg(*rhs),
            cond: IcmpCond::GeS,
            src_op: po,
        }),
        LpirOp::IltU { dst, lhs, rhs } => Ok(VInst::Icmp32 {
            dst: fa_vreg(*dst),
            lhs: fa_vreg(*lhs),
            rhs: fa_vreg(*rhs),
            cond: IcmpCond::LtU,
            src_op: po,
        }),
        LpirOp::IleU { dst, lhs, rhs } => Ok(VInst::Icmp32 {
            dst: fa_vreg(*dst),
            lhs: fa_vreg(*lhs),
            rhs: fa_vreg(*rhs),
            cond: IcmpCond::LeU,
            src_op: po,
        }),
        LpirOp::IgtU { dst, lhs, rhs } => Ok(VInst::Icmp32 {
            dst: fa_vreg(*dst),
            lhs: fa_vreg(*lhs),
            rhs: fa_vreg(*rhs),
            cond: IcmpCond::GtU,
            src_op: po,
        }),
        LpirOp::IgeU { dst, lhs, rhs } => Ok(VInst::Icmp32 {
            dst: fa_vreg(*dst),
            lhs: fa_vreg(*lhs),
            rhs: fa_vreg(*rhs),
            cond: IcmpCond::GeU,
            src_op: po,
        }),
        LpirOp::IeqImm { dst, src, imm } => Ok(VInst::IeqImm32 {
            dst: fa_vreg(*dst),
            src: fa_vreg(*src),
            imm: *imm,
            src_op: po,
        }),
        LpirOp::Iand { dst, lhs, rhs } => Ok(VInst::And32 {
            dst: fa_vreg(*dst),
            src1: fa_vreg(*lhs),
            src2: fa_vreg(*rhs),
            src_op: po,
        }),
        LpirOp::Ior { dst, lhs, rhs } => Ok(VInst::Or32 {
            dst: fa_vreg(*dst),
            src1: fa_vreg(*lhs),
            src2: fa_vreg(*rhs),
            src_op: po,
        }),
        LpirOp::Ixor { dst, lhs, rhs } => Ok(VInst::Xor32 {
            dst: fa_vreg(*dst),
            src1: fa_vreg(*lhs),
            src2: fa_vreg(*rhs),
            src_op: po,
        }),
        LpirOp::Ibnot { dst, src } => Ok(VInst::Bnot32 {
            dst: fa_vreg(*dst),
            src: fa_vreg(*src),
            src_op: po,
        }),
        LpirOp::Ishl { dst, lhs, rhs } => Ok(VInst::Shl32 {
            dst: fa_vreg(*dst),
            src1: fa_vreg(*lhs),
            src2: fa_vreg(*rhs),
            src_op: po,
        }),
        LpirOp::IshrS { dst, lhs, rhs } => Ok(VInst::ShrS32 {
            dst: fa_vreg(*dst),
            src1: fa_vreg(*lhs),
            src2: fa_vreg(*rhs),
            src_op: po,
        }),
        LpirOp::IshrU { dst, lhs, rhs } => Ok(VInst::ShrU32 {
            dst: fa_vreg(*dst),
            src1: fa_vreg(*lhs),
            src2: fa_vreg(*rhs),
            src_op: po,
        }),
        LpirOp::Select {
            dst,
            cond,
            if_true,
            if_false,
        } => Ok(VInst::Select32 {
            dst: fa_vreg(*dst),
            cond: fa_vreg(*cond),
            if_true: fa_vreg(*if_true),
            if_false: fa_vreg(*if_false),
            src_op: po,
        }),
        LpirOp::Copy { dst, src } => Ok(VInst::Mov32 {
            dst: fa_vreg(*dst),
            src: fa_vreg(*src),
            src_op: po,
        }),
        LpirOp::IconstI32 { dst, value } => Ok(VInst::IConst32 {
            dst: fa_vreg(*dst),
            val: *value,
            src_op: po,
        }),

        LpirOp::Load { dst, base, offset } => {
            let off = i32::try_from(*offset).map_err(|_| LowerError::UnsupportedOp {
                description: String::from("Load: offset does not fit i32"),
            })?;
            Ok(VInst::Load32 {
                dst: fa_vreg(*dst),
                base: fa_vreg(*base),
                offset: off,
                src_op: po,
            })
        }
        LpirOp::Store {
            base,
            offset,
            value,
        } => {
            let off = i32::try_from(*offset).map_err(|_| LowerError::UnsupportedOp {
                description: String::from("Store: offset does not fit i32"),
            })?;
            Ok(VInst::Store32 {
                src: fa_vreg(*value),
                base: fa_vreg(*base),
                offset: off,
                src_op: po,
            })
        }
        LpirOp::SlotAddr { dst, slot } => Ok(VInst::SlotAddr {
            dst: fa_vreg(*dst),
            slot: slot.0,
            src_op: po,
        }),
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
            Ok(VInst::MemcpyWords {
                dst_base: fa_vreg(*dst_addr),
                src_base: fa_vreg(*src_addr),
                size: *size,
                src_op: po,
            })
        }

        LpirOp::Return { values } => {
            let slice = func.pool_slice(*values);
            if slice.len() != values.count as usize {
                return Err(LowerError::UnsupportedOp {
                    description: String::from("Return: vreg_pool slice out of range"),
                });
            }
            Ok(VInst::Ret {
                vals: push_vregs_slice(vreg_pool, slice)?,
                src_op: po,
            })
        }

        LpirOp::Fadd { dst, lhs, rhs } if float_mode == FloatMode::Q32 => sym_call(
            symbols,
            vreg_pool,
            "__lp_lpir_fadd_q32",
            &[*lhs, *rhs],
            &[*dst],
            src_op,
        ),
        LpirOp::Fsub { dst, lhs, rhs } if float_mode == FloatMode::Q32 => sym_call(
            symbols,
            vreg_pool,
            "__lp_lpir_fsub_q32",
            &[*lhs, *rhs],
            &[*dst],
            src_op,
        ),
        LpirOp::Fmul { dst, lhs, rhs } if float_mode == FloatMode::Q32 => sym_call(
            symbols,
            vreg_pool,
            "__lp_lpir_fmul_q32",
            &[*lhs, *rhs],
            &[*dst],
            src_op,
        ),
        LpirOp::Fdiv { dst, lhs, rhs } if float_mode == FloatMode::Q32 => sym_call(
            symbols,
            vreg_pool,
            "__lp_lpir_fdiv_q32",
            &[*lhs, *rhs],
            &[*dst],
            src_op,
        ),
        LpirOp::Fneg { dst, src } if float_mode == FloatMode::Q32 => Ok(VInst::Neg32 {
            dst: fa_vreg(*dst),
            src: fa_vreg(*src),
            src_op: po,
        }),
        LpirOp::ItofS { dst, src } if float_mode == FloatMode::Q32 => sym_call(
            symbols,
            vreg_pool,
            "__lp_lpir_itof_s_q32",
            &[*src],
            &[*dst],
            src_op,
        ),
        LpirOp::ItofU { dst, src } if float_mode == FloatMode::Q32 => sym_call(
            symbols,
            vreg_pool,
            "__lp_lpir_itof_u_q32",
            &[*src],
            &[*dst],
            src_op,
        ),

        LpirOp::Feq { dst, lhs, rhs } if float_mode == FloatMode::Q32 => Ok(VInst::Icmp32 {
            dst: fa_vreg(*dst),
            lhs: fa_vreg(*lhs),
            rhs: fa_vreg(*rhs),
            cond: IcmpCond::Eq,
            src_op: po,
        }),
        LpirOp::Fne { dst, lhs, rhs } if float_mode == FloatMode::Q32 => Ok(VInst::Icmp32 {
            dst: fa_vreg(*dst),
            lhs: fa_vreg(*lhs),
            rhs: fa_vreg(*rhs),
            cond: IcmpCond::Ne,
            src_op: po,
        }),
        LpirOp::Flt { dst, lhs, rhs } if float_mode == FloatMode::Q32 => Ok(VInst::Icmp32 {
            dst: fa_vreg(*dst),
            lhs: fa_vreg(*lhs),
            rhs: fa_vreg(*rhs),
            cond: IcmpCond::LtS,
            src_op: po,
        }),
        LpirOp::Fle { dst, lhs, rhs } if float_mode == FloatMode::Q32 => Ok(VInst::Icmp32 {
            dst: fa_vreg(*dst),
            lhs: fa_vreg(*lhs),
            rhs: fa_vreg(*rhs),
            cond: IcmpCond::LeS,
            src_op: po,
        }),
        LpirOp::Fgt { dst, lhs, rhs } if float_mode == FloatMode::Q32 => Ok(VInst::Icmp32 {
            dst: fa_vreg(*dst),
            lhs: fa_vreg(*lhs),
            rhs: fa_vreg(*rhs),
            cond: IcmpCond::GtS,
            src_op: po,
        }),
        LpirOp::Fge { dst, lhs, rhs } if float_mode == FloatMode::Q32 => Ok(VInst::Icmp32 {
            dst: fa_vreg(*dst),
            lhs: fa_vreg(*lhs),
            rhs: fa_vreg(*rhs),
            cond: IcmpCond::GeS,
            src_op: po,
        }),

        // Q32 float constants: convert f32 to Q32 fixed-point (multiply by 65536.0)
        LpirOp::FconstF32 { dst, value } if float_mode == FloatMode::Q32 => {
            let q32_val = ((*value as f64) * 65536.0) as i32;
            Ok(VInst::IConst32 {
                dst: fa_vreg(*dst),
                val: q32_val,
                src_op: po,
            })
        }

        LpirOp::Fsqrt { dst, src } if float_mode == FloatMode::Q32 => sym_call(
            symbols,
            vreg_pool,
            "__lp_lpir_fsqrt_q32",
            &[*src],
            &[*dst],
            src_op,
        ),
        LpirOp::Fnearest { dst, src } if float_mode == FloatMode::Q32 => sym_call(
            symbols,
            vreg_pool,
            "__lp_lpir_fnearest_q32",
            &[*src],
            &[*dst],
            src_op,
        ),
        LpirOp::Fabs { dst, src } if float_mode == FloatMode::Q32 => sym_call(
            symbols,
            vreg_pool,
            "__lp_lpir_fabs_q32",
            &[*src],
            &[*dst],
            src_op,
        ),
        LpirOp::Fmin { dst, lhs, rhs } if float_mode == FloatMode::Q32 => sym_call(
            symbols,
            vreg_pool,
            "__lp_lpir_fmin_q32",
            &[*lhs, *rhs],
            &[*dst],
            src_op,
        ),
        LpirOp::Fmax { dst, lhs, rhs } if float_mode == FloatMode::Q32 => sym_call(
            symbols,
            vreg_pool,
            "__lp_lpir_fmax_q32",
            &[*lhs, *rhs],
            &[*dst],
            src_op,
        ),
        LpirOp::Ffloor { dst, src } if float_mode == FloatMode::Q32 => sym_call(
            symbols,
            vreg_pool,
            "__lp_lpir_ffloor_q32",
            &[*src],
            &[*dst],
            src_op,
        ),
        LpirOp::Fceil { dst, src } if float_mode == FloatMode::Q32 => sym_call(
            symbols,
            vreg_pool,
            "__lp_lpir_fceil_q32",
            &[*src],
            &[*dst],
            src_op,
        ),
        LpirOp::Ftrunc { dst, src } if float_mode == FloatMode::Q32 => sym_call(
            symbols,
            vreg_pool,
            "__lp_lpir_ftrunc_q32",
            &[*src],
            &[*dst],
            src_op,
        ),
        LpirOp::FtoiSatS { dst, src } if float_mode == FloatMode::Q32 => sym_call(
            symbols,
            vreg_pool,
            "__lp_lpir_ftoi_sat_s_q32",
            &[*src],
            &[*dst],
            src_op,
        ),
        LpirOp::FtoiSatU { dst, src } if float_mode == FloatMode::Q32 => sym_call(
            symbols,
            vreg_pool,
            "__lp_lpir_ftoi_sat_u_q32",
            &[*src],
            &[*dst],
            src_op,
        ),
        LpirOp::FfromI32Bits { dst, src } if float_mode == FloatMode::Q32 => Ok(VInst::Mov32 {
            dst: fa_vreg(*dst),
            src: fa_vreg(*src),
            src_op: po,
        }),

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
        | LpirOp::FfromI32Bits { .. } => Err(LowerError::UnsupportedOp {
            description: String::from("float op requires Q32 mode (F32 not supported on rv32)"),
        }),

        LpirOp::IfStart { .. } | LpirOp::Else | LpirOp::End | LpirOp::LoopStart { .. } => {
            Err(LowerError::UnsupportedOp {
                description: String::from(
                    "structural control-flow op must be lowered via lower_ops (IfStart/LoopStart/Else/End)",
                ),
            })
        }
        LpirOp::Break | LpirOp::Continue | LpirOp::BrIfNot { .. } => Err(LowerError::UnsupportedOp {
            description: String::from(
                "break/continue/br_if_not must be lowered via lower_ops with loop context",
            ),
        }),

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
            Ok(VInst::Call {
                target: symbols.intern(name),
                args: push_vregs_slice(vreg_pool, args_slice)?,
                rets: push_vregs_slice(vreg_pool, results_slice)?,
                callee_uses_sret,
                src_op: po,
            })
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
}

struct LowerCtx<'a> {
    func: &'a IrFunction,
    ir: &'a LpirModule,
    abi: &'a ModuleAbi,
    float_mode: FloatMode,
    out: Vec<VInst>,
    vreg_pool: Vec<VReg>,
    symbols: ModuleSymbols,
    next_label: LabelId,
    loop_stack: Vec<LoopFrame>,
    epilogue_label: LabelId,
    loop_regions: Vec<LoopRegion>,
}

impl<'a> LowerCtx<'a> {
    fn alloc_label(&mut self) -> LabelId {
        let id = self.next_label;
        self.next_label = self.next_label.wrapping_add(1);
        id
    }

    fn lower_range(&mut self, start: usize, end: usize) -> Result<(), LowerError> {
        let mut i = start;
        while i < end {
            match &self.func.body[i] {
                LpirOp::IfStart {
                    cond,
                    else_offset,
                    end_offset,
                } => {
                    let eo = *else_offset as usize;
                    let merge_after = *end_offset as usize;
                    let else_is_empty = matches!(self.func.body.get(eo), Some(LpirOp::End));
                    if else_is_empty {
                        // `else_offset` points at `End` (no `Else` op); false and true paths share one label.
                        let merge = self.alloc_label();
                        self.out.push(VInst::BrIf {
                            cond: fa_vreg(*cond),
                            target: merge,
                            invert: true,
                            src_op: pack_src_op(Some(i as u32)),
                        });
                        self.lower_range(i + 1, eo)?;
                        self.out.push(VInst::Br {
                            target: merge,
                            src_op: pack_src_op(Some(i as u32)),
                        });
                        self.out
                            .push(VInst::Label(merge, pack_src_op(Some(eo as u32))));
                    } else {
                        let else_label = self.alloc_label();
                        let end_label = self.alloc_label();
                        self.out.push(VInst::BrIf {
                            cond: fa_vreg(*cond),
                            target: else_label,
                            invert: true,
                            src_op: pack_src_op(Some(i as u32)),
                        });
                        self.lower_range(i + 1, eo)?;
                        self.out.push(VInst::Br {
                            target: end_label,
                            src_op: pack_src_op(Some(i as u32)),
                        });
                        self.out
                            .push(VInst::Label(else_label, pack_src_op(Some(*else_offset))));
                        self.lower_range(eo + 1, merge_after)?;
                        let end_idx = merge_after.saturating_sub(1);
                        self.out
                            .push(VInst::Label(end_label, pack_src_op(Some(end_idx as u32))));
                    }
                    i = merge_after;
                }
                LpirOp::LoopStart {
                    continuing_offset,
                    end_offset,
                } => {
                    let header = self.alloc_label();
                    let continuing = self.alloc_label();
                    let exit = self.alloc_label();
                    self.out.push(VInst::Br {
                        target: header,
                        src_op: pack_src_op(Some(i as u32)),
                    });
                    let header_idx = self.out.len();
                    self.out
                        .push(VInst::Label(header, pack_src_op(Some((i + 1) as u32))));
                    self.loop_stack.push(LoopFrame { continuing, exit });
                    let co = *continuing_offset as usize;
                    let eo = *end_offset as usize;
                    // Body: from after LoopStart to continuing_offset
                    self.lower_range(i + 1, co)?;
                    // Continuing ops (increment, br_if_not, …) when `co < end`. When `co == i + 1`
                    // the body is empty but continuing still starts at the first op after LoopStart;
                    // we must emit it (otherwise the loop back-edge never hits BrIfNot).
                    if co < eo {
                        self.out.push(VInst::Label(
                            continuing,
                            pack_src_op(Some(*continuing_offset)),
                        ));
                        self.lower_range(co, eo.saturating_sub(1))?
                    }
                    // Loop-closing End: back-edge to header
                    let backedge_idx = self.out.len();
                    self.out.push(VInst::Br {
                        target: header,
                        src_op: pack_src_op(Some((eo.saturating_sub(1)) as u32)),
                    });
                    self.loop_regions.push(LoopRegion {
                        header_idx,
                        backedge_idx,
                    });
                    self.out
                        .push(VInst::Label(exit, pack_src_op(Some(*end_offset))));
                    self.loop_stack.pop();
                    i = eo;
                }
                LpirOp::Break => {
                    let frame =
                        self.loop_stack
                            .last()
                            .ok_or_else(|| LowerError::UnsupportedOp {
                                description: String::from("break outside loop"),
                            })?;
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
                    // If cond is false, exit the loop; if true, fall through (continue loop)
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
                    let is_return = matches!(other, LpirOp::Return { .. });
                    self.out.push(lower_lpir_op(
                        other,
                        self.float_mode,
                        Some(i as u32),
                        self.func,
                        self.ir,
                        self.abi,
                        &mut self.symbols,
                        &mut self.vreg_pool,
                    )?);
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
        Ok(())
    }
}

/// Lower full function body (including if/else and loop control flow).
pub fn lower_ops(
    func: &IrFunction,
    ir: &LpirModule,
    abi: &ModuleAbi,
    float_mode: FloatMode,
) -> Result<LoweredFunction, LowerError> {
    let mut ctx = LowerCtx {
        func,
        ir,
        abi,
        float_mode,
        out: Vec::with_capacity(func.body.len().saturating_mul(2)),
        vreg_pool: Vec::new(),
        symbols: ModuleSymbols::default(),
        next_label: 0,
        loop_stack: Vec::new(),
        epilogue_label: 0,
        loop_regions: Vec::new(),
    };
    ctx.epilogue_label = ctx.alloc_label();
    ctx.lower_range(0, func.body.len())?;
    ctx.out.push(VInst::Label(ctx.epilogue_label, SRC_OP_NONE));
    Ok(LoweredFunction {
        vinsts: ctx.out,
        vreg_pool: ctx.vreg_pool,
        symbols: ctx.symbols,
        loop_regions: ctx.loop_regions,
        region_tree: RegionTree::default(),
    })
}

fn resolve_callee_name(ir: &LpirModule, callee: CalleeRef) -> Option<String> {
    let idx = callee.0 as usize;
    let ni = ir.imports.len();
    if idx < ni {
        // For imports, map to the C ABI symbol name using BuiltinId
        ir.imports.get(idx).map(|imp| {
            // Try to resolve to a BuiltinId to get the proper C symbol name
            if let Some(bid) = resolve_import_to_builtin(imp) {
                String::from(bid.name())
            } else {
                // Fallback: use the import name directly (for non-builtin imports)
                imp.func_name.clone()
            }
        })
    } else {
        ir.functions.get(idx - ni).map(|f| f.name.clone())
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
        "lpfx" => {
            // LPFX builtins are named like "lpfx_psrdnoise_34" - strip the suffix
            let base = lpfx_strip_suffix(&decl.func_name)?;
            // Get GLSL kinds from lpfx_glsl_params CSV or fall back to IR types
            let kinds = lpfx_glsl_kinds_from_decl(decl);
            glsl_lpfx_q32_builtin_id(base, &kinds)
        }
        "vm" => {
            let ac = decl.param_types.len();
            vm_q32_builtin_id(&decl.func_name, ac)
        }
        _ => None,
    }
}

/// Strip the numeric suffix from LPFX import names (e.g., "lpfx_psrdnoise_34" → "lpfx_psrdnoise").
fn lpfx_strip_suffix(func_name: &str) -> Option<&str> {
    let (base, tail) = func_name.rsplit_once('_')?;
    tail.parse::<u32>().ok()?;
    Some(base)
}

/// Get GLSL parameter kinds from lpfx_glsl_params CSV or infer from IR types.
fn lpfx_glsl_kinds_from_decl(decl: &lpir::lpir_module::ImportDecl) -> Vec<GlslParamKind> {
    if let Some(ref enc) = decl.lpfx_glsl_params {
        parse_lpfx_glsl_params_csv(enc)
            .unwrap_or_else(|_| ir_params_to_glsl_kinds(&decl.param_types))
    } else {
        ir_params_to_glsl_kinds(&decl.param_types)
    }
}

/// Parse LPFX glsl params CSV (e.g., "Vec2,Vec2,Float,Vec2,UInt").
fn parse_lpfx_glsl_params_csv(enc: &str) -> Result<Vec<GlslParamKind>, String> {
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
    let idx = callee.0 as usize;
    let ni = ir.imports.len();
    if idx < ni {
        return ir.imports[idx].return_types.len() > SRET_SCALAR_THRESHOLD;
    }
    let Some(f) = ir.functions.get(idx - ni) else {
        return false;
    };
    if let Some(fa) = abi.func_abi(f.name.as_str()) {
        fa.is_sret()
    } else {
        f.return_types.len() > SRET_SCALAR_THRESHOLD
    }
}

#[cfg(test)]
mod tests {
    use alloc::string::String;
    use alloc::vec;

    use super::*;
    use crate::error::LowerError;
    use crate::vinst::{IcmpCond, ModuleSymbols, VReg as FaVReg, unpack_src_op};

    fn call_lower_op(
        op: &LpirOp,
        float_mode: FloatMode,
        src_op: Option<u32>,
        f: &IrFunction,
        ir: &LpirModule,
        abi: &ModuleAbi,
    ) -> Result<VInst, LowerError> {
        let mut symbols = ModuleSymbols::default();
        let mut pool = Vec::new();
        super::lower_lpir_op(op, float_mode, src_op, f, ir, abi, &mut symbols, &mut pool)
    }

    fn call_lower_op_full(
        op: &LpirOp,
        float_mode: FloatMode,
        src_op: Option<u32>,
        f: &IrFunction,
        ir: &LpirModule,
        abi: &ModuleAbi,
    ) -> Result<(VInst, ModuleSymbols, Vec<FaVReg>), LowerError> {
        let mut symbols = ModuleSymbols::default();
        let mut pool = Vec::new();
        let v = super::lower_lpir_op(op, float_mode, src_op, f, ir, abi, &mut symbols, &mut pool)?;
        Ok((v, symbols, pool))
    }
    use lpir::types::{SlotId, VRegRange};
    use lpir::{LpirModule, IrType, VReg as IrVReg};
    use lps_shared::LpsModuleSig;

    fn empty_ir_abi() -> (LpirModule, ModuleAbi) {
        let ir = LpirModule::default();
        let abi = ModuleAbi::from_ir_and_sig(&ir, &LpsModuleSig { functions: vec![] });
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
        let got = call_lower_op(&op, FloatMode::Q32, Some(0), &f, &ir, &abi).expect("ok");
        assert!(matches!(
            got,
            VInst::Add32 {
                dst: FaVReg(2),
                src1: FaVReg(0),
                src2: FaVReg(1),
                src_op,
            } if unpack_src_op(src_op) == Some(0)
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
        assert!(matches!(
            call_lower_op(&load, FloatMode::Q32, None, &f, &ir, &abi).expect("load"),
            VInst::Load32 {
                dst: FaVReg(3),
                base: FaVReg(2),
                offset: 4,
                ..
            }
        ));
        let store = LpirOp::Store {
            base: v(2),
            offset: 8,
            value: v(3),
        };
        assert!(matches!(
            call_lower_op(&store, FloatMode::Q32, None, &f, &ir, &abi).expect("store"),
            VInst::Store32 {
                src: FaVReg(3),
                base: FaVReg(2),
                offset: 8,
                ..
            }
        ));
        let sa = LpirOp::SlotAddr {
            dst: v(1),
            slot: SlotId(0),
        };
        assert!(matches!(
            call_lower_op(&sa, FloatMode::Q32, None, &f, &ir, &abi).expect("slot_addr"),
            VInst::SlotAddr {
                dst: FaVReg(1),
                slot: 0,
                ..
            }
        ));
        let mc = LpirOp::Memcpy {
            dst_addr: v(4),
            src_addr: v(5),
            size: 12,
        };
        assert!(matches!(
            call_lower_op(&mc, FloatMode::Q32, None, &f, &ir, &abi).expect("memcpy"),
            VInst::MemcpyWords {
                dst_base: FaVReg(4),
                src_base: FaVReg(5),
                size: 12,
                ..
            }
        ));
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
        let (got, symbols, pool) =
            call_lower_op_full(&op, FloatMode::Q32, Some(3), &f, &ir, &abi).expect("ok");
        match got {
            VInst::Call {
                target,
                args,
                rets,
                callee_uses_sret,
                src_op,
            } => {
                assert_eq!(symbols.name(target), "__lp_lpir_fadd_q32");
                assert_eq!(args.vregs(&pool), &[FaVReg(0), FaVReg(1)]);
                assert_eq!(rets.vregs(&pool), &[FaVReg(2)]);
                assert!(!callee_uses_sret);
                assert_eq!(unpack_src_op(src_op), Some(3));
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
        let (got, symbols, pool) =
            call_lower_op_full(&op, FloatMode::Q32, Some(0), &f, &ir, &abi).expect("ok");
        match got {
            VInst::Call {
                target,
                args,
                rets,
                callee_uses_sret,
                src_op,
            } => {
                assert_eq!(symbols.name(target), "__lp_lpir_fdiv_q32");
                assert_eq!(args.vregs(&pool), &[FaVReg(0), FaVReg(1)]);
                assert_eq!(rets.vregs(&pool), &[FaVReg(2)]);
                assert!(!callee_uses_sret);
                assert_eq!(unpack_src_op(src_op), Some(0));
            }
            other => panic!("expected Call, got {other:?}"),
        }
    }

    #[test]
    fn lower_q32_fneg_to_neg32() {
        let op = LpirOp::Fneg {
            dst: v(1),
            src: v(0),
        };
        let f = empty_func();
        let (ir, abi) = empty_ir_abi();
        assert!(matches!(
            call_lower_op(&op, FloatMode::Q32, Some(0), &f, &ir, &abi).expect("ok"),
            VInst::Neg32 {
                dst: FaVReg(1),
                src: FaVReg(0),
                src_op,
            } if unpack_src_op(src_op) == Some(0)
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
        let (got, symbols, pool) =
            call_lower_op_full(&op, FloatMode::Q32, Some(0), &f, &ir, &abi).expect("ok");
        match got {
            VInst::Call {
                target,
                args,
                rets,
                callee_uses_sret,
                src_op,
            } => {
                assert_eq!(symbols.name(target), "__lp_lpir_itof_s_q32");
                assert_eq!(args.vregs(&pool), &[FaVReg(0)]);
                assert_eq!(rets.vregs(&pool), &[FaVReg(1)]);
                assert!(!callee_uses_sret);
                assert_eq!(unpack_src_op(src_op), Some(0));
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
        let (got, symbols, pool) =
            call_lower_op_full(&op, FloatMode::Q32, Some(0), &f, &ir, &abi).expect("ok");
        match got {
            VInst::Call {
                target,
                args,
                rets,
                callee_uses_sret,
                src_op,
            } => {
                assert_eq!(symbols.name(target), "__lp_lpir_itof_u_q32");
                assert_eq!(args.vregs(&pool), &[FaVReg(0)]);
                assert_eq!(rets.vregs(&pool), &[FaVReg(1)]);
                assert!(!callee_uses_sret);
                assert_eq!(unpack_src_op(src_op), Some(0));
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
        let (got, symbols, pool) =
            call_lower_op_full(&op, FloatMode::Q32, Some(0), &f, &ir, &abi).expect("ok");
        match got {
            VInst::Call {
                target,
                args,
                rets,
                callee_uses_sret,
                src_op,
            } => {
                assert_eq!(symbols.name(target), "__lp_lpir_fsqrt_q32");
                assert_eq!(args.vregs(&pool), &[FaVReg(0)]);
                assert_eq!(rets.vregs(&pool), &[FaVReg(1)]);
                assert!(!callee_uses_sret);
                assert_eq!(unpack_src_op(src_op), Some(0));
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
        let (got, symbols, pool) =
            call_lower_op_full(&op, FloatMode::Q32, Some(0), &f, &ir, &abi).expect("ok");
        match got {
            VInst::Call {
                target,
                args,
                rets,
                src_op,
                ..
            } => {
                assert_eq!(symbols.name(target), "__lp_lpir_ffloor_q32");
                assert_eq!(args.vregs(&pool), &[FaVReg(0)]);
                assert_eq!(rets.vregs(&pool), &[FaVReg(1)]);
                assert_eq!(unpack_src_op(src_op), Some(0));
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
        assert!(matches!(
            call_lower_op(&op, FloatMode::Q32, Some(2), &f, &ir, &abi).expect("ok"),
            VInst::Mov32 {
                dst: FaVReg(1),
                src: FaVReg(0),
                src_op,
            } if unpack_src_op(src_op) == Some(2)
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
        let got = call_lower_op(&op, FloatMode::Q32, Some(0), &f, &ir, &abi).expect("ok");
        assert!(matches!(
            got,
            VInst::Neg32 {
                dst: FaVReg(1),
                src: FaVReg(0),
                src_op,
            } if unpack_src_op(src_op) == Some(0)
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
        assert!(matches!(
            call_lower_op(&op, FloatMode::Q32, Some(0), &f, &ir, &abi).expect("ok"),
            VInst::IeqImm32 {
                dst: FaVReg(1),
                src: FaVReg(0),
                imm: 0,
                src_op,
            } if unpack_src_op(src_op) == Some(0)
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
        assert!(matches!(
            call_lower_op(&op, FloatMode::Q32, Some(0), &f, &ir, &abi).expect("ok"),
            VInst::And32 {
                dst: FaVReg(2),
                src1: FaVReg(0),
                src2: FaVReg(1),
                src_op,
            } if unpack_src_op(src_op) == Some(0)
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
        assert!(matches!(
            call_lower_op(&op, FloatMode::Q32, Some(0), &f, &ir, &abi).expect("ok"),
            VInst::Bnot32 {
                dst: FaVReg(1),
                src: FaVReg(0),
                src_op,
            } if unpack_src_op(src_op) == Some(0)
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
        let got = call_lower_op(&op, FloatMode::Q32, Some(0), &f, &ir, &abi).expect("ok");
        assert!(matches!(
            got,
            VInst::DivS32 {
                dst: FaVReg(2),
                lhs: FaVReg(0),
                rhs: FaVReg(1),
                src_op,
            } if unpack_src_op(src_op) == Some(0)
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
        match call_lower_op(&op, FloatMode::Q32, Some(0), &f, &ir, &abi).expect("ok") {
            VInst::Icmp32 { cond, .. } => assert_eq!(cond, IcmpCond::Eq),
            other => panic!("expected Icmp32, got {other:?}"),
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
        match call_lower_op(&op, FloatMode::Q32, Some(0), &f, &ir, &abi).expect("ok") {
            VInst::Icmp32 { cond, .. } => assert_eq!(cond, IcmpCond::LtU),
            other => panic!("expected Icmp32, got {other:?}"),
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
        match call_lower_op(&op, FloatMode::Q32, Some(0), &f, &ir, &abi).expect("ok") {
            VInst::Select32 {
                dst,
                cond,
                if_true,
                if_false,
                src_op,
            } => {
                assert_eq!(dst, FaVReg(3));
                assert_eq!(cond, FaVReg(0));
                assert_eq!(if_true, FaVReg(1));
                assert_eq!(if_false, FaVReg(2));
                assert_eq!(unpack_src_op(src_op), Some(0));
            }
            other => panic!("expected Select32, got {other:?}"),
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
            vreg_types: vec![],
            slots: vec![],
            body: vec![],
            vreg_pool: vec![v(10), v(11)],
        };
        let op = LpirOp::Return {
            values: VRegRange { start: 0, count: 2 },
        };
        let (ir, abi) = empty_ir_abi();
        let (got, _symbols, pool) =
            call_lower_op_full(&op, FloatMode::Q32, Some(1), &f, &ir, &abi).expect("ok");
        match got {
            VInst::Ret { vals, src_op } => {
                assert_eq!(vals.vregs(&pool), &[FaVReg(10), FaVReg(11)]);
                assert_eq!(unpack_src_op(src_op), Some(1));
            }
            other => panic!("expected Ret, got {other:?}"),
        }
    }

    fn assert_q32_fcmp(want: IcmpCond, op: LpirOp) {
        let f = empty_func();
        let (ir, abi) = empty_ir_abi();
        match call_lower_op(&op, FloatMode::Q32, None, &f, &ir, &abi).expect("ok") {
            VInst::Icmp32 {
                cond,
                dst: FaVReg(2),
                lhs: FaVReg(0),
                rhs: FaVReg(1),
                ..
            } => assert_eq!(cond, want),
            other => panic!("expected Icmp32, got {other:?}"),
        }
    }
}
