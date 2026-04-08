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
    Mul32 {
        dst: VReg,
        src1: VReg,
        src2: VReg,
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
            | VInst::Mul32 { src_op, .. }
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
            | VInst::Mul32 { dst, .. }
            | VInst::Mov32 { dst, .. }
            | VInst::Load32 { dst, .. }
            | VInst::IConst32 { dst, .. } => v.push(*dst),
            VInst::Store32 { .. } | VInst::Label(..) => {}
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
            | VInst::Mul32 { src1, src2, .. } => {
                v.push(*src1);
                v.push(*src2);
            }
            VInst::Mov32 { src, .. } => v.push(*src),
            VInst::Load32 { base, .. } => v.push(*base),
            VInst::Store32 { src, base, .. } => {
                v.push(*src);
                v.push(*base);
            }
            VInst::IConst32 { .. } | VInst::Label(..) => {}
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
