//! Virtual instructions: post-lowering, pre-regalloc.

use alloc::string::String;
use alloc::vec::Vec;

pub use lpir::VReg;

/// Callee symbol for [`VInst::Call`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SymbolRef {
    pub name: String,
}

/// Label id for future control-flow lowering.
pub type LabelId = u32;

/// Integer comparison condition for [`VInst::Icmp32`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IcmpCond {
    Eq,
    Ne,
    LtS,
    LeS,
    GtS,
    GeS,
    LtU,
    LeU,
    GtU,
    GeU,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VInst {
    Add32 {
        dst: VReg,
        src1: VReg,
        src2: VReg,
        src_op: Option<u32>,
    },
    Sub32 {
        dst: VReg,
        src1: VReg,
        src2: VReg,
        src_op: Option<u32>,
    },
    /// Negate: 0 - src (uses hardware x0 register)
    Neg32 {
        dst: VReg,
        src: VReg,
        src_op: Option<u32>,
    },
    Mul32 {
        dst: VReg,
        src1: VReg,
        src2: VReg,
        src_op: Option<u32>,
    },
    And32 {
        dst: VReg,
        src1: VReg,
        src2: VReg,
        src_op: Option<u32>,
    },
    Or32 {
        dst: VReg,
        src1: VReg,
        src2: VReg,
        src_op: Option<u32>,
    },
    Xor32 {
        dst: VReg,
        src1: VReg,
        src2: VReg,
        src_op: Option<u32>,
    },
    /// Bitwise not: `xori dst, src, -1`
    Bnot32 {
        dst: VReg,
        src: VReg,
        src_op: Option<u32>,
    },
    Shl32 {
        dst: VReg,
        src1: VReg,
        src2: VReg,
        src_op: Option<u32>,
    },
    ShrS32 {
        dst: VReg,
        src1: VReg,
        src2: VReg,
        src_op: Option<u32>,
    },
    ShrU32 {
        dst: VReg,
        src1: VReg,
        src2: VReg,
        src_op: Option<u32>,
    },
    DivS32 {
        dst: VReg,
        lhs: VReg,
        rhs: VReg,
        src_op: Option<u32>,
    },
    DivU32 {
        dst: VReg,
        lhs: VReg,
        rhs: VReg,
        src_op: Option<u32>,
    },
    RemS32 {
        dst: VReg,
        lhs: VReg,
        rhs: VReg,
        src_op: Option<u32>,
    },
    RemU32 {
        dst: VReg,
        lhs: VReg,
        rhs: VReg,
        src_op: Option<u32>,
    },
    Icmp32 {
        dst: VReg,
        lhs: VReg,
        rhs: VReg,
        cond: IcmpCond,
        src_op: Option<u32>,
    },
    /// `src == imm` as i32 0/1.
    IeqImm32 {
        dst: VReg,
        src: VReg,
        imm: i32,
        src_op: Option<u32>,
    },
    /// Branchless `cond ? if_true : if_false` with `cond` in {0, 1}.
    Select32 {
        dst: VReg,
        cond: VReg,
        if_true: VReg,
        if_false: VReg,
        src_op: Option<u32>,
    },
    /// Unconditional jump to `target` (`jal x0, offset`).
    Br {
        target: LabelId,
        src_op: Option<u32>,
    },
    /// Conditional branch: `invert == true` → branch when `cond == 0` (`beq`);
    /// `invert == false` → branch when `cond != 0` (`bne`).
    BrIf {
        cond: VReg,
        target: LabelId,
        invert: bool,
        src_op: Option<u32>,
    },
    /// `addi dst, src, 0` — used for LPIR `Copy` when registers differ.
    Mov32 {
        dst: VReg,
        src: VReg,
        src_op: Option<u32>,
    },
    Load32 {
        dst: VReg,
        base: VReg,
        offset: i32,
        src_op: Option<u32>,
    },
    Store32 {
        src: VReg,
        base: VReg,
        offset: i32,
        src_op: Option<u32>,
    },
    IConst32 {
        dst: VReg,
        val: i32,
        src_op: Option<u32>,
    },
    Call {
        target: SymbolRef,
        args: Vec<VReg>,
        rets: Vec<VReg>,
        /// Callee returns via hidden sret pointer in a0; caller must pass buffer and load results.
        callee_uses_sret: bool,
        src_op: Option<u32>,
    },
    Ret {
        vals: Vec<VReg>,
        src_op: Option<u32>,
    },
    Label(LabelId, Option<u32>),
}

