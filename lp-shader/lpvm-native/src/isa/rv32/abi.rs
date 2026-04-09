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

/// Sret pointer preservation register (callee-saved).
/// When a function uses the sret calling convention, a0 contains the buffer
/// pointer at entry. We save it to s1 in the prologue and use s1 when
/// storing return values, since a0 may be clobbered by the function body.
pub const SRET_PTR: PhysReg = 9; // s1

pub const A0: PhysReg = 10;
pub const A1: PhysReg = 11;

/// Integer argument registers a0–a7.
pub const ARG_REGS: [PhysReg; 8] = [10, 11, 12, 13, 14, 15, 16, 17];

/// First four scalar return values (a0, a1, a2, a3).
/// RV32 ILP32: up to 4 registers for return values (16 bytes).
pub const RET_REGS: [PhysReg; 4] = [A0, A1, 12, 13];

/// Caller-saved: a0–a7 and t0–t6 (clobbered across calls).
pub const CALLER_SAVED: &[PhysReg] = &[
    10, 11, 12, 13, 14, 15, 16, 17, // a0-a7
    5, 6, 7, 28, 29, 30, 31, // t0-t2, t3-t6
];

/// Callee-saved: s0–s11.
pub const CALLEE_SAVED: &[PhysReg] = &[8, 9, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27];

/// Registers available for greedy allocation (x5-x31 minus reserved).
/// Includes: t2,t3-t6 (caller-saved), s1-s11 (callee-saved).
/// Excludes: x0 (zero), x1 (ra), x2 (sp), a0-a7 (args/return), t0-t1 (reserved for spill temps), s0 (frame pointer).
pub const ALLOCA_REGS: &[PhysReg] = &[
    7, // t2 (caller-saved)
    9, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, // s1-s11 (callee-saved)
    28, 29, 30, 31, // t3-t6 (caller-saved)
];

/// Temporary registers reserved for spill handling.
pub const SPILL_TEMPS: &[PhysReg] = &[5, 6]; // t0, t1

/// XLEN for RV32.
pub const XLEN: u32 = 32;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArgAssignment {
    pub regs: Vec<PhysReg>,
    pub stack_slots: u32,
}

/// Stack slot (LPIR slot or spill slot).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StackSlot {
    pub index: u32,
    pub size: u32,
    pub align: u32,
}

impl StackSlot {
    /// Offset from s0 (frame pointer) for this slot.
    /// QBE-style: negative offsets below the frame pointer.
    /// Slot 0 = -8, Slot 1 = -12, Slot 2 = -16, ...
    pub fn offset_from_s0(&self) -> i32 {
        -((8 + self.index * 4) as i32)
    }
}

/// Frame layout for a function.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameLayout {
    /// Total frame size (16-byte aligned).
    pub size: u32,
    /// Whether to save ra.
    pub saved_ra: bool,
    /// Whether to save s0 (frame pointer).
    pub saved_s0: bool,
    /// LPIR stack slots (for sret buffers, local storage).
    pub stack_slots: Vec<StackSlot>,
    /// Number of spill slots assigned by regalloc.
    pub spill_count: u32,
    /// Offset where s0 is saved (relative to sp after prologue).
    pub s0_save_offset: i32,
    /// Offset where ra is saved (relative to sp after prologue).
    pub ra_save_offset: i32,
}

impl FrameLayout {
    /// Compute total frame size and layout.
    /// QBE-style: s0-relative with negative offsets for slots.
    pub fn compute(spill_count: u32) -> Self {
        // Fixed header: saved s0 + saved ra = 8 bytes
        let header_size = 8u32;

        // Spill space: 4 bytes per spill slot
        let spill_space = spill_count * 4;

        // Total, rounded to 16-byte alignment
        let total = (header_size + spill_space + 15) & !15;

        Self {
            size: total,
            saved_ra: spill_count > 0 || true, // conservative: assume non-leaf
            saved_s0: true,                    // always use frame pointer for M1+
            stack_slots: Vec::new(),
            spill_count,
            s0_save_offset: 0, // s0 saved at sp+0
            ra_save_offset: 4, // ra saved at sp+4
        }
    }

