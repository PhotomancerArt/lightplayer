//! Physical-register instructions (`PInst`).

use crate::vinst::SymbolRef;

pub use super::abi::PhysReg;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PInst {
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

impl PInst {
    pub fn mnemonic(&self) -> &'static str {
        match self {
            PInst::FrameSetup { .. } => "FrameSetup",
            PInst::FrameTeardown { .. } => "FrameTeardown",
            PInst::Add { .. } => "add",
            PInst::Sub { .. } => "sub",
            PInst::Mul { .. } => "mul",
            PInst::Div { .. } => "div",
            PInst::Divu { .. } => "divu",
            PInst::Rem { .. } => "rem",
            PInst::Remu { .. } => "remu",
            PInst::And { .. } => "and",
            PInst::Or { .. } => "or",
            PInst::Xor { .. } => "xor",
            PInst::Sll { .. } => "sll",
            PInst::Srl { .. } => "srl",
            PInst::Sra { .. } => "sra",
            PInst::Neg { .. } => "neg",
            PInst::Not { .. } => "not",
            PInst::Mv { .. } => "mv",
            PInst::Slt { .. } => "slt",
            PInst::Sltu { .. } => "sltu",
            PInst::Seqz { .. } => "seqz",
            PInst::Snez { .. } => "snez",
            PInst::Sltz { .. } => "sltz",
            PInst::Sgtz { .. } => "sgtz",
            PInst::Li { .. } => "li",
            PInst::Addi { .. } => "addi",
            PInst::Lw { .. } => "lw",
            PInst::Sw { .. } => "sw",
            PInst::SlotAddr { .. } => "SlotAddr",
            PInst::MemcpyWords { .. } => "MemcpyWords",
            PInst::Call { .. } => "call",
            PInst::Ret => "ret",
            PInst::Beq { .. } => "beq",
            PInst::Bne { .. } => "bne",
            PInst::Blt { .. } => "blt",
            PInst::Bge { .. } => "bge",
            PInst::J { .. } => "j",
        }
    }
}
