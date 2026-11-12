## Phase 3: RV32 ABI (Shader Subset)

### Scope

Define RV32 ILP32 register roles and calling convention. This is the critical layer that failed in the previous backend attempt. Validate with extensive unit tests — no emission yet, just the contract.

### Implementation details

**`isa/rv32/abi.rs`:**

```rust
//! RV32 ILP32 calling convention — shader subset
//! Based on RISC-V psABI, validated against QBE rv64/abi.c structure

pub type PhysReg = u8;

// Register indices (x0-x31)
pub const ZERO: PhysReg = 0;
pub const RA: PhysReg = 1;
pub const SP: PhysReg = 2;
pub const GP: PhysReg = 3;
pub const TP: PhysReg = 4;

// Temporaries (caller-saved)
pub const T0: PhysReg = 5;
pub const T1: PhysReg = 6;
pub const T2: PhysReg = 7;

// Callee-saved / frame pointer
pub const S0: PhysReg = 8;  // FP
pub const S1: PhysReg = 9;
// ... S2-S11 = 18-27

// Argument registers (caller-saved)
pub const A0: PhysReg = 10;
pub const A1: PhysReg = 11;
// ... A2-A7 = 12-17

pub const ARG_REGS: [PhysReg; 8] = [10, 11, 12, 13, 14, 15, 16, 17];
pub const RET_REGS: [PhysReg; 2] = [10, 11];

/// Caller-saved registers (clobbered by calls)
pub const CALLER_SAVED: &[PhysReg] = &[
    10, 11, 12, 13, 14, 15, 16, 17,  // a0-a7
    5, 6, 7, 28, 29, 30, 31,         // t0-t6
];

/// Callee-saved registers (must preserve)
pub const CALLEE_SAVED: &[PhysReg] = &[
    8, 9, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27,  // s0-s11
];

/// Allocatable for greedy allocator (x8-x31, excluding special roles)
pub const ALLOCA_REGS: &[PhysReg] = &[
    8, 9, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27,  // s0-s11 (s0 reserved as FP)
    28, 29, 30, 31,                                 // t3-t6
];

use lps_shared::{LpsFnSig, FnParam};

/// Argument register assignment for a function signature
#[derive(Debug, Clone)]
pub struct ArgAssignment {
    pub regs: alloc::vec::Vec<PhysReg>,
    pub stack_slots: u32,  // M1: should be 0 (error on >8 args)
}

/// Assign argument registers for a function call/definition
pub fn assign_args(sig: &LpsFnSig) -> Result<ArgAssignment, AbiError> {
    let mut regs = alloc::vec::Vec::new();
    
    for (i, param) in sig.params.iter().enumerate() {
        if i >= ARG_REGS.len() {
            return Err(AbiError::TooManyArgs);
        }
        // Simple: one scalar = one register
        // TODO(phase-4): multi-component types (vec2, vec3, vec4)
        regs.push(ARG_REGS[i]);
    }
    
    Ok(ArgAssignment { regs, stack_slots: 0 })
}

/// Return register for a single scalar return (M1)
pub fn return_reg(_sig: &LpsFnSig) -> PhysReg {
    RET_REGS[0]  // a0
}

/// Frame layout for a function
#[derive(Debug, Clone)]
pub struct FrameLayout {
    pub size: u32,
    pub saved_ra: bool,
    pub saved_s0: bool,
}

/// Minimal frame for leaf function (no calls, no spills)
pub fn leaf_frame() -> FrameLayout {
    FrameLayout { size: 0, saved_ra: false, saved_s0: false }
}

/// Non-leaf frame (has calls, needs saved RA)
pub fn nonleaf_frame(_spill_slots: u32) -> FrameLayout {
    // M1: stub — full calculation in M2
    FrameLayout { size: 0, saved_ra: true, saved_s0: false }
}

#[derive(Debug)]
pub enum AbiError {
    TooManyArgs,
}
```

### Tests

Extensive unit tests in `abi.rs` (bottom of file, per rules):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use lps_shared::{FnParam, LpsType};
    
    fn test_sig_2arg() -> LpsFnSig {
        LpsFnSig {
            params: vec![
                FnParam { name: "a".into(), ty: LpsType::Float, qualifier: ParamQualifier::In },
                FnParam { name: "b".into(), ty: LpsType::Float, qualifier: ParamQualifier::In },
            ],
            return_type: LpsType::Float,
        }
    }
    
    #[test]
    fn test_assign_2_args() {
        let sig = test_sig_2arg();
        let args = assign_args(&sig).unwrap();
        assert_eq!(args.regs, vec![A0, A1]);
        assert_eq!(args.stack_slots, 0);
    }
    
    #[test]
    fn test_return_reg() {
        let sig = test_sig_2arg();
        assert_eq!(return_reg(&sig), A0);
    }
    
    #[test]
    fn test_caller_saved_includes_args() {
        // A0-A7 must be in CALLER_SAVED for clobber tracking
        for reg in &ARG_REGS {
            assert!(CALLER_SAVED.contains(reg), "A{} not in CALLER_SAVED", reg - A0);
        }
    }
    
    #[test]
    fn test_too_many_args_errors() {
        let sig = LpsFnSig {
            params: (0..10).map(|i| FnParam {
                name: format!("a{}", i),
                ty: LpsType::Float,
                qualifier: ParamQualifier::In,
            }).collect(),
            return_type: LpsType::Void,
        };
        assert!(matches!(assign_args(&sig), Err(AbiError::TooManyArgs)));
    }
}
```

### Validation

```bash
cargo test -p lpvm-native --lib abi
```

All ABI tests must pass before proceeding to lowering.
