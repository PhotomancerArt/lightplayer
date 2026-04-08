//! RV32 ILP32 calling convention — shader subset (see RISC-V psABI, QBE `rv64/abi.c` structure).

use alloc::vec::Vec;
use core::fmt;

use lps_shared::LpsFnSig;

/// Physical register index (x0–x31).
pub type PhysReg = u8;

pub const ZERO: PhysReg = 0;
pub const RA: PhysReg = 1;
pub const SP: PhysReg = 2;

pub const T0: PhysReg = 5;
pub const T1: PhysReg = 6;
pub const T2: PhysReg = 7;

/// Frame pointer (callee-saved).
pub const S0: PhysReg = 8;

pub const A0: PhysReg = 10;
pub const A1: PhysReg = 11;

/// Integer argument registers a0–a7.
pub const ARG_REGS: [PhysReg; 8] = [10, 11, 12, 13, 14, 15, 16, 17];

/// First two scalar return values (a0, a1).
pub const RET_REGS: [PhysReg; 2] = [A0, A1];

/// Caller-saved: a0–a7 and t0–t6 (clobbered across calls).
pub const CALLER_SAVED: &[PhysReg] = &[
    10, 11, 12, 13, 14, 15, 16, 17, // a0-a7
    5, 6, 7, 28, 29, 30, 31, // t0-t2, t3-t6
];

/// Callee-saved: s0–s11.
pub const CALLEE_SAVED: &[PhysReg] = &[8, 9, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27];

/// Registers available for greedy allocation (x8–x31 minus none for M1; includes callee-saved + upper temps).
pub const ALLOCA_REGS: &[PhysReg] = &[
    8, 9, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, // s0-s11
    28, 29, 30, 31, // t3-t6
];

/// XLEN for RV32.
pub const XLEN: u32 = 32;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArgAssignment {
    pub regs: Vec<PhysReg>,
    pub stack_slots: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameLayout {
    pub size: u32,
    pub saved_ra: bool,
    pub saved_s0: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AbiError {
    TooManyArgs,
}

impl fmt::Display for AbiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AbiError::TooManyArgs => {
                write!(f, "more than 8 scalar parameters (stack args not in M1)")
            }
        }
    }
}

impl core::error::Error for AbiError {}

/// Map each scalar `in` parameter to the next argument register. M1: one slot per `FnParam` (no struct expansion).
pub fn assign_args(sig: &LpsFnSig) -> Result<ArgAssignment, AbiError> {
    let n = sig.parameters.len();
    if n > ARG_REGS.len() {
        return Err(AbiError::TooManyArgs);
    }
    let regs = ARG_REGS[..n].to_vec();
    Ok(ArgAssignment {
        regs,
        stack_slots: 0,
    })
}

/// Primary return register for a scalar return (M1).
pub fn return_reg(_sig: &LpsFnSig) -> PhysReg {
    RET_REGS[0]
}

pub fn leaf_frame() -> FrameLayout {
    FrameLayout {
        size: 0,
        saved_ra: false,
        saved_s0: false,
    }
}

pub fn nonleaf_frame(_spill_slots: u32) -> FrameLayout {
    FrameLayout {
        size: 0,
        saved_ra: true,
        saved_s0: false,
    }
}

#[cfg(test)]
mod tests {
    use alloc::string::String;
    use alloc::vec;

    use super::*;
    use lps_shared::{FnParam, LpsType, ParamQualifier};

    fn sig_two_float() -> LpsFnSig {
        LpsFnSig {
            name: String::from("add"),
            return_type: LpsType::Float,
            parameters: vec![
                FnParam {
                    name: String::from("a"),
                    ty: LpsType::Float,
                    qualifier: ParamQualifier::In,
                },
                FnParam {
                    name: String::from("b"),
                    ty: LpsType::Float,
                    qualifier: ParamQualifier::In,
                },
            ],
        }
    }

    #[test]
    fn assign_two_args_uses_a0_a1() {
        let sig = sig_two_float();
        let a = assign_args(&sig).expect("assign");
        assert_eq!(a.regs, vec![A0, A1]);
        assert_eq!(a.stack_slots, 0);
    }

    #[test]
    fn return_reg_is_a0() {
        let sig = sig_two_float();
        assert_eq!(return_reg(&sig), A0);
    }

    #[test]
    fn arg_regs_in_caller_saved() {
        for reg in ARG_REGS {
            assert!(CALLER_SAVED.contains(&reg), "x{reg} should be caller-saved");
        }
    }

    #[test]
    fn too_many_args_errors() {
        let sig = LpsFnSig {
            name: String::from("many"),
            return_type: LpsType::Void,
            parameters: (0..10)
                .map(|i| FnParam {
                    name: alloc::format!("a{i}"),
                    ty: LpsType::Float,
                    qualifier: ParamQualifier::In,
                })
                .collect(),
        };
        assert_eq!(assign_args(&sig), Err(AbiError::TooManyArgs));
    }
}
