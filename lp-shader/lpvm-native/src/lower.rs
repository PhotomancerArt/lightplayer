//! LPIR [`Op`] → [`VInst`] lowering (M1 subset).

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use lpir::{FloatMode, Op, VReg, VRegRange};

use crate::error::LowerError;
use crate::vinst::{SymbolRef, VInst};

fn vregs_from_range(r: VRegRange) -> Vec<VReg> {
    (0..r.count).map(|i| VReg(r.start + u32::from(i))).collect()
}

/// Lower one LPIR op. Q32 float math lowers to builtin [`VInst::Call`].
pub fn lower_op(op: &Op, float_mode: FloatMode) -> Result<VInst, LowerError> {
    match op {
        Op::Iadd { dst, lhs, rhs } => Ok(VInst::Add32 {
            dst: *dst,
            src1: *lhs,
            src2: *rhs,
        }),
        Op::Isub { dst, lhs, rhs } => Ok(VInst::Sub32 {
            dst: *dst,
            src1: *lhs,
            src2: *rhs,
        }),
        Op::Imul { dst, lhs, rhs } => Ok(VInst::Mul32 {
            dst: *dst,
            src1: *lhs,
            src2: *rhs,
        }),
        Op::IconstI32 { dst, value } => Ok(VInst::IConst32 {
            dst: *dst,
            val: *value,
        }),
        Op::Return { values } => Ok(VInst::Ret {
            vals: vregs_from_range(*values),
        }),

        Op::Fadd { dst, lhs, rhs } if float_mode == FloatMode::Q32 => Ok(VInst::Call {
            target: SymbolRef {
                name: String::from("__lp_lpir_fadd_q32"),
            },
            args: alloc::vec![*lhs, *rhs],
            rets: alloc::vec![*dst],
        }),
        Op::Fsub { dst, lhs, rhs } if float_mode == FloatMode::Q32 => Ok(VInst::Call {
            target: SymbolRef {
                name: String::from("__lp_lpir_fsub_q32"),
            },
            args: alloc::vec![*lhs, *rhs],
            rets: alloc::vec![*dst],
        }),
        Op::Fmul { dst, lhs, rhs } if float_mode == FloatMode::Q32 => Ok(VInst::Call {
            target: SymbolRef {
                name: String::from("__lp_lpir_fmul_q32"),
            },
            args: alloc::vec![*lhs, *rhs],
            rets: alloc::vec![*dst],
        }),

        Op::Fadd { .. } | Op::Fsub { .. } | Op::Fmul { .. } => Err(LowerError::UnsupportedOp {
            description: String::from("float op in F32 mode (M1: Q32 only for float lowering)"),
        }),

        other => Err(LowerError::UnsupportedOp {
            description: format!("{other:?}"),
        }),
    }
}

/// Lower a straight-line slice of ops (no control-flow markers). Stops at first error.
pub fn lower_ops(ops: &[Op], float_mode: FloatMode) -> Result<Vec<VInst>, LowerError> {
    let mut out = Vec::with_capacity(ops.len());
    for op in ops {
        out.push(lower_op(op, float_mode)?);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use alloc::vec;

    use super::*;
    use lpir::types::VRegRange;

    fn v(n: u32) -> VReg {
        VReg(n)
    }

    #[test]
    fn lower_iadd() {
        let op = Op::Iadd {
            dst: v(2),
            lhs: v(0),
            rhs: v(1),
        };
        let got = lower_op(&op, FloatMode::Q32).expect("ok");
        assert!(matches!(
            got,
            VInst::Add32 {
                dst: VReg(2),
                src1: VReg(0),
                src2: VReg(1)
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
        match lower_op(&op, FloatMode::Q32).expect("ok") {
            VInst::Call { target, args, rets } => {
                assert_eq!(target.name, "__lp_lpir_fadd_q32");
                assert_eq!(args, vec![v(0), v(1)]);
                assert_eq!(rets, vec![v(2)]);
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
        assert!(lower_op(&op, FloatMode::F32).is_err());
    }

    #[test]
    fn lower_return_range() {
        let op = Op::Return {
            values: VRegRange {
                start: 10,
                count: 2,
            },
        };
        let got = lower_op(&op, FloatMode::Q32).expect("ok");
        match got {
            VInst::Ret { vals } => assert_eq!(vals, vec![v(10), v(11)]),
            other => panic!("expected Ret, got {other:?}"),
        }
    }
}
