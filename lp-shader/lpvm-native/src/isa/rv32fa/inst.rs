//! Physical-register instructions (`PhysInst`).

use crate::vinst::SymbolRef;

pub use super::abi::PhysReg;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PhysInst {
    FrameSetup {
        spill_slots: u32,
    },
    FrameTeardown {
        spill_slots: u32,
    },

    Add {
        dst: PhysReg,
        src1: PhysReg,
        src2: PhysReg,
    },
    Sub {
        dst: PhysReg,
        src1: PhysReg,
        src2: PhysReg,
    },
    Mul {
        dst: PhysReg,
        src1: PhysReg,
        src2: PhysReg,
    },
    Div {
        dst: PhysReg,
        src1: PhysReg,
        src2: PhysReg,
    },
    Divu {
        dst: PhysReg,
        src1: PhysReg,
        src2: PhysReg,
    },
    Rem {
        dst: PhysReg,
        src1: PhysReg,
        src2: PhysReg,
    },
    Remu {
        dst: PhysReg,
        src1: PhysReg,
        src2: PhysReg,
    },

    And {
        dst: PhysReg,
        src1: PhysReg,
        src2: PhysReg,
    },
    Or {
        dst: PhysReg,
        src1: PhysReg,
        src2: PhysReg,
    },
    Xor {
        dst: PhysReg,
        src1: PhysReg,
        src2: PhysReg,
    },

    Sll {
        dst: PhysReg,
        src1: PhysReg,
        src2: PhysReg,
    },
    Srl {
        dst: PhysReg,
        src1: PhysReg,
        src2: PhysReg,
    },
    Sra {
        dst: PhysReg,
        src1: PhysReg,
        src2: PhysReg,
    },

    Neg {
        dst: PhysReg,
        src: PhysReg,
    },
    Not {
        dst: PhysReg,
        src: PhysReg,
    },
    Mv {
        dst: PhysReg,
        src: PhysReg,
    },

    Slt {
        dst: PhysReg,
        src1: PhysReg,
        src2: PhysReg,
    },
    Sltu {
        dst: PhysReg,
        src1: PhysReg,
        src2: PhysReg,
    },
    Seqz {
        dst: PhysReg,
        src: PhysReg,
    },
    Snez {
        dst: PhysReg,
        src: PhysReg,
    },
    Sltz {
        dst: PhysReg,
        src: PhysReg,
    },
    Sgtz {
        dst: PhysReg,
        src: PhysReg,
    },

    Li {
        dst: PhysReg,
        imm: i32,
    },
    Addi {
        dst: PhysReg,
        src: PhysReg,
        imm: i32,
    },

    Lw {
        dst: PhysReg,
        base: PhysReg,
        offset: i32,
    },
    Sw {
        src: PhysReg,
        base: PhysReg,
        offset: i32,
    },

    SlotAddr {
        dst: PhysReg,
        slot: u32,
    },
    MemcpyWords {
        dst: PhysReg,
        src: PhysReg,
        size: u32,
    },

    Call {
        target: SymbolRef,
    },
    Ret,

    Beq {
        src1: PhysReg,
        src2: PhysReg,
        target: u32,
    },
    Bne {
        src1: PhysReg,
        src2: PhysReg,
        target: u32,
    },
    Blt {
        src1: PhysReg,
        src2: PhysReg,
        target: u32,
    },
    Bge {
        src1: PhysReg,
        src2: PhysReg,
        target: u32,
    },
    J {
        target: u32,
    },
}

impl PhysInst {
    pub fn mnemonic(&self) -> &'static str {
        match self {
            PhysInst::FrameSetup { .. } => "FrameSetup",
            PhysInst::FrameTeardown { .. } => "FrameTeardown",
            PhysInst::Add { .. } => "add",
            PhysInst::Sub { .. } => "sub",
            PhysInst::Mul { .. } => "mul",
            PhysInst::Div { .. } => "div",
            PhysInst::Divu { .. } => "divu",
            PhysInst::Rem { .. } => "rem",
            PhysInst::Remu { .. } => "remu",
            PhysInst::And { .. } => "and",
            PhysInst::Or { .. } => "or",
            PhysInst::Xor { .. } => "xor",
            PhysInst::Sll { .. } => "sll",
            PhysInst::Srl { .. } => "srl",
            PhysInst::Sra { .. } => "sra",
            PhysInst::Neg { .. } => "neg",
            PhysInst::Not { .. } => "not",
            PhysInst::Mv { .. } => "mv",
            PhysInst::Slt { .. } => "slt",
            PhysInst::Sltu { .. } => "sltu",
            PhysInst::Seqz { .. } => "seqz",
            PhysInst::Snez { .. } => "snez",
            PhysInst::Sltz { .. } => "sltz",
            PhysInst::Sgtz { .. } => "sgtz",
            PhysInst::Li { .. } => "li",
            PhysInst::Addi { .. } => "addi",
            PhysInst::Lw { .. } => "lw",
            PhysInst::Sw { .. } => "sw",
            PhysInst::SlotAddr { .. } => "SlotAddr",
            PhysInst::MemcpyWords { .. } => "MemcpyWords",
            PhysInst::Call { .. } => "call",
            PhysInst::Ret => "ret",
            PhysInst::Beq { .. } => "beq",
            PhysInst::Bne { .. } => "bne",
            PhysInst::Blt { .. } => "blt",
            PhysInst::Bge { .. } => "bge",
            PhysInst::J { .. } => "j",
        }
    }
}
