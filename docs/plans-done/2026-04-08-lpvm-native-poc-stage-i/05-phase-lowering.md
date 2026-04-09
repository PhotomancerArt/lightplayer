## Phase 5: LPIR → VInst Lowering

### Scope

Implement `lower.rs` with `lower_op()` function. Support integer ALU and return. Q32 float ops lower to `Call` VInsts. Everything else returns `Err(UnsupportedOp)`.

### Implementation details

**`lower.rs`:**

```rust
use lpir::op::Op;
use crate::vinst::{VInst, SymbolRef};
use crate::types::NativeType;
use crate::error::LowerError;

/// Lower a single LPIR Op to a VInst
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
        
        Op::IconstI32 { dst, val } => Ok(VInst::IConst32 {
            dst: *dst,
            val: *val,
        }),
        
        Op::Return { val } => Ok(VInst::Ret {
            vals: val.iter().copied().collect(),
        }),
        
        // Q32 float ops → builtin calls
        Op::Fadd { dst, lhs, rhs } if float_mode == FloatMode::Q32 => {
            Ok(VInst::Call {
                target: SymbolRef { name: "__lp_lpir_fadd_q32".into() },
                args: vec![*lhs, *rhs],
                rets: vec![*dst],
            })
        }
        Op::Fsub { dst, lhs, rhs } if float_mode == FloatMode::Q32 => {
            Ok(VInst::Call {
                target: SymbolRef { name: "__lp_lpir_fsub_q32".into() },
                args: vec![*lhs, *rhs],
                rets: vec![*dst],
            })
        }
        Op::Fmul { dst, lhs, rhs } if float_mode == FloatMode::Q32 => {
            Ok(VInst::Call {
                target: SymbolRef { name: "__lp_lpir_fmul_q32".into() },
                args: vec![*lhs, *rhs],
                rets: vec![*dst],
            })
        }
        
        // F32 mode not implemented in M1
        Op::Fadd { .. } | Op::Fsub { .. } | Op::Fmul { .. } => {
            Err(LowerError::UnsupportedOp { op: op.clone() })
        }
        
        // Everything else unsupported in M1
        _ => Err(LowerError::UnsupportedOp { op: op.clone() }),
    }
}

/// Lower a full function body
pub fn lower_function(body: &[Op], float_mode: FloatMode) -> Result<Vec<VInst>, LowerError> {
    let mut result = Vec::with_capacity(body.len());
    for op in body {
        result.push(lower_op(op, float_mode)?);
    }
    Ok(result)
}
```

### Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use lpir::op::Op;
    use lpir::types::VReg;
    
    fn v(n: u32) -> VReg { VReg(n) }
    
    #[test]
    fn test_lower_iadd() {
        let op = Op::Iadd { dst: v(2), lhs: v(0), rhs: v(1) };
        let inst = lower_op(&op, FloatMode::Q32).unwrap();
        assert!(matches!(inst, VInst::Add32 { dst: VReg(2), .. }));
    }
    
    #[test]
    fn test_lower_q32_fadd_to_call() {
        let op = Op::Fadd { dst: v(2), lhs: v(0), rhs: v(1) };
        let inst = lower_op(&op, FloatMode::Q32).unwrap();
        match inst {
            VInst::Call { target, args, rets } => {
                assert_eq!(target.name, "__lp_lpir_fadd_q32");
                assert_eq!(args, vec![v(0), v(1)]);
                assert_eq!(rets, vec![v(2)]);
            }
            _ => panic!("Expected Call, got {:?}", inst),
        }
    }
    
    #[test]
    fn test_lower_f32_unsupported() {
        let op = Op::Fadd { dst: v(0), lhs: v(1), rhs: v(2) };
        let result = lower_op(&op, FloatMode::F32);
        assert!(result.is_err());
    }
}
```

### Validation

```bash
cargo test -p lpvm-native --lib lower
```