    /// Convert spill slot index to s0-relative offset.
    /// Slot 0 = -8, Slot 1 = -12, etc.
    pub fn spill_to_offset(&self, slot_index: u32) -> i32 {
        assert!(slot_index < self.spill_count);
        -((8 + slot_index * 4) as i32)
    }

    /// Get offset for a stack slot (LPIR slot) from s0.
    /// Stack slots come after spill area.
    pub fn stack_slot_offset(&self, slot_index: u32) -> i32 {
        assert!((slot_index as usize) < self.stack_slots.len());
        let spill_space = self.spill_count * 4;
        -((8 + spill_space + slot_index * 4) as i32)
    }
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

/// Return value classification for RV32 ILP32 calling convention.
/// Up to two scalar words: returned in `a0`–`a1`. More than two: sret pointer in `a0`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReturnClass {
    /// Return in registers a0–a1 (up to two scalar values).
    Direct { regs: Vec<PhysReg> },
    /// Return via struct return pointer in a0.
    /// Caller allocates buffer, passes address in a0 as first arg.
    Sret { ptr_reg: PhysReg },
}

impl ReturnClass {
    /// Classify return types for RV32 ILP32 ABI.
    ///
    /// Matches Cranelift / [`lpvm_cranelift::signature_for_ir_func`]: more than **two** scalar
    /// return words use a struct-return pointer in `a0` (see Cranelift #9510).
    pub fn from_lps_types(return_types: &[lps_shared::LpsType]) -> Self {
        let scalar_count: usize = return_types.iter().map(scalar_count_of_type).sum();

        if scalar_count > 2 {
            ReturnClass::Sret { ptr_reg: A0 }
        } else {
            let regs = RET_REGS.iter().copied().take(scalar_count).collect();
            ReturnClass::Direct { regs }
        }
    }

    /// Classify a single LpsType (convenience for single-return functions).
    pub fn from_lps_type(return_type: &lps_shared::LpsType) -> Self {
        Self::from_lps_types(&[return_type.clone()])
    }
}

/// Per-function ABI information for the RV32 calling convention.
/// Determines sret vs direct return and argument register assignment.
#[derive(Debug, Clone)]
pub struct AbiInfo {
    /// Return classification (Direct or Sret)
    pub return_class: ReturnClass,
    /// Physical registers for arguments (may be shifted for sret)
    pub arg_regs: Vec<PhysReg>,
    /// Size of sret buffer if applicable (bytes)
    pub sret_size: Option<u32>,
    /// Scalar count of return type
    pub return_scalar_count: u32,
}

impl AbiInfo {
    /// Derive ABI info from an LPIR function and its signature.
    ///
    /// For sret functions (>2 scalar return words), the buffer pointer is passed in a0,
    /// so real arguments start at a1.
    pub fn from_lps_sig(sig: &lps_shared::LpsFnSig) -> Self {
        let return_class = ReturnClass::from_lps_type(&sig.return_type);
        let return_scalar_count = scalar_count_of_type(&sig.return_type) as u32;

        let (arg_regs, sret_size) = match &return_class {
            ReturnClass::Sret { .. } => {
                // Sret: buffer ptr in a0, real args start at a1
                (ARG_REGS[1..].to_vec(), Some(return_scalar_count * 4))
            }
            ReturnClass::Direct { .. } => {
                // Direct: normal arg layout starting at a0
                (ARG_REGS.to_vec(), None)
            }
        };

        Self {
            return_class,
            arg_regs,
            sret_size,
            return_scalar_count,
        }
    }

    /// Returns true if this function uses the sret calling convention.
    pub fn is_sret(&self) -> bool {
        matches!(self.return_class, ReturnClass::Sret { .. })
    }

    /// Returns the offset into ARG_REGS for parameter assignment.
    /// 0 for direct returns (params start at a0), 1 for sret (params start at a1).
    pub fn arg_reg_offset(&self) -> usize {
        if self.is_sret() { 1 } else { 0 }
    }
}

