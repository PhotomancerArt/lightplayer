//! Physical-register instructions (`PInst`).

use crate::vinst::SymbolRef;

pub use super::gpr::PReg;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PInst {
    FrameSetup { spill_slots: u32 },
    FrameTeardown { spill_slots: u32 },

    Add { dst: PReg, src1: PReg, src2: PReg },
    Sub { dst: PReg, src1: PReg, src2: PReg },
    Mul { dst: PReg, src1: PReg, src2: PReg },
    Div { dst: PReg, src1: PReg, src2: PReg },
    Divu { dst: PReg, src1: PReg, src2: PReg },
    Rem { dst: PReg, src1: PReg, src2: PReg },
    Remu { dst: PReg, src1: PReg, src2: PReg },

    And { dst: PReg, src1: PReg, src2: PReg },
    Or { dst: PReg, src1: PReg, src2: PReg },
    Xor { dst: PReg, src1: PReg, src2: PReg },

    Sll { dst: PReg, src1: PReg, src2: PReg },
    Srl { dst: PReg, src1: PReg, src2: PReg },
    Sra { dst: PReg, src1: PReg, src2: PReg },

    Neg { dst: PReg, src: PReg },
    Not { dst: PReg, src: PReg },
    Mv { dst: PReg, src: PReg },

    Slt { dst: PReg, src1: PReg, src2: PReg },
    Sltu { dst: PReg, src1: PReg, src2: PReg },
    Seqz { dst: PReg, src: PReg },
    Snez { dst: PReg, src: PReg },
    Sltz { dst: PReg, src: PReg },
    Sgtz { dst: PReg, src: PReg },

    Li { dst: PReg, imm: i32 },
    Addi { dst: PReg, src: PReg, imm: i32 },

    Lw { dst: PReg, base: PReg, offset: i32 },
    Sw { src: PReg, base: PReg, offset: i32 },

    SlotAddr { dst: PReg, slot: u32 },
    MemcpyWords { dst: PReg, src: PReg, size: u32 },

    Call { target: SymbolRef },
    Ret,

    Beq { src1: PReg, src2: PReg, target: u32 },
    Bne { src1: PReg, src2: PReg, target: u32 },
    Blt { src1: PReg, src2: PReg, target: u32 },
    Bge { src1: PReg, src2: PReg, target: u32 },
    J { target: u32 },
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
