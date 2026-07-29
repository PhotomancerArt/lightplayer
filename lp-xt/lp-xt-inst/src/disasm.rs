//! objdump-style disassembly. `format_instruction` decodes then renders a
//! `mnemonic\toperands` string, resolving PC-relative targets to absolute
//! addresses when a PC is supplied.

use crate::decode::{DecodeError, decode};
use crate::*;
use alloc::format;
use alloc::string::{String, ToString};

/// Compute a branch target: `pc + 4 + offset`.
#[inline]
fn br_target(pc: u32, offset: i32) -> u32 {
    pc.wrapping_add(4).wrapping_add(offset as u32)
}

/// Compute a `CALLn` target: `(pc & !3) + (offset << 2) + 4`.
#[inline]
fn call_target(pc: u32, word_offset: i32) -> u32 {
    (pc & !3)
        .wrapping_add((word_offset as u32) << 2)
        .wrapping_add(4)
}

/// Compute an `l32r` literal address from the raw 16-bit field (always backward).
#[inline]
pub fn l32r_target(pc: u32, imm16: u16) -> u32 {
    let neg = (imm16 as i32) - 0x1_0000; // -65536..=-1
    ((pc.wrapping_add(3)) & !3).wrapping_add((neg << 2) as u32)
}

/// Decode and format one instruction at `pc`. On an unsupported opcode, renders
/// a `.unsupported` placeholder carrying the raw word (never panics).
pub fn format_instruction(bytes: &[u8], pc: u32) -> String {
    match decode(bytes) {
        Ok((inst, _len)) => format_inst(&inst, pc),
        Err(DecodeError::Unsupported { word, len }) => {
            format!(".unsupported\t{word:#0width$x}", width = 2 + 2 * len)
        }
        Err(DecodeError::Truncated { .. }) => ".truncated".to_string(),
    }
}

