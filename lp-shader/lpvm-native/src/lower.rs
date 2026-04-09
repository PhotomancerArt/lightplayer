//! LPIR [`Op`] → [`VInst`] lowering (M1 subset).

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use lpir::{FloatMode, IrFunction, Op};

use crate::error::LowerError;
use crate::vinst::{IcmpCond, SymbolRef, VInst};

/// Lower one LPIR op. `src_op` is the index in [`IrFunction::body`].
pub fn lower_op(
    op: &Op,
    float_mode: FloatMode,
    src_op: Option<u32>,
    func: &IrFunction,
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
            src_op,
        }),
        Op::Fsub { dst, lhs, rhs } if float_mode == FloatMode::Q32 => Ok(VInst::Call {
            target: SymbolRef {
                name: String::from("__lp_lpir_fsub_q32"),
            },
            args: alloc::vec![*lhs, *rhs],
            rets: alloc::vec![*dst],
            src_op,
        }),
        Op::Fmul { dst, lhs, rhs } if float_mode == FloatMode::Q32 => Ok(VInst::Call {
            target: SymbolRef {
                name: String::from("__lp_lpir_fmul_q32"),
            },
            args: alloc::vec![*lhs, *rhs],
            rets: alloc::vec![*dst],
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

        other => Err(LowerError::UnsupportedOp {
            description: format!("{other:?}"),
        }),
    }
}

/// Lower a straight-line slice of ops (no control-flow markers). Stops at first error.
pub fn lower_ops(func: &IrFunction, float_mode: FloatMode) -> Result<Vec<VInst>, LowerError> {
    let mut out = Vec::with_capacity(func.body.len());
    for (i, op) in func.body.iter().enumerate() {
        if let Op::Copy { dst, src } = op {
            if dst == src {
                continue;
            }
        }
        out.push(lower_op(op, float_mode, Some(i as u32), func)?);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use alloc::string::String;
    use alloc::vec;

    use super::*;
    use crate::vinst::IcmpCond;
    use lpir::types::VRegRange;
    use lpir::{IrType, VReg};

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
        let got = lower_op(&op, FloatMode::Q32, Some(0), &f).expect("ok");
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
        match lower_op(&op, FloatMode::Q32, Some(3), &f).expect("ok") {
            VInst::Call {
                target,
                args,
                rets,
                src_op,
            } => {
                assert_eq!(target.name, "__lp_lpir_fadd_q32");
                assert_eq!(args, vec![v(0), v(1)]);
                assert_eq!(rets, vec![v(2)]);
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
        assert!(lower_op(&op, FloatMode::F32, None, &f).is_err());
    }

    #[test]
    fn lower_ineg() {
        let op = Op::Ineg {
            dst: v(1),
            src: v(0),
        };
        let f = empty_func();
        let got = lower_op(&op, FloatMode::Q32, Some(0), &f).expect("ok");
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
        assert!(matches!(
            lower_op(&op, FloatMode::Q32, Some(0), &f).expect("ok"),
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
        assert!(matches!(
            lower_op(&op, FloatMode::Q32, Some(0), &f).expect("ok"),
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
        assert!(matches!(
            lower_op(&op, FloatMode::Q32, Some(0), &f).expect("ok"),
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
        let got = lower_op(&op, FloatMode::Q32, Some(0), &f).expect("ok");
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
        match lower_op(&op, FloatMode::Q32, Some(0), &f).expect("ok") {
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
        match lower_op(&op, FloatMode::Q32, Some(0), &f).expect("ok") {
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
        match lower_op(&op, FloatMode::Q32, Some(0), &f).expect("ok") {
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
        let got = lower_op(&op, FloatMode::Q32, Some(1), &f).expect("ok");
        match got {
            VInst::Ret { vals, src_op } => {
                assert_eq!(vals, vec![v(10), v(11)]);
                assert_eq!(src_op, Some(1));
            }
            other => panic!("expected Ret, got {other:?}"),
        }
    }
}