impl VInst {
    /// Index of the originating LPIR op in [`lpir::IrFunction::body`], when tracked.
    pub fn src_op(&self) -> Option<u32> {
        match self {
            VInst::Add32 { src_op, .. }
            | VInst::Sub32 { src_op, .. }
            | VInst::Neg32 { src_op, .. }
            | VInst::Mul32 { src_op, .. }
            | VInst::And32 { src_op, .. }
            | VInst::Or32 { src_op, .. }
            | VInst::Xor32 { src_op, .. }
            | VInst::Bnot32 { src_op, .. }
            | VInst::Shl32 { src_op, .. }
            | VInst::ShrS32 { src_op, .. }
            | VInst::ShrU32 { src_op, .. }
            | VInst::DivS32 { src_op, .. }
            | VInst::DivU32 { src_op, .. }
            | VInst::RemS32 { src_op, .. }
            | VInst::RemU32 { src_op, .. }
            | VInst::Icmp32 { src_op, .. }
            | VInst::IeqImm32 { src_op, .. }
            | VInst::Select32 { src_op, .. }
            | VInst::Br { src_op, .. }
            | VInst::BrIf { src_op, .. }
            | VInst::Mov32 { src_op, .. }
            | VInst::Load32 { src_op, .. }
            | VInst::Store32 { src_op, .. }
            | VInst::IConst32 { src_op, .. }
            | VInst::Call { src_op, .. }
            | VInst::Ret { src_op, .. } => *src_op,
            VInst::Label(_, src_op) => *src_op,
        }
    }

    /// VRegs written by this instruction.
    pub fn defs(&self) -> impl Iterator<Item = VReg> + '_ {
        let mut v = Vec::new();
        match self {
            VInst::Add32 { dst, .. }
            | VInst::Sub32 { dst, .. }
            | VInst::Neg32 { dst, .. }
            | VInst::Mul32 { dst, .. }
            | VInst::And32 { dst, .. }
            | VInst::Or32 { dst, .. }
            | VInst::Xor32 { dst, .. }
            | VInst::Bnot32 { dst, .. }
            | VInst::Shl32 { dst, .. }
            | VInst::ShrS32 { dst, .. }
            | VInst::ShrU32 { dst, .. }
            | VInst::DivS32 { dst, .. }
            | VInst::DivU32 { dst, .. }
            | VInst::RemS32 { dst, .. }
            | VInst::RemU32 { dst, .. }
            | VInst::Icmp32 { dst, .. }
            | VInst::IeqImm32 { dst, .. }
            | VInst::Select32 { dst, .. }
            | VInst::Mov32 { dst, .. }
            | VInst::Load32 { dst, .. }
            | VInst::IConst32 { dst, .. } => v.push(*dst),
            VInst::Store32 { .. } | VInst::Label(..) | VInst::Br { .. } | VInst::BrIf { .. } => {}
            VInst::Call { rets, .. } => v.extend(rets.iter().copied()),
            VInst::Ret { .. } => {}
        }
        v.into_iter()
    }

    /// VRegs read by this instruction.
    pub fn uses(&self) -> impl Iterator<Item = VReg> + '_ {
        let mut v = Vec::new();
        match self {
            VInst::Add32 { src1, src2, .. }
            | VInst::Sub32 { src1, src2, .. }
            | VInst::Mul32 { src1, src2, .. }
            | VInst::And32 { src1, src2, .. }
            | VInst::Or32 { src1, src2, .. }
            | VInst::Xor32 { src1, src2, .. }
            | VInst::Shl32 { src1, src2, .. }
            | VInst::ShrS32 { src1, src2, .. }
            | VInst::ShrU32 { src1, src2, .. }
            | VInst::DivS32 {
                lhs: src1,
                rhs: src2,
                ..
            }
            | VInst::DivU32 {
                lhs: src1,
                rhs: src2,
                ..
            }
            | VInst::RemS32 {
                lhs: src1,
                rhs: src2,
                ..
            }
            | VInst::RemU32 {
                lhs: src1,
                rhs: src2,
                ..
            }
            | VInst::Icmp32 {
                lhs: src1,
                rhs: src2,
                ..
            } => {
                v.push(*src1);
                v.push(*src2);
            }
            VInst::Select32 {
                cond,
                if_true,
                if_false,
                ..
            } => {
                v.push(*cond);
                v.push(*if_true);
                v.push(*if_false);
            }
            VInst::Neg32 { src, .. } | VInst::Bnot32 { src, .. } | VInst::IeqImm32 { src, .. } => {
                v.push(*src)
            }
            VInst::Mov32 { src, .. } => v.push(*src),
            VInst::Load32 { base, .. } => v.push(*base),
            VInst::Store32 { src, base, .. } => {
                v.push(*src);
                v.push(*base);
            }
            VInst::IConst32 { .. } | VInst::Label(..) | VInst::Br { .. } => {}
            VInst::BrIf { cond, .. } => v.push(*cond),
            VInst::Call { args, .. } => v.extend(args.iter().copied()),
            VInst::Ret { vals, .. } => v.extend(vals.iter().copied()),
        }
        v.into_iter()
    }

    /// True if this is a call (clobbers caller-saved registers).
    pub fn is_call(&self) -> bool {
        matches!(self, VInst::Call { .. })
    }
}