/// Format an already-decoded [`Inst`] at `pc`.
pub fn format_inst(inst: &Inst, pc: u32) -> String {
    use Inst::*;
    match *inst {
        Rrr(op, rd, rs, rt) => {
            format!("{}\t{:?}, {:?}, {:?}", rrr_mnem(op), rd, rs, rt)
        }
        Rt(op, rd, rt) => {
            let m = match op {
                AluRt::Neg => "neg",
                AluRt::Abs => "abs",
                AluRt::Sra => "sra",
                AluRt::Srl => "srl",
                AluRt::Nsa => "nsa",
                AluRt::Nsau => "nsau",
            };
            format!("{m}\t{rd:?}, {rt:?}")
        }
        Rs(op, rd, rs) => {
            let m = match op {
                AluRs::Sll => "sll",
                AluRs::Movsp => "movsp",
            };
            format!("{m}\t{rd:?}, {rs:?}")
        }
        ShiftSet(op, rs) => {
            let m = match op {
                ShiftSetOp::Ssr => "ssr",
                ShiftSetOp::Ssl => "ssl",
                ShiftSetOp::Ssa8l => "ssa8l",
                ShiftSetOp::Ssa8b => "ssa8b",
            };
            format!("{m}\t{rs:?}")
        }
        Ssai(imm) => format!("ssai\t{imm}"),
        Slli(rd, rs, sa) => format!("slli\t{rd:?}, {rs:?}, {sa}"),
        Srli(rd, rt, sa) => format!("srli\t{rd:?}, {rt:?}, {sa}"),
        Srai(rd, rt, sa) => format!("srai\t{rd:?}, {rt:?}, {sa}"),
        Extui(rd, rt, shiftimm, maskimm) => {
            format!("extui\t{rd:?}, {rt:?}, {shiftimm}, {maskimm}")
        }
        Sext(rd, rs, imm) => format!("sext\t{rd:?}, {rs:?}, {imm}"),
        MovN(rt, rs) => format!("mov.n\t{rt:?}, {rs:?}"),
        AddN(rd, rs, rt) => format!("add.n\t{rd:?}, {rs:?}, {rt:?}"),
        AddiN(rd, rs, imm) => format!("addi.n\t{rd:?}, {rs:?}, {imm}"),
        Addi(rt, rs, imm) => format!("addi\t{rt:?}, {rs:?}, {imm}"),
        Addmi(rt, rs, imm) => format!("addmi\t{rt:?}, {rs:?}, {imm}"),
        Movi(rt, imm) => format!("movi\t{rt:?}, {imm}"),
        MoviN(rt, imm) => format!("movi.n\t{rt:?}, {imm}"),
        Load(op, rt, rs, off) => {
            let m = match op {
                LoadOp::L8ui => "l8ui",
                LoadOp::L16ui => "l16ui",
                LoadOp::L16si => "l16si",
                LoadOp::L32i => "l32i",
            };
            format!("{m}\t{rt:?}, {rs:?}, {off}")
        }
        Store(op, rt, rs, off) => {
            let m = match op {
                StoreOp::S8i => "s8i",
                StoreOp::S16i => "s16i",
                StoreOp::S32i => "s32i",
            };
            format!("{m}\t{rt:?}, {rs:?}, {off}")
        }
        L32iN(rt, rs, off) => format!("l32i.n\t{rt:?}, {rs:?}, {off}"),
        S32iN(rt, rs, off) => format!("s32i.n\t{rt:?}, {rs:?}, {off}"),
        L32r(rt, imm16) => {
            let target = l32r_target(pc, imm16);
            format!("l32r\t{rt:?}, {target:#x}")
        }
        BranchRr(op, rs, rt, off) => {
            let m = match op {
                BrRr::Beq => "beq",
                BrRr::Bne => "bne",
                BrRr::Blt => "blt",
                BrRr::Bge => "bge",
                BrRr::Bltu => "bltu",
                BrRr::Bgeu => "bgeu",
                BrRr::Ball => "ball",
                BrRr::Bany => "bany",
                BrRr::Bnall => "bnall",
                BrRr::Bnone => "bnone",
                BrRr::Bbc => "bbc",
                BrRr::Bbs => "bbs",
            };
            format!("{m}\t{rs:?}, {rt:?}, {:#x}", br_target(pc, off))
        }
        BranchRi(op, rs, val, off) => {
            let m = match op {
                BrRi::Beqi => "beqi",
                BrRi::Bnei => "bnei",
                BrRi::Blti => "blti",
                BrRi::Bgei => "bgei",
            };
            format!("{m}\t{rs:?}, {val}, {:#x}", br_target(pc, off))
        }
        BranchRiu(op, rs, val, off) => {
            let m = match op {
                BrRiu::Bltui => "bltui",
                BrRiu::Bgeui => "bgeui",
            };
            format!("{m}\t{rs:?}, {val}, {:#x}", br_target(pc, off))
        }
        BranchZ(op, rs, off) => {
            let m = match op {
                BrZ::Beqz => "beqz",
                BrZ::Bnez => "bnez",
                BrZ::Bltz => "bltz",
                BrZ::Bgez => "bgez",
            };
            format!("{m}\t{rs:?}, {:#x}", br_target(pc, off))
        }
        BranchBiI(set, rs, imm, off) => {
            let m = if set { "bbsi" } else { "bbci" };
            format!("{m}\t{rs:?}, {imm}, {:#x}", br_target(pc, off))
        }
        BranchZN(nez, rs, imm6) => {
            let m = if nez { "bnez.n" } else { "beqz.n" };
            format!("{m}\t{rs:?}, {:#x}", br_target(pc, imm6 as i32))
        }
        J(off) => format!("j\t{:#x}", br_target(pc, off)),
        Jx(rs) => format!("jx\t{rs:?}"),
        Call(op, off) => {
            let m = match op {
                CallOp::Call0 => "call0",
                CallOp::Call4 => "call4",
                CallOp::Call8 => "call8",
                CallOp::Call12 => "call12",
            };
            format!("{m}\t{:#x}", call_target(pc, off))
        }
        Callx(op, rs) => {
            let m = match op {
                CallxOp::Callx0 => "callx0",
                CallxOp::Callx4 => "callx4",
                CallxOp::Callx8 => "callx8",
                CallxOp::Callx12 => "callx12",
            };
            format!("{m}\t{rs:?}")
        }
        Entry(rs, imm) => format!("entry\t{rs:?}, {imm}"),
        Nullary(op) => {
            let m = match op {
                NullaryOp::Memw => "memw",
                NullaryOp::Extw => "extw",
                NullaryOp::Isync => "isync",
                NullaryOp::Rsync => "rsync",
                NullaryOp::Esync => "esync",
                NullaryOp::Dsync => "dsync",
                NullaryOp::Nop => "nop",
                NullaryOp::Ret => "ret",
                NullaryOp::Retw => "retw",
                NullaryOp::Ill => "ill",
                NullaryOp::Syscall => "syscall",
            };
            m.to_string()
        }
        NullaryN(op) => {
            let m = match op {
                NullaryNarrowOp::RetN => "ret.n",
                NullaryNarrowOp::RetwN => "retw.n",
                NullaryNarrowOp::NopN => "nop.n",
                NullaryNarrowOp::IllN => "ill.n",
            };
            m.to_string()
        }
    }
}

fn rrr_mnem(op: AluRrr) -> &'static str {
    match op {
        AluRrr::And => "and",
        AluRrr::Or => "or",
        AluRrr::Xor => "xor",
        AluRrr::Add => "add",
        AluRrr::Sub => "sub",
        AluRrr::Addx2 => "addx2",
        AluRrr::Addx4 => "addx4",
        AluRrr::Addx8 => "addx8",
        AluRrr::Subx2 => "subx2",
        AluRrr::Subx4 => "subx4",
        AluRrr::Subx8 => "subx8",
        AluRrr::Src => "src",
        AluRrr::Mull => "mull",
        AluRrr::Muluh => "muluh",
        AluRrr::Mulsh => "mulsh",
        AluRrr::Quou => "quou",
        AluRrr::Quos => "quos",
        AluRrr::Remu => "remu",
        AluRrr::Rems => "rems",
        AluRrr::Min => "min",
        AluRrr::Max => "max",
        AluRrr::Minu => "minu",
        AluRrr::Maxu => "maxu",
        AluRrr::Mul16u => "mul16u",
        AluRrr::Mul16s => "mul16s",
        AluRrr::Moveqz => "moveqz",
        AluRrr::Movnez => "movnez",
        AluRrr::Movltz => "movltz",
        AluRrr::Movgez => "movgez",
    }
}