/// Count scalar components in an LpsType.
fn scalar_count_of_type(ty: &lps_shared::LpsType) -> usize {
    use lps_shared::LpsType;
    match ty {
        LpsType::Void => 0,
        LpsType::Float | LpsType::Int | LpsType::UInt | LpsType::Bool => 1,
        LpsType::Vec2 | LpsType::IVec2 | LpsType::UVec2 | LpsType::BVec2 => 2,
        LpsType::Vec3 | LpsType::IVec3 | LpsType::UVec3 | LpsType::BVec3 => 3,
        LpsType::Vec4 | LpsType::IVec4 | LpsType::UVec4 | LpsType::BVec4 => 4,
        LpsType::Mat2 => 4,  // 2x2
        LpsType::Mat3 => 9,  // 3x3
        LpsType::Mat4 => 16, // 4x4
        LpsType::Array { element, len } => scalar_count_of_type(element) * *len as usize,
        LpsType::Struct { members, .. } => {
            members.iter().map(|m| scalar_count_of_type(&m.ty)).sum()
        }
    }
}

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

/// Primary return register for a scalar return (legacy API).
/// Use [`ReturnClass::from_lps_types`] for full return classification.
pub fn return_reg(_sig: &LpsFnSig) -> PhysReg {
    A0
}

/// Create a minimal leaf frame (no spills, no s0).
pub fn leaf_frame() -> FrameLayout {
    FrameLayout {
        size: 16, // Minimum 16-byte alignment
        saved_ra: false,
        saved_s0: false,
        stack_slots: Vec::new(),
        spill_count: 0,
        s0_save_offset: 0,
        ra_save_offset: 4,
    }
}

