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

/// Compute the `LEND` value a `loop`/`loopnez`/`loopgtz` at `pc` latches.
///
/// **The formula is `LEND = pc + 4 + imm8`**, with `imm8` unsigned (0..=255) —
/// the same `pc + 4` base every Xtensa PC-relative branch uses, independent of
/// the instruction's own 3-byte width. Verified against `xtensa-esp32-elf-as`
/// and `xtensa-esp32s3-elf-as` at five offsets spanning the field, including
/// the wraparound-looking ones (`imm8 = 0xfb` at pc 0x1b resolves to 0x11a, not
/// backwards).
///
/// It lives here, once, because a decoder that silently drops the loop
/// back-edge produces a wrong answer with no fault: everything that needs
/// `LEND` — the machine hart, the block-discovery sweep, the disassembler —
/// must get it from the same place rather than each redoing the arithmetic.
#[inline]
pub fn loop_end(pc: u32, imm8: u8) -> u32 {
    pc.wrapping_add(4).wrapping_add(imm8 as u32)
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
        AtomicLs(op, at, ars, off) => {
            let m = match op {
                AtomicLsOp::L32ai => "l32ai",
                AtomicLsOp::S32ri => "s32ri",
                AtomicLsOp::S32c1i => "s32c1i",
            };
            format!("{m}\t{at:?}, {ars:?}, {off}")
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
        Loop(op, ars, imm) => {
            let m = match op {
                LoopOp::Loop => "loop",
                LoopOp::Loopnez => "loopnez",
                LoopOp::Loopgtz => "loopgtz",
            };
            format!("{m}\t{ars:?}, {:#x}", loop_end(pc, imm))
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
        Clamps(rd, rs, imm) => format!("clamps\t{rd:?}, {rs:?}, {imm}"),
        BoolLogic(op, br, bs, bt) => {
            let m = match op {
                BoolOp::Andb => "andb",
                BoolOp::Andbc => "andbc",
                BoolOp::Orb => "orb",
                BoolOp::Orbc => "orbc",
                BoolOp::Xorb => "xorb",
            };
            format!("{m}\t{br:?}, {bs:?}, {bt:?}")
        }
        BoolAll(op, br, bs) => {
            let m = match op {
                BoolAllOp::Any4 => "any4",
                BoolAllOp::All4 => "all4",
                BoolAllOp::Any8 => "any8",
                BoolAllOp::All8 => "all8",
            };
            format!("{m}\t{br:?}, {bs:?}")
        }
        Tlb(op, at, ars) => {
            let m = match op {
                TlbOp::Ritlb0 => "ritlb0",
                TlbOp::Pitlb => "pitlb",
                TlbOp::Witlb => "witlb",
                TlbOp::Ritlb1 => "ritlb1",
                TlbOp::Rdtlb0 => "rdtlb0",
                TlbOp::Pdtlb => "pdtlb",
                TlbOp::Wdtlb => "wdtlb",
                TlbOp::Rdtlb1 => "rdtlb1",
            };
            format!("{m}\t{at:?}, {ars:?}")
        }
        TlbInv(data, ars) => {
            let m = if data { "idtlb" } else { "iitlb" };
            format!("{m}\t{ars:?}")
        }
        ExtReg(write, at, ars) => {
            let m = if write { "wer" } else { "rer" };
            format!("{m}\t{at:?}, {ars:?}")
        }
        Rf(op) => match op {
            RfOp::Rfe => "rfe".to_string(),
            RfOp::Rfde => "rfde".to_string(),
            RfOp::Rfwo => "rfwo".to_string(),
            RfOp::Rfwu => "rfwu".to_string(),
        },
        Rfi(level) => format!("rfi\t{level}"),
        Rsil(at, level) => format!("rsil\t{at:?}, {level}"),
        Waiti(level) => format!("waiti\t{level}"),
        Rotw(imm) => format!("rotw\t{imm}"),
        WindowLs(op, at, ars, off) => {
            let m = match op {
                WindowLsOp::L32e => "l32e",
                WindowLsOp::S32e => "s32e",
            };
            format!("{m}\t{at:?}, {ars:?}, {off}")
        }
        Break(imms, immt) => format!("break\t{imms}, {immt}"),
        BreakN(imms) => format!("break.n\t{imms}"),
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

        // --- floating point ---
        FpRrr(op, fr, fs, ft) => format!("{}\t{fr:?}, {fs:?}, {ft:?}", fp_rrr_mnem(op)),
        FpRr(op, fr, fs) => format!("{}\t{fr:?}, {fs:?}", fp_rr_mnem(op)),
        ConstS(fr, imm) => format!("const.s\t{fr:?}, {imm}"),
        Rfr(ar, fs) => format!("rfr\t{ar:?}, {fs:?}"),
        Wfr(fr, ars) => format!("wfr\t{fr:?}, {ars:?}"),
        FpMovAr(op, fr, fs, at) => {
            let m = match op {
                FpMovArOp::MoveqzS => "moveqz.s",
                FpMovArOp::MovnezS => "movnez.s",
                FpMovArOp::MovltzS => "movltz.s",
                FpMovArOp::MovgezS => "movgez.s",
            };
            format!("{m}\t{fr:?}, {fs:?}, {at:?}")
        }
        FpMovBr(op, fr, fs, bt) => {
            let m = match op {
                FpMovBrOp::MovfS => "movf.s",
                FpMovBrOp::MovtS => "movt.s",
            };
            format!("{m}\t{fr:?}, {fs:?}, {bt:?}")
        }
        FpCmp(op, br, fs, ft) => {
            let m = match op {
                FpCmpOp::UnS => "un.s",
                FpCmpOp::OeqS => "oeq.s",
                FpCmpOp::UeqS => "ueq.s",
                FpCmpOp::OltS => "olt.s",
                FpCmpOp::UltS => "ult.s",
                FpCmpOp::OleS => "ole.s",
                FpCmpOp::UleS => "ule.s",
            };
            format!("{m}\t{br:?}, {fs:?}, {ft:?}")
        }
        FpToInt(op, ar, fs, imm) => {
            let m = match op {
                FpToIntOp::RoundS => "round.s",
                FpToIntOp::TruncS => "trunc.s",
                FpToIntOp::FloorS => "floor.s",
                FpToIntOp::CeilS => "ceil.s",
                FpToIntOp::UtruncS => "utrunc.s",
            };
            format!("{m}\t{ar:?}, {fs:?}, {imm}")
        }
        IntToFp(op, fr, ars, imm) => {
            let m = match op {
                IntToFpOp::FloatS => "float.s",
                IntToFpOp::UfloatS => "ufloat.s",
            };
            format!("{m}\t{fr:?}, {ars:?}, {imm}")
        }
        FpLsx(op, fr, ars, at) => {
            let m = match op {
                FpLsxOp::Lsx => "lsx",
                FpLsxOp::Lsxp => "lsxp",
                FpLsxOp::Ssx => "ssx",
                FpLsxOp::Ssxp => "ssxp",
            };
            format!("{m}\t{fr:?}, {ars:?}, {at:?}")
        }
        FpLsi(op, ft, ars, off) => {
            let m = match op {
                FpLsiOp::Lsi => "lsi",
                FpLsiOp::Ssi => "ssi",
                FpLsiOp::Lsip => "lsip",
                FpLsiOp::Ssip => "ssip",
            };
            format!("{m}\t{ft:?}, {ars:?}, {off}")
        }

        // --- boolean register file ---
        MovBool(set, ar, ars, bt) => {
            let m = if set { "movt" } else { "movf" };
            format!("{m}\t{ar:?}, {ars:?}, {bt:?}")
        }
        BranchBool(set, bs, off) => {
            let m = if set { "bt" } else { "bf" };
            format!("{m}\t{bs:?}, {:#x}", br_target(pc, off))
        }

        // --- special / user registers ---
        Sr(op, sreg, at) => format!("{}.{}\t{at:?}", op.name(), sreg.name_for(op)),
        Ur(op, ureg, at) => format!("{}.{}\t{at:?}", op.name(), ureg.name()),
    }
}

fn fp_rrr_mnem(op: FpRrrOp) -> &'static str {
    match op {
        FpRrrOp::AddS => "add.s",
        FpRrrOp::SubS => "sub.s",
        FpRrrOp::MulS => "mul.s",
        FpRrrOp::MaddS => "madd.s",
        FpRrrOp::MsubS => "msub.s",
        FpRrrOp::MaddnS => "maddn.s",
        FpRrrOp::DivnS => "divn.s",
    }
}

fn fp_rr_mnem(op: FpRrOp) -> &'static str {
    match op {
        FpRrOp::MovS => "mov.s",
        FpRrOp::AbsS => "abs.s",
        FpRrOp::NegS => "neg.s",
        FpRrOp::Div0S => "div0.s",
        FpRrOp::Recip0S => "recip0.s",
        FpRrOp::Sqrt0S => "sqrt0.s",
        FpRrOp::Rsqrt0S => "rsqrt0.s",
        FpRrOp::Nexp01S => "nexp01.s",
        FpRrOp::MksadjS => "mksadj.s",
        FpRrOp::MkdadjS => "mkdadj.s",
        FpRrOp::AddexpS => "addexp.s",
        FpRrOp::AddexpmS => "addexpm.s",
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
