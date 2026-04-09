//! LPIR [`Op`] → [`VInst`] lowering (M1 subset).

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use lpir::{CalleeRef, FloatMode, IrFunction, IrModule, Op};

use crate::abi::ModuleAbi;
use crate::error::LowerError;
use crate::isa::rv32::abi::SRET_SCALAR_THRESHOLD;
use crate::vinst::{IcmpCond, LabelId, SymbolRef, VInst};

/// Lower one LPIR op. `src_op` is the index in [`IrFunction::body`].
pub fn lower_op(
    op: &Op,
    float_mode: FloatMode,
    src_op: Option<u32>,
    func: &IrFunction,
    ir: &IrModule,
    abi: &ModuleAbi,
) -> Result<VInst, LowerError> {
    match op {
        Op::Iadd { dst, lhs, rhs } => Ok(VInst::Add32 {
            dst: *dst,
            src1: *lhs,
            src2: *rhs,
            src_op,
        }),
        Op::Isub { dst, lhs, rhs } => Ok(VInst::Sub32 {
            dst: *dst,
            src1: *lhs,
            src2: *rhs,
            src_op,
        }),
        Op::Imul { dst, lhs, rhs } => Ok(VInst::Mul32 {
            dst: *dst,
            src1: *lhs,
            src2: *rhs,
            src_op,
        }),
        Op::IdivS { dst, lhs, rhs } => Ok(VInst::DivS32 {
            dst: *dst,
            lhs: *lhs,
            rhs: *rhs,
            src_op,
        }),
        Op::IdivU { dst, lhs, rhs } => Ok(VInst::DivU32 {
            dst: *dst,
            lhs: *lhs,
            rhs: *rhs,
            src_op,
        }),
        Op::IremS { dst, lhs, rhs } => Ok(VInst::RemS32 {
            dst: *dst,
            lhs: *lhs,
            rhs: *rhs,
            src_op,
        }),
        Op::IremU { dst, lhs, rhs } => Ok(VInst::RemU32 {
            dst: *dst,
            lhs: *lhs,
            rhs: *rhs,
            src_op,
        }),
        Op::Ineg { dst, src } => Ok(VInst::Neg32 {
            dst: *dst,
            src: *src,
            src_op,
        }),
        Op::Ieq { dst, lhs, rhs } => Ok(VInst::Icmp32 {
            dst: *dst,
            lhs: *lhs,
            rhs: *rhs,
            cond: IcmpCond::Eq,
            src_op,
        }),
        Op::Ine { dst, lhs, rhs } => Ok(VInst::Icmp32 {
            dst: *dst,
            lhs: *lhs,
            rhs: *rhs,
            cond: IcmpCond::Ne,
            src_op,
        }),
        Op::IltS { dst, lhs, rhs } => Ok(VInst::Icmp32 {
            dst: *dst,
            lhs: *lhs,
            rhs: *rhs,
            cond: IcmpCond::LtS,
            src_op,
        }),
        Op::IleS { dst, lhs, rhs } => Ok(VInst::Icmp32 {
            dst: *dst,
            lhs: *lhs,
            rhs: *rhs,
            cond: IcmpCond::LeS,
            src_op,
        }),
        Op::IgtS { dst, lhs, rhs } => Ok(VInst::Icmp32 {
            dst: *dst,
            lhs: *lhs,
            rhs: *rhs,
            cond: IcmpCond::GtS,
            src_op,
        }),
        Op::IgeS { dst, lhs, rhs } => Ok(VInst::Icmp32 {
            dst: *dst,
            lhs: *lhs,
            rhs: *rhs,
            cond: IcmpCond::GeS,
            src_op,
        }),
        Op::IltU { dst, lhs, rhs } => Ok(VInst::Icmp32 {
            dst: *dst,
            lhs: *lhs,
            rhs: *rhs,
            cond: IcmpCond::LtU,
            src_op,
        }),
        Op::IleU { dst, lhs, rhs } => Ok(VInst::Icmp32 {
            dst: *dst,
            lhs: *lhs,
            rhs: *rhs,
            cond: IcmpCond::LeU,
            src_op,
        }),
        Op::IgtU { dst, lhs, rhs } => Ok(VInst::Icmp32 {
            dst: *dst,
            lhs: *lhs,
            rhs: *rhs,
            cond: IcmpCond::GtU,
            src_op,
        }),
        Op::IgeU { dst, lhs, rhs } => Ok(VInst::Icmp32 {
            dst: *dst,
            lhs: *lhs,
            rhs: *rhs,
            cond: IcmpCond::GeU,
            src_op,
        }),
        Op::IeqImm { dst, src, imm } => Ok(VInst::IeqImm32 {
            dst: *dst,
            src: *src,
            imm: *imm,
            src_op,
        }),
        Op::Iand { dst, lhs, rhs } => Ok(VInst::And32 {
            dst: *dst,
            src1: *lhs,
            src2: *rhs,
            src_op,
        }),
        Op::Ior { dst, lhs, rhs } => Ok(VInst::Or32 {
            dst: *dst,
            src1: *lhs,
            src2: *rhs,
            src_op,
        }),
        Op::Ixor { dst, lhs, rhs } => Ok(VInst::Xor32 {
            dst: *dst,
            src1: *lhs,
            src2: *rhs,
            src_op,
        }),
        Op::Ibnot { dst, src } => Ok(VInst::Bnot32 {
            dst: *dst,
            src: *src,
            src_op,
        }),
        Op::Ishl { dst, lhs, rhs } => Ok(VInst::Shl32 {
            dst: *dst,
            src1: *lhs,
            src2: *rhs,
            src_op,
        }),
        Op::IshrS { dst, lhs, rhs } => Ok(VInst::ShrS32 {
            dst: *dst,
            src1: *lhs,
            src2: *rhs,
            src_op,
        }),
        Op::IshrU { dst, lhs, rhs } => Ok(VInst::ShrU32 {
            dst: *dst,
            src1: *lhs,
            src2: *rhs,
            src_op,
        }),
        Op::Select {
            dst,
            cond,
            if_true,
            if_false,
        } => Ok(VInst::Select32 {
            dst: *dst,
            cond: *cond,
            if_true: *if_true,
            if_false: *if_false,
            src_op,
        }),
        Op::Copy { dst, src } => Ok(VInst::Mov32 {
            dst: *dst,
            src: *src,
            src_op,
        }),
        Op::IconstI32 { dst, value } => Ok(VInst::IConst32 {
            dst: *dst,
            val: *value,
            src_op,
        }),
        Op::Return { values } => {
            let slice = func.pool_slice(*values);
            if slice.len() != values.count as usize {
                return Err(LowerError::UnsupportedOp {
                    description: String::from("Return: vreg_pool slice out of range"),
                });
            }
            Ok(VInst::Ret {
                vals: slice.to_vec(),
                src_op,
            })
        }

        Op::Fadd { dst, lhs, rhs } if float_mode == FloatMode::Q32 => Ok(VInst::Call {
            target: SymbolRef {
                name: String::from("__lp_lpir_fadd_q32"),
            },
            args: alloc::vec![*lhs, *rhs],
            rets: alloc::vec![*dst],
            callee_uses_sret: false,
            src_op,
        }),
        Op::Fsub { dst, lhs, rhs } if float_mode == FloatMode::Q32 => Ok(VInst::Call {
            target: SymbolRef {
                name: String::from("__lp_lpir_fsub_q32"),
            },
            args: alloc::vec![*lhs, *rhs],
            rets: alloc::vec![*dst],
            callee_uses_sret: false,
            src_op,
        }),
        Op::Fmul { dst, lhs, rhs } if float_mode == FloatMode::Q32 => Ok(VInst::Call {
            target: SymbolRef {
                name: String::from("__lp_lpir_fmul_q32"),
            },
            args: alloc::vec![*lhs, *rhs],
            rets: alloc::vec![*dst],
            callee_uses_sret: false,
            src_op,
        }),

        // Q32 float constants: convert f32 to Q32 fixed-point (multiply by 65536.0)
        Op::FconstF32 { dst, value } if float_mode == FloatMode::Q32 => {
            let q32_val = ((*value as f64) * 65536.0) as i32;
            Ok(VInst::IConst32 {
                dst: *dst,
                val: q32_val,
                src_op,
            })
        }

        Op::Fadd { .. } | Op::Fsub { .. } | Op::Fmul { .. } | Op::FconstF32 { .. } => {
            Err(LowerError::UnsupportedOp {
                description: String::from("float op in F32 mode (M1: Q32 only for float lowering)"),
            })
        }

        Op::IfStart { .. } | Op::Else | Op::End | Op::LoopStart { .. } => {
            Err(LowerError::UnsupportedOp {
                description: String::from(
                    "structural control-flow op must be lowered via lower_ops (IfStart/LoopStart/Else/End)",
                ),
            })
        }
        Op::Break | Op::Continue | Op::BrIfNot { .. } => Err(LowerError::UnsupportedOp {
            description: String::from(
                "break/continue/br_if_not must be lowered via lower_ops with loop context",
            ),
        }),

        Op::Call {
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
                target: SymbolRef { name },
                args: args_slice.to_vec(),
                rets: results_slice.to_vec(),
                callee_uses_sret,
                src_op,
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

struct LowerCtx<'a> {
    func: &'a IrFunction,
    ir: &'a IrModule,
    abi: &'a ModuleAbi,
    float_mode: FloatMode,
    out: Vec<VInst>,
    next_label: LabelId,
    loop_stack: Vec<LoopFrame>,
    epilogue_label: LabelId,
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
                Op::IfStart {
                    cond,
                    else_offset,
                    end_offset,
                } => {
                    let eo = *else_offset as usize;
                    let merge_after = *end_offset as usize;
                    let else_is_empty = matches!(self.func.body.get(eo), Some(Op::End));
                    if else_is_empty {
                        // `else_offset` points at `End` (no `Else` op); false and true paths share one label.
                        let merge = self.alloc_label();
                        self.out.push(VInst::BrIf {
                            cond: *cond,
                            target: merge,
                            invert: true,
                            src_op: Some(i as u32),
                        });
                        self.lower_range(i + 1, eo)?;
                        self.out.push(VInst::Br {
                            target: merge,
                            src_op: Some(i as u32),
                        });
                        self.out.push(VInst::Label(merge, Some(eo as u32)));
                    } else {
                        let else_label = self.alloc_label();
                        let end_label = self.alloc_label();
                        self.out.push(VInst::BrIf {
                            cond: *cond,
                            target: else_label,
                            invert: true,
                            src_op: Some(i as u32),
                        });
                        self.lower_range(i + 1, eo)?;
                        self.out.push(VInst::Br {
                            target: end_label,
                            src_op: Some(i as u32),
                        });
                        self.out.push(VInst::Label(else_label, Some(*else_offset)));
                        self.lower_range(eo + 1, merge_after)?;
                        let end_idx = merge_after.saturating_sub(1);
                        self.out.push(VInst::Label(end_label, Some(end_idx as u32)));
                    }
                    i = merge_after;
                }
                Op::LoopStart {
                    continuing_offset,
                    end_offset,
                } => {
                    let header = self.alloc_label();
                    let continuing = self.alloc_label();
                    let exit = self.alloc_label();
                    self.out.push(VInst::Br {
                        target: header,
                        src_op: Some(i as u32),
                    });
                    self.out.push(VInst::Label(header, Some((i + 1) as u32)));
                    self.loop_stack.push(LoopFrame { continuing, exit });
                    let co = *continuing_offset as usize;
                    let eo = *end_offset as usize;
                    // Body: from after LoopStart to continuing_offset
                    self.lower_range(i + 1, co)?;
                    // Continuing ops (increment, br_if_not, …) when `co < end`. When `co == i + 1`
                    // the body is empty but continuing still starts at the first op after LoopStart;
                    // we must emit it (otherwise the loop back-edge never hits BrIfNot).
                    if co < eo {
                        self.out
                            .push(VInst::Label(continuing, Some(*continuing_offset)));
                        self.lower_range(co, eo.saturating_sub(1))?
                    }
                    // Loop-closing End: back-edge to header
                    self.out.push(VInst::Br {
                        target: header,
                        src_op: Some((eo.saturating_sub(1)) as u32),
                    });
                    self.out.push(VInst::Label(exit, Some(*end_offset)));
                    self.loop_stack.pop();
                    i = eo;
                }
                Op::Break => {
                    let frame =
                        self.loop_stack
                            .last()
                            .ok_or_else(|| LowerError::UnsupportedOp {
                                description: String::from("break outside loop"),
                            })?;
                    self.out.push(VInst::Br {
                        target: frame.exit,
                        src_op: Some(i as u32),
                    });
                    i += 1;
                }
                Op::Continue => {
                    let frame =
                        self.loop_stack
                            .last()
                            .ok_or_else(|| LowerError::UnsupportedOp {
                                description: String::from("continue outside loop"),
                            })?;
                    self.out.push(VInst::Br {
                        target: frame.continuing,
                        src_op: Some(i as u32),
                    });
                    i += 1;
                }
                Op::BrIfNot { cond } => {
                    let frame =
                        self.loop_stack
                            .last()
                            .ok_or_else(|| LowerError::UnsupportedOp {
                                description: String::from("br_if_not outside loop"),
                            })?;
                    // If cond is false, exit the loop; if true, fall through (continue loop)
                    self.out.push(VInst::BrIf {
                        cond: *cond,
                        target: frame.exit,
                        invert: true,
                        src_op: Some(i as u32),
                    });
                    i += 1;
                }
                Op::Else | Op::End => {
                    i += 1;
                }
                other => {
                    if let Op::Copy { dst, src } = other {
                        if dst == src {
                            i += 1;
                            continue;
                        }
                    }
                    let is_return = matches!(other, Op::Return { .. });
                    self.out.push(lower_op(
                        other,
                        self.float_mode,
                        Some(i as u32),
                        self.func,
                        self.ir,
                        self.abi,
                    )?);
                    if is_return {
                        self.out.push(VInst::Br {
                            target: self.epilogue_label,
                            src_op: Some(i as u32),
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
    ir: &IrModule,
    abi: &ModuleAbi,
    float_mode: FloatMode,
) -> Result<Vec<VInst>, LowerError> {
    let mut ctx = LowerCtx {
        func,
        ir,
        abi,
        float_mode,
        out: Vec::with_capacity(func.body.len().saturating_mul(2)),
        next_label: 0,
        loop_stack: Vec::new(),
        epilogue_label: 0,
    };
    ctx.epilogue_label = ctx.alloc_label();
    ctx.lower_range(0, func.body.len())?;
    ctx.out.push(VInst::Label(ctx.epilogue_label, None));
    Ok(ctx.out)
}

fn resolve_callee_name(ir: &IrModule, callee: CalleeRef) -> Option<String> {
    let idx = callee.0 as usize;
    let ni = ir.imports.len();
    if idx < ni {
        ir.imports.get(idx).map(|imp| imp.func_name.clone())
    } else {
        ir.functions.get(idx - ni).map(|f| f.name.clone())
    }
}

fn callee_return_uses_sret(ir: &IrModule, abi: &ModuleAbi, callee: CalleeRef) -> bool {
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
    use crate::vinst::IcmpCond;
    use lpir::types::VRegRange;
    use lpir::{IrModule, IrType, VReg};
    use lps_shared::LpsModuleSig;

    fn empty_ir_abi() -> (IrModule, ModuleAbi) {
        let ir = IrModule::default();
        let abi = ModuleAbi::from_ir_and_sig(&ir, &LpsModuleSig { functions: vec![] });
        (ir, abi)
    }

    fn v(n: u32) -> VReg {
        VReg(n)
    }

    fn empty_func() -> IrFunction {
        IrFunction {
            name: String::new(),
            is_entry: true,
            vmctx_vreg: VReg(0),
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
        let op = Op::Iadd {
            dst: v(2),
            lhs: v(0),
            rhs: v(1),
        };
        let f = empty_func();
        let (ir, abi) = empty_ir_abi();
        let got = lower_op(&op, FloatMode::Q32, Some(0), &f, &ir, &abi).expect("ok");
        assert!(matches!(
            got,
            VInst::Add32 {
                dst: VReg(2),
                src1: VReg(0),
                src2: VReg(1),
                src_op: Some(0),
            }
        ));
    }

    #[test]
    fn lower_q32_fadd_to_call() {
        let op = Op::Fadd {
            dst: v(2),
            lhs: v(0),
            rhs: v(1),
        };
        let f = empty_func();
        let (ir, abi) = empty_ir_abi();
        match lower_op(&op, FloatMode::Q32, Some(3), &f, &ir, &abi).expect("ok") {
            VInst::Call {
                target,
                args,
                rets,
                callee_uses_sret,
                src_op,
            } => {
                assert_eq!(target.name, "__lp_lpir_fadd_q32");
                assert_eq!(args, vec![v(0), v(1)]);
                assert_eq!(rets, vec![v(2)]);
                assert!(!callee_uses_sret);
                assert_eq!(src_op, Some(3));
            }
            other => panic!("expected Call, got {other:?}"),
        }
    }

    #[test]
    fn lower_f32_float_unsupported() {
        let op = Op::Fadd {
            dst: v(0),
            lhs: v(1),
            rhs: v(2),
        };
        let f = empty_func();
        let (ir, abi) = empty_ir_abi();
        assert!(lower_op(&op, FloatMode::F32, None, &f, &ir, &abi).is_err());
    }

    #[test]
    fn lower_ineg() {
        let op = Op::Ineg {
            dst: v(1),
            src: v(0),
        };
        let f = empty_func();
        let (ir, abi) = empty_ir_abi();
        let got = lower_op(&op, FloatMode::Q32, Some(0), &f, &ir, &abi).expect("ok");
        assert!(matches!(
            got,
            VInst::Neg32 {
                dst: VReg(1),
                src: VReg(0),
                src_op: Some(0),
            }
        ));
    }

    #[test]
    fn lower_ieq_imm() {
        let op = Op::IeqImm {
            dst: v(1),
            src: v(0),
            imm: 0,
        };
        let f = empty_func();
        let (ir, abi) = empty_ir_abi();
        assert!(matches!(
            lower_op(&op, FloatMode::Q32, Some(0), &f, &ir, &abi).expect("ok"),
            VInst::IeqImm32 {
                dst: VReg(1),
                src: VReg(0),
                imm: 0,
                src_op: Some(0),
            }
        ));
    }

    #[test]
    fn lower_iand() {
        let op = Op::Iand {
            dst: v(2),
            lhs: v(0),
            rhs: v(1),
        };
        let f = empty_func();
        let (ir, abi) = empty_ir_abi();
        assert!(matches!(
            lower_op(&op, FloatMode::Q32, Some(0), &f, &ir, &abi).expect("ok"),
            VInst::And32 {
                dst: VReg(2),
                src1: VReg(0),
                src2: VReg(1),
                src_op: Some(0),
            }
        ));
    }

    #[test]
    fn lower_ibnot() {
        let op = Op::Ibnot {
            dst: v(1),
            src: v(0),
        };
        let f = empty_func();
        let (ir, abi) = empty_ir_abi();
        assert!(matches!(
            lower_op(&op, FloatMode::Q32, Some(0), &f, &ir, &abi).expect("ok"),
            VInst::Bnot32 {
                dst: VReg(1),
                src: VReg(0),
                src_op: Some(0),
            }
        ));
    }

    #[test]
    fn lower_idivs() {
        let op = Op::IdivS {
            dst: v(2),
            lhs: v(0),
            rhs: v(1),
        };
        let f = empty_func();
        let (ir, abi) = empty_ir_abi();
        let got = lower_op(&op, FloatMode::Q32, Some(0), &f, &ir, &abi).expect("ok");
        assert!(matches!(
            got,
            VInst::DivS32 {
                dst: VReg(2),
                lhs: VReg(0),
                rhs: VReg(1),
                src_op: Some(0),
            }
        ));
    }

    #[test]
    fn lower_ieq_to_icmp() {
        let op = Op::Ieq {
            dst: v(2),
            lhs: v(0),
            rhs: v(1),
        };
        let f = empty_func();
        let (ir, abi) = empty_ir_abi();
        match lower_op(&op, FloatMode::Q32, Some(0), &f, &ir, &abi).expect("ok") {
            VInst::Icmp32 { cond, .. } => assert_eq!(cond, IcmpCond::Eq),
            other => panic!("expected Icmp32, got {other:?}"),
        }
    }

    #[test]
    fn lower_iltu_to_icmp() {
        let op = Op::IltU {
            dst: v(2),
            lhs: v(0),
            rhs: v(1),
        };
        let f = empty_func();
        let (ir, abi) = empty_ir_abi();
        match lower_op(&op, FloatMode::Q32, Some(0), &f, &ir, &abi).expect("ok") {
            VInst::Icmp32 { cond, .. } => assert_eq!(cond, IcmpCond::LtU),
            other => panic!("expected Icmp32, got {other:?}"),
        }
    }

    #[test]
    fn lower_select() {
        let op = Op::Select {
            dst: v(3),
            cond: v(0),
            if_true: v(1),
            if_false: v(2),
        };
        let f = empty_func();
        let (ir, abi) = empty_ir_abi();
        match lower_op(&op, FloatMode::Q32, Some(0), &f, &ir, &abi).expect("ok") {
            VInst::Select32 {
                dst,
                cond,
                if_true,
                if_false,
                src_op,
            } => {
                assert_eq!(dst, v(3));
                assert_eq!(cond, v(0));
                assert_eq!(if_true, v(1));
                assert_eq!(if_false, v(2));
                assert_eq!(src_op, Some(0));
            }
            other => panic!("expected Select32, got {other:?}"),
        }
    }

    #[test]
    fn lower_return_uses_vreg_pool() {
        let f = IrFunction {
            name: String::from("f"),
            is_entry: true,
            vmctx_vreg: VReg(0),
            param_count: 0,
            return_types: vec![IrType::I32],
            vreg_types: vec![],
            slots: vec![],
            body: vec![],
            vreg_pool: vec![v(10), v(11)],
        };
        let op = Op::Return {
            values: VRegRange { start: 0, count: 2 },
        };
        let (ir, abi) = empty_ir_abi();
        let got = lower_op(&op, FloatMode::Q32, Some(1), &f, &ir, &abi).expect("ok");
        match got {
            VInst::Ret { vals, src_op } => {
                assert_eq!(vals, vec![v(10), v(11)]);
                assert_eq!(src_op, Some(1));
            }
            other => panic!("expected Ret, got {other:?}"),
        }
    }
}