/// Create a non-leaf frame with potential spill slots.
pub fn nonleaf_frame(spill_count: u32) -> FrameLayout {
    FrameLayout::compute(spill_count)
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

    // Return classification tests
    use LpsType as Ty;

    #[test]
    fn classify_scalar_is_direct_a0() {
        let rc = ReturnClass::from_lps_types(&[Ty::Float]);
        assert!(matches!(rc, ReturnClass::Direct { regs } if regs == vec![A0]));
    }

    #[test]
    fn classify_vec2_is_direct_a0_a1() {
        let rc = ReturnClass::from_lps_types(&[Ty::Vec2]);
        assert!(matches!(rc, ReturnClass::Direct { regs } if regs == vec![A0, A1]));
    }

    #[test]
    fn classify_vec4_is_sret() {
        let rc = ReturnClass::from_lps_types(&[Ty::Vec4]);
        assert!(matches!(rc, ReturnClass::Sret { ptr_reg: A0 }));
    }

    #[test]
    fn classify_mat4_is_sret() {
        // Mat4 = 16 scalars, exceeds 4 register limit
        let rc = ReturnClass::from_lps_types(&[Ty::Mat4]);
        assert!(matches!(rc, ReturnClass::Sret { ptr_reg: A0 }));
    }

    #[test]
    fn classify_two_vec2_is_sret() {
        let rc = ReturnClass::from_lps_types(&[Ty::Vec2, Ty::Vec2]);
        assert!(matches!(rc, ReturnClass::Sret { .. }));
    }

    #[test]
    fn classify_vec4_scalar_is_sret() {
        let rc = ReturnClass::from_lps_types(&[Ty::Vec4, Ty::Float]);
        assert!(matches!(rc, ReturnClass::Sret { .. }));
    }

    #[test]
    fn classify_mat3_is_sret() {
        // Mat3 = 9 scalars, exceeds 4 register limit
        let rc = ReturnClass::from_lps_types(&[Ty::Mat3]);
        assert!(matches!(rc, ReturnClass::Sret { .. }));
    }

    #[test]
    fn classify_mat2_is_sret() {
        let rc = ReturnClass::from_lps_types(&[Ty::Mat2]);
        assert!(matches!(rc, ReturnClass::Sret { .. }));
    }

    #[test]
    fn classify_void_is_direct_empty() {
        let rc = ReturnClass::from_lps_types(&[Ty::Void]);
        assert!(matches!(rc, ReturnClass::Direct { regs } if regs.is_empty()));
    }

    #[test]
    fn classify_ivec3_is_sret() {
        let rc = ReturnClass::from_lps_types(&[Ty::IVec3]);
        assert!(matches!(rc, ReturnClass::Sret { .. }));
    }

    // Frame layout and stack slot tests
    #[test]
    fn leaf_frame_minimal_16_bytes() {
        let frame = leaf_frame();
        assert_eq!(frame.size, 16);
        assert!(!frame.saved_ra);
        assert!(!frame.saved_s0);
        assert_eq!(frame.spill_count, 0);
    }

    #[test]
    fn nonleaf_no_spills_16_bytes() {
        let frame = nonleaf_frame(0);
        // Header (8 bytes) aligned to 16
        assert_eq!(frame.size, 16);
        assert!(frame.saved_ra);
        assert!(frame.saved_s0);
    }

    #[test]
    fn nonleaf_two_spills_16_bytes() {
        let frame = nonleaf_frame(2);
        // Header (8) + spills (2 * 4 = 8) = 16
        assert_eq!(frame.size, 16);
        assert_eq!(frame.spill_count, 2);
    }

    #[test]
    fn nonleaf_three_spills_rounds_to_32() {
        let frame = nonleaf_frame(3);
        // Header (8) + spills (3 * 4 = 12) = 20 -> round to 32
        assert_eq!(frame.size, 32);
    }

    #[test]
    fn spill_offset_slot_0() {
        let frame = nonleaf_frame(2);
        assert_eq!(frame.spill_to_offset(0), -8);
    }

    #[test]
    fn spill_offset_slot_1() {
        let frame = nonleaf_frame(2);
        assert_eq!(frame.spill_to_offset(1), -12);
    }

    #[test]
    fn stack_slot_offset_computed() {
        let mut frame = nonleaf_frame(2);
        frame.stack_slots = alloc::vec![StackSlot {
            index: 0,
            size: 16,
            align: 16
        },];
        // After 2 spills (8 bytes), stack slot 0 is at -8 - 8 = -16
        assert_eq!(frame.stack_slot_offset(0), -16);
    }

    // AbiInfo tests
    #[test]
    fn abi_info_mat4_is_sret() {
        let sig = LpsFnSig {
            name: String::from("test"),
            return_type: LpsType::Mat4,
            parameters: vec![],
        };

        let abi = AbiInfo::from_lps_sig(&sig);
        assert!(
            matches!(abi.return_class, ReturnClass::Sret { ptr_reg: A0 }),
            "mat4 should use sret"
        );
        assert_eq!(abi.sret_size, Some(64), "mat4 = 16 scalars * 4 bytes");
        assert_eq!(abi.return_scalar_count, 16);
    }

    #[test]
    fn abi_info_vec4_is_sret() {
        let sig = LpsFnSig {
            name: String::from("test"),
            return_type: LpsType::Vec4,
            parameters: vec![],
        };

        let abi = AbiInfo::from_lps_sig(&sig);
        assert!(
            matches!(abi.return_class, ReturnClass::Sret { .. }),
            "vec4 uses sret (>2 scalars)"
        );
        assert_eq!(abi.sret_size, Some(16));
        assert_eq!(abi.return_scalar_count, 4);
    }

    #[test]
    fn abi_info_args_shifted_for_sret() {
        let sig = LpsFnSig {
            name: String::from("test"),
            return_type: LpsType::Mat4, // sret
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
        };

        let abi = AbiInfo::from_lps_sig(&sig);
        // For sret, args start at a1 (a0 holds sret pointer)
        assert_eq!(abi.arg_regs.len(), 7, "7 regs available (a1-a7)");
        assert_eq!(abi.arg_regs[0], A1, "first real arg in a1");
        assert_eq!(abi.arg_regs[1], ARG_REGS[2], "second real arg in a2");
    }

    #[test]
    fn abi_info_args_start_at_a0_for_direct() {
        let sig = LpsFnSig {
            name: String::from("test"),
            return_type: LpsType::Float, // direct
            parameters: vec![FnParam {
                name: String::from("a"),
                ty: LpsType::Float,
                qualifier: ParamQualifier::In,
            }],
        };

        let abi = AbiInfo::from_lps_sig(&sig);
        // For direct, args start at a0
        assert_eq!(abi.arg_regs.len(), 8, "8 regs available (a0-a7)");
        assert_eq!(abi.arg_regs[0], A0, "first arg in a0");
    }
}
