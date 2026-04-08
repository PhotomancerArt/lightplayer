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
    },
    Sub32 {
        dst: VReg,
        src1: VReg,
        src2: VReg,
    },
    Mul32 {
        dst: VReg,
        src1: VReg,
        src2: VReg,
    },
    Load32 {
        dst: VReg,
        base: VReg,
        offset: i32,
    },
    Store32 {
        src: VReg,
        base: VReg,
        offset: i32,
    },
    IConst32 {
        dst: VReg,
        val: i32,
    },
    Call {
        target: SymbolRef,
        args: Vec<VReg>,
        rets: Vec<VReg>,
    },
    Ret {
        vals: Vec<VReg>,
    },
    Label(LabelId),
}

impl VInst {
    /// VRegs written by this instruction.
    pub fn defs(&self) -> impl Iterator<Item = VReg> + '_ {
        let mut v = Vec::new();
        match self {
            VInst::Add32 { dst, .. }
            | VInst::Sub32 { dst, .. }
            | VInst::Mul32 { dst, .. }
            | VInst::Load32 { dst, .. }
            | VInst::IConst32 { dst, .. } => v.push(*dst),
            VInst::Store32 { .. } | VInst::Label(_) => {}
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
            VInst::Load32 { base, .. } => v.push(*base),
            VInst::Store32 { src, base, .. } => {
                v.push(*src);
                v.push(*base);
            }
            VInst::IConst32 { .. } | VInst::Label(_) => {}
            VInst::Call { args, .. } => v.extend(args.iter().copied()),
            VInst::Ret { vals } => v.extend(vals.iter().copied()),
        }
        v.into_iter()
    }

    /// True if this is a call (clobbers caller-saved registers).
    pub fn is_call(&self) -> bool {
        matches!(self, VInst::Call { .. })
    }
}
