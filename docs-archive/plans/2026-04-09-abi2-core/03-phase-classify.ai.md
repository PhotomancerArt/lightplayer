# Phase 3: Classification

## Scope

Implement pure classification functions in `abi2/classify.rs`:
- `classify_params()` - where each parameter lives (reg or stack)
- `classify_return()` - return method (void, direct, or sret)

These are pure functions: signature in, locations out. No mutation, no state.

## Code Organization

- `ArgLoc` enum at top
- `ReturnMethod` enum follows
- Classification functions
- Helper for scalar counting at bottom
- Tests at end

## Implementation Details

### ArgLoc

```rust
/// Where a single scalar parameter/return value lives
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArgLoc {
    /// In a specific register
    Reg(PReg),
    /// On the stack at offset from SP at call time
    /// RV32: stack grows down, args passed at positive offsets from SP
    Stack { offset: i32, size: u32 },
}

impl ArgLoc {
    pub fn preg(&self) -> Option<PReg> {
        match self {
            ArgLoc::Reg(p) => Some(*p),
            ArgLoc::Stack { .. } => None,
        }
    }
}
```

### ReturnMethod

```rust
/// How a function returns its values
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReturnMethod {
    Void,
    /// Values returned directly in registers (up to 2 scalars for RV32)
    Direct { locs: Vec<ArgLoc> },
    /// Values returned via caller-allocated buffer (sret for >2 scalars)
    Sret {
        /// Register holding buffer pointer at entry (a0)
        ptr_reg: PReg,
        /// Callee-saved register for preservation (s1)
        preserved_reg: PReg,
        /// Number of scalar words to store
        word_count: u32,
    },
}

impl ReturnMethod {
    pub fn is_sret(&self) -> bool {
        matches!(self, ReturnMethod::Sret { .. })
    }
}
```

### classify_params

```rust
use lps_shared::{LpsFnSig, LpsType};
use crate::abi2::rv32;

/// Classify function parameters per RV32 ILP32 ABI.
/// 
/// # Arguments
/// * `sig` - Function signature
/// * `is_sret` - Whether return uses sret (a0 is occupied by ptr)
/// 
/// # Returns
/// Vector of ArgLoc, one per scalar component of parameters.
pub fn classify_params(sig: &LpsFnSig, is_sret: bool) -> Vec<ArgLoc> {
    let mut result = Vec::new();
    let mut arg_reg_idx = if is_sret { 1 } else { 0 };  // Start at a1 if sret
    let mut stack_offset = 0i32;

    for param in &sig.parameters {
        let scalar_count = scalar_count_of_type(&param.ty);
        for _ in 0..scalar_count {
            if arg_reg_idx < rv32::ARG_REGS.len() {
                // Argument goes in register
                result.push(ArgLoc::Reg(rv32::ARG_REGS[arg_reg_idx]));
                arg_reg_idx += 1;
            } else {
                // Argument goes on stack
                result.push(ArgLoc::Stack {
                    offset: stack_offset,
                    size: 4,  // 4 bytes per scalar
                });
                stack_offset += 4;
            }
        }
    }

    result
}
```

### classify_return

```rust
/// Classify function return per RV32 ILP32 ABI.
/// 
/// RV32 threshold: >2 scalars uses sret.
pub fn classify_return(sig: &LpsFnSig) -> ReturnMethod {
    let scalar_count = scalar_count_of_type(&sig.return_type);
    
    if scalar_count == 0 {
        return ReturnMethod::Void;
    }

    if scalar_count > rv32::SRET_THRESHOLD {
        // Sret: caller passes buffer pointer in a0
        // Callee stores results there
        ReturnMethod::Sret {
            ptr_reg: rv32::A0,
            preserved_reg: rv32::S1,
            word_count: scalar_count as u32,
        }
    } else {
        // Direct: return in a0-a1
        let mut locs = Vec::new();
        for i in 0..scalar_count {
            locs.push(ArgLoc::Reg(rv32::RET_REGS[i]));
        }
        ReturnMethod::Direct { locs }
    }
}
```

### scalar_count_of_type

```rust
/// Count scalar components in an LpsType.
fn scalar_count_of_type(ty: &LpsType) -> usize {
    use lps_shared::LpsType;
    match ty {
        LpsType::Void => 0,
        LpsType::Scalar(_) => 1,
        LpsType::Vec(n, _) => *n as usize,
        LpsType::Mat(n, m, _) => (*n * *m) as usize,
        LpsType::Array(_, _) => {
            // Arrays are passed by slot (indirect), 
            // so they count as 1 pointer for parameter passing
            1
        }
    }
}
```

## Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::abi2::rv32;

    fn float_sig() -> LpsFnSig {
        LpsFnSig {
            name: "test".into(),
            return_type: LpsType::Scalar(ScalarType::Float),
            parameters: vec![],
        }
    }

    fn vec4_sig() -> LpsFnSig {
        LpsFnSig {
            name: "test".into(),
            return_type: LpsType::Vec(4, ScalarType::Float),
            parameters: vec![],
        }
    }

    fn mat4_sig() -> LpsFnSig {
        LpsFnSig {
            name: "test".into(),
            return_type: LpsType::Mat(4, 4, ScalarType::Float),
            parameters: vec![],
        }
    }

    #[test]
    fn void_return_is_void() {
        let sig = LpsFnSig {
            name: "test".into(),
            return_type: LpsType::Void,
            parameters: vec![],
        };
        let ret = classify_return(&sig);
        assert!(matches!(ret, ReturnMethod::Void));
    }

    #[test]
    fn float_returns_in_a0() {
        let sig = float_sig();
        let ret = classify_return(&sig);
        match ret {
            ReturnMethod::Direct { locs } => {
                assert_eq!(locs.len(), 1);
                assert_eq!(locs[0], ArgLoc::Reg(rv32::A0));
            }
            _ => panic!("Expected Direct return"),
        }
    }

    #[test]
    fn vec4_returns_in_a0_a1_a2_a3() {
        let sig = vec4_sig();
        let ret = classify_return(&sig);
        match ret {
            ReturnMethod::Direct { locs } => {
                assert_eq!(locs.len(), 4);
                assert_eq!(locs[0], ArgLoc::Reg(rv32::A0));
                assert_eq!(locs[1], ArgLoc::Reg(rv32::A1));
                assert_eq!(locs[2], ArgLoc::Reg(rv32::A2));
                assert_eq!(locs[3], ArgLoc::Reg(rv32::A3));
            }
            _ => panic!("Expected Direct return"),
        }
    }

    #[test]
    fn mat4_is_sret() {
        let sig = mat4_sig();
        let ret = classify_return(&sig);
        match ret {
            ReturnMethod::Sret { ptr_reg, preserved_reg, word_count } => {
                assert_eq!(ptr_reg, rv32::A0);
                assert_eq!(preserved_reg, rv32::S1);
                assert_eq!(word_count, 16);  // 4x4 = 16 scalars
            }
            _ => panic!("Expected Sret"),
        }
    }

    #[test]
    fn params_start_at_a0_for_direct_return() {
        let sig = LpsFnSig {
            name: "test".into(),
            return_type: LpsType::Void,
            parameters: vec![
                Param { name: "a".into(), ty: LpsType::Scalar(ScalarType::Float), .. },
                Param { name: "b".into(), ty: LpsType::Scalar(ScalarType::Float), .. },
            ],
        };
        let is_sret = false;
        let locs = classify_params(&sig, is_sret);
        assert_eq!(locs.len(), 2);
        assert_eq!(locs[0], ArgLoc::Reg(rv32::A0));
        assert_eq!(locs[1], ArgLoc::Reg(rv32::A1));
    }

    #[test]
    fn params_start_at_a1_for_sret() {
        let sig = LpsFnSig {
            name: "test".into(),
            return_type: LpsType::Mat(4, 4, ScalarType::Float),  // sret
            parameters: vec![
                Param { name: "ctx".into(), ty: LpsType::Pointer, .. },  // vmctx
            ],
        };
        let is_sret = true;
        let locs = classify_params(&sig, is_sret);
        // vmctx should be in a1, not a0 (which holds sret ptr)
        assert_eq!(locs[0], ArgLoc::Reg(rv32::A1));
    }

    #[test]
    fn params_spill_after_a7() {
        let sig = LpsFnSig {
            name: "test".into(),
            return_type: LpsType::Void,
            parameters: vec![
                // 10 float params = 10 scalars, exceeds 8 arg regs
                Param { name: "a".into(), ty: LpsType::Scalar(ScalarType::Float), .. },
                // ... 9 more
            ],
        };
        let is_sret = false;
        let locs = classify_params(&sig, is_sret);
        assert_eq!(locs.len(), 10);
        // First 8 in regs
        assert_eq!(locs[0], ArgLoc::Reg(rv32::A0));
        assert_eq!(locs[7], ArgLoc::Reg(rv32::A7));
        // Last 2 on stack
        match locs[8] {
            ArgLoc::Stack { offset, size: 4 } => {
                assert_eq!(offset, 0);  // First stack arg
            }
            _ => panic!("Expected stack"),
        }
        match locs[9] {
            ArgLoc::Stack { offset, size: 4 } => {
                assert_eq!(offset, 4);  // Second stack arg
            }
            _ => panic!("Expected stack"),
        }
    }
}
```

## Validate

```bash
cargo test -p lpvm-native abi::classify
```

All classification tests should pass. Test edge cases:
- 0, 1, 2, 3, 4 scalar returns (void, direct, sret thresholds)
- Parameters in all arg registers + stack
- sret shifts params to a1+
