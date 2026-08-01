//! Round-trip property tests: `decode(encode(i)) == (i, len)` over the whole
//! supported set, plus per-opcode immediate-range coverage (the ranges differ
//! per instruction, unlike rv32's uniform imm12).

use lp_xt_inst::*;

fn r(n: u8) -> Reg {
    Reg::new(n)
}
fn f(n: u8) -> FReg {
    FReg::new(n)
}
fn b(n: u8) -> BReg {
    BReg::new(n)
}

/// Assert `inst` encodes to `expect_len` bytes and decodes back to itself.
#[track_caller]
fn rt(inst: Inst, expect_len: usize) {
    let bytes = encode(&inst);
    assert_eq!(bytes.len(), expect_len, "encoded length for {inst:?}");
    // The density length rule must agree with the actual length.
    assert_eq!(base_inst_len(bytes[0]), expect_len, "len rule for {inst:?}");
    let (back, len) = decode(&bytes).unwrap_or_else(|e| panic!("decode {inst:?}: {e}"));
    assert_eq!(len, expect_len, "decoded length for {inst:?}");
    assert_eq!(back, inst, "round-trip mismatch, bytes={bytes:02x?}");
}

const REGS: [u8; 5] = [0, 1, 7, 8, 15];

#[test]
fn rrr_alu() {
    use AluRrr::*;
    let ops = [
        And, Or, Xor, Add, Sub, Addx2, Addx4, Addx8, Subx2, Subx4, Subx8, Src, Mull, Muluh, Mulsh,
        Quou, Quos, Remu, Rems, Min, Max, Minu, Maxu, Mul16u, Mul16s, Moveqz, Movnez, Movltz,
        Movgez,
    ];
    for op in ops {
        for &a in &REGS {
            for &b in &REGS {
                rt(Inst::Rrr(op, r(a), r(b), r(15 - a)), 3);
            }
        }
    }
}

#[test]
fn two_and_one_reg() {
    for op in [
        AluRt::Neg,
        AluRt::Abs,
        AluRt::Sra,
        AluRt::Srl,
        AluRt::Nsa,
        AluRt::Nsau,
    ] {
        for &a in &REGS {
            for &b in &REGS {
                rt(Inst::Rt(op, r(a), r(b)), 3);
            }
        }
    }
    for op in [AluRs::Sll, AluRs::Movsp] {
        for &a in &REGS {
            for &b in &REGS {
                rt(Inst::Rs(op, r(a), r(b)), 3);
            }
        }
    }
    for op in [
        ShiftSetOp::Ssr,
        ShiftSetOp::Ssl,
        ShiftSetOp::Ssa8l,
        ShiftSetOp::Ssa8b,
    ] {
        for &a in &REGS {
            rt(Inst::ShiftSet(op, r(a)), 3);
        }
    }
}

#[test]
fn shifts_and_extui() {
    for &a in &REGS {
        for &b in &REGS {
            for sa in 1..=32u8 {
                rt(Inst::Slli(r(a), r(b), sa), 3);
            }
            for sa in 0..=15u8 {
                rt(Inst::Srli(r(a), r(b), sa), 3);
            }
            for sa in 0..=31u8 {
                rt(Inst::Srai(r(a), r(b), sa), 3);
            }
            for shiftimm in [0u8, 1, 12, 31] {
                for maskimm in 1..=16u8 {
                    rt(Inst::Extui(r(a), r(b), shiftimm, maskimm), 3);
                }
            }
        }
    }
    for imm in 0..=31u8 {
        rt(Inst::Ssai(imm), 3);
    }
    for &a in &REGS {
        for &b in &REGS {
            for imm in 7..=22u8 {
                rt(Inst::Sext(r(a), r(b), imm), 3);
            }
        }
    }
}

#[test]
fn immediates_addi_movi() {
    for imm in -128..=127i32 {
        rt(Inst::Addi(r(3), r(4), imm), 3);
    }
    // addmi: -32768..=32512 step 256
    for k in -128..=127i32 {
        rt(Inst::Addmi(r(3), r(4), k * 256), 3);
    }
    for imm in -2048..=2047i32 {
        rt(Inst::Movi(r(9), imm), 3);
    }
    for imm in -32..=95i32 {
        rt(Inst::MoviN(r(9), imm), 2);
    }
    for imm in [-1i32, 1, 2, 7, 15] {
        rt(Inst::AddiN(r(3), r(4), imm), 2);
    }
}

#[test]
fn loads_stores() {
    // offsets scale per width; cover the full field.
    for off in (0..=1020).step_by(4) {
        rt(Inst::Load(LoadOp::L32i, r(2), r(3), off), 3);
        rt(Inst::Store(StoreOp::S32i, r(2), r(3), off), 3);
    }
    for off in 0..=255 {
        rt(Inst::Load(LoadOp::L8ui, r(2), r(3), off), 3);
        rt(Inst::Store(StoreOp::S8i, r(2), r(3), off), 3);
    }
    for off in (0..=510).step_by(2) {
        rt(Inst::Load(LoadOp::L16ui, r(2), r(3), off), 3);
        rt(Inst::Load(LoadOp::L16si, r(2), r(3), off), 3);
        rt(Inst::Store(StoreOp::S16i, r(2), r(3), off), 3);
    }
    for off in (0..=60).step_by(4) {
        rt(Inst::L32iN(r(2), r(3), off), 2);
        rt(Inst::S32iN(r(2), r(3), off), 2);
    }
}

#[test]
fn l32r_field() {
    for imm16 in [0u16, 1, 0x1234, 0xfffe, 0xffff] {
        rt(Inst::L32r(r(8), imm16), 3);
    }
}

#[test]
fn branches() {
    let offs = [-128i32, -1, 0, 1, 5, 127];
    for &off in &offs {
        for op in [
            BrRr::Beq,
            BrRr::Bne,
            BrRr::Blt,
            BrRr::Bge,
            BrRr::Bltu,
            BrRr::Bgeu,
            BrRr::Ball,
            BrRr::Bany,
            BrRr::Bnall,
            BrRr::Bnone,
            BrRr::Bbc,
            BrRr::Bbs,
        ] {
            rt(Inst::BranchRr(op, r(3), r(4), off), 3);
        }
        for op in [BrRi::Beqi, BrRi::Bnei, BrRi::Blti, BrRi::Bgei] {
            for &v in &B4CONST {
                rt(Inst::BranchRi(op, r(3), v, off), 3);
            }
        }
        for op in [BrRiu::Bltui, BrRiu::Bgeui] {
            for &v in &B4CONSTU {
                rt(Inst::BranchRiu(op, r(3), v, off), 3);
            }
        }
        for imm in [0u8, 5, 31] {
            rt(Inst::BranchBiI(true, r(3), imm, off), 3);
            rt(Inst::BranchBiI(false, r(3), imm, off), 3);
        }
    }
    // BRI12 zero-branches: signed 12-bit offset.
    for off in [-2048i32, -1, 0, 1, 2047] {
        for op in [BrZ::Beqz, BrZ::Bnez, BrZ::Bltz, BrZ::Bgez] {
            rt(Inst::BranchZ(op, r(5), off), 3);
        }
    }
    // Narrow zero-branches: unsigned 6-bit forward offset.
    for imm6 in 0..=63u32 {
        rt(Inst::BranchZN(true, r(5), imm6), 2);
        rt(Inst::BranchZN(false, r(5), imm6), 2);
    }
}

#[test]
fn calls_jumps_entry() {
    for off in [-131072i32, -1, 0, 1, 131071] {
        rt(Inst::J(off), 3);
        rt(Inst::Call(CallOp::Call0, off), 3);
        rt(Inst::Call(CallOp::Call4, off), 3);
        rt(Inst::Call(CallOp::Call8, off), 3);
        rt(Inst::Call(CallOp::Call12, off), 3);
    }
    for &a in &REGS {
        rt(Inst::Jx(r(a)), 3);
        rt(Inst::Callx(CallxOp::Callx0, r(a)), 3);
        rt(Inst::Callx(CallxOp::Callx4, r(a)), 3);
        rt(Inst::Callx(CallxOp::Callx8, r(a)), 3);
        rt(Inst::Callx(CallxOp::Callx12, r(a)), 3);
    }
    // entry: 0..=32760 step 8
    for k in (0..=32760).step_by(8) {
        rt(Inst::Entry(r(1), k), 3);
    }
}

#[test]
fn narrow_reg_and_nullary() {
    for &a in &REGS {
        for &b in &REGS {
            rt(Inst::MovN(r(a), r(b)), 2);
            rt(Inst::AddN(r(a), r(b), r(15 - a)), 2);
        }
    }
    for op in [
        NullaryOp::Memw,
        NullaryOp::Extw,
        NullaryOp::Isync,
        NullaryOp::Rsync,
        NullaryOp::Esync,
        NullaryOp::Dsync,
        NullaryOp::Nop,
        NullaryOp::Ret,
        NullaryOp::Retw,
        NullaryOp::Ill,
        NullaryOp::Syscall,
    ] {
        rt(Inst::Nullary(op), 3);
    }
    for op in [
        NullaryNarrowOp::RetN,
        NullaryNarrowOp::RetwN,
        NullaryNarrowOp::NopN,
        NullaryNarrowOp::IllN,
    ] {
        rt(Inst::NullaryN(op), 2);
    }
}

/// Every FP register field, over the register boundary set. Each of the FR/AR/BR
/// operand *kinds* is exercised in every position it appears in, because a
/// swapped `r`/`s`/`t` field is the mistake this catches.
#[test]
fn fp_registers() {
    use FpRrOp::*;
    use FpRrrOp::*;
    for op in [AddS, SubS, MulS, MaddS, MsubS, MaddnS, DivnS] {
        for &x in &REGS {
            for &y in &REGS {
                rt(Inst::FpRrr(op, f(x), f(y), f(15 - x)), 3);
            }
        }
    }
    for op in [
        MovS, AbsS, NegS, Div0S, Recip0S, Sqrt0S, Rsqrt0S, Nexp01S, MkdadjS, AddexpS, AddexpmS,
    ] {
        for &x in &REGS {
            for &y in &REGS {
                rt(Inst::FpRr(op, f(x), f(y)), 3);
            }
        }
    }
    for &x in &REGS {
        for &y in &REGS {
            rt(Inst::Rfr(r(x), f(y)), 3);
            rt(Inst::Wfr(f(x), r(y)), 3);
        }
        // const.s takes a 0..=15 selector in the `s` field.
        for imm in 0..=15u8 {
            rt(Inst::ConstS(f(x), imm), 3);
        }
    }
}

#[test]
fn fp_compares_and_moves() {
    for op in [
        FpCmpOp::UnS,
        FpCmpOp::OeqS,
        FpCmpOp::UeqS,
        FpCmpOp::OltS,
        FpCmpOp::UltS,
        FpCmpOp::OleS,
        FpCmpOp::UleS,
    ] {
        for &x in &REGS {
            for &y in &REGS {
                rt(Inst::FpCmp(op, b(x), f(y), f(15 - x)), 3);
            }
        }
    }
    for op in [
        FpMovArOp::MoveqzS,
        FpMovArOp::MovnezS,
        FpMovArOp::MovltzS,
        FpMovArOp::MovgezS,
    ] {
        for &x in &REGS {
            for &y in &REGS {
                rt(Inst::FpMovAr(op, f(x), f(y), r(15 - x)), 3);
            }
        }
    }
    for op in [FpMovBrOp::MovfS, FpMovBrOp::MovtS] {
        for &x in &REGS {
            for &y in &REGS {
                rt(Inst::FpMovBr(op, f(x), f(y), b(15 - x)), 3);
            }
        }
    }
}

/// Conversions carry a 0..=15 scale immediate in the `t` field — the whole
/// range, both directions.
#[test]
fn fp_conversions() {
    for op in [
        FpToIntOp::RoundS,
        FpToIntOp::TruncS,
        FpToIntOp::FloorS,
        FpToIntOp::CeilS,
        FpToIntOp::UtruncS,
    ] {
        for &x in &REGS {
            for imm in 0..=15u8 {
                rt(Inst::FpToInt(op, r(x), f(15 - x), imm), 3);
            }
        }
    }
    for op in [IntToFpOp::FloatS, IntToFpOp::UfloatS] {
        for &x in &REGS {
            for imm in 0..=15u8 {
                rt(Inst::IntToFp(op, f(x), r(15 - x), imm), 3);
            }
        }
    }
}

/// FP load/store offsets: 0..=1020 in steps of 4, the full `imm8` field.
#[test]
fn fp_loads_stores() {
    for op in [FpLsiOp::Lsi, FpLsiOp::Ssi, FpLsiOp::Lsip, FpLsiOp::Ssip] {
        for off in (0..=1020).step_by(4) {
            rt(Inst::FpLsi(op, f(2), r(3), off), 3);
        }
    }
    for op in [FpLsxOp::Lsx, FpLsxOp::Lsxp, FpLsxOp::Ssx, FpLsxOp::Ssxp] {
        for &x in &REGS {
            for &y in &REGS {
                rt(Inst::FpLsx(op, f(x), r(y), r(15 - x)), 3);
            }
        }
    }
}

#[test]
fn boolean_moves_and_branches() {
    for set in [true, false] {
        for &x in &REGS {
            for &y in &REGS {
                rt(Inst::MovBool(set, r(x), r(y), b(15 - x)), 3);
            }
        }
        // bt/bf carry a signed 8-bit PC-relative offset, like the other RRI8
        // branches.
        for off in [-128i32, -22, -1, 0, 1, 5, 127] {
            for &x in &REGS {
                rt(Inst::BranchBool(set, b(x), off), 3);
            }
        }
    }
}

#[test]
fn special_and_user_registers() {
    for op in [SrOp::Rsr, SrOp::Wsr, SrOp::Xsr] {
        for sreg in [SpecialReg::Br, SpecialReg::Cpenable] {
            for &x in &REGS {
                rt(Inst::Sr(op, sreg, r(x)), 3);
            }
        }
    }
    for op in [UrOp::Rur, UrOp::Wur] {
        for ureg in [UserReg::Fcr, UserReg::Fsr] {
            for &x in &REGS {
                rt(Inst::Ur(op, ureg, r(x)), 3);
            }
        }
    }
}

/// The density length rule: op0 in 0x8..=0xD is 16-bit, everything else 24-bit.
#[test]
fn length_rule() {
    for b in 0u8..=0xff {
        let expect = match b & 0x0f {
            0x8..=0xd => 2,
            _ => 3,
        };
        assert_eq!(base_inst_len(b), expect, "byte {b:#04x}");
    }
}

/// Unsupported / out-of-scope opcodes must decode as `Unsupported`, carrying the
/// correct length so a stream walk stays aligned.
#[test]
fn unsupported_reports_length() {
    // `andb b0, b1, b2` (op0=0, op1=2, op2=0) — the boolean *logic* ops are
    // deliberately outside the subset (M6 needs only the compare readback
    // paths); 3 bytes. Assembler-derived bytes.
    let e = decode(&[0x20, 0x01, 0x02]).unwrap_err();
    assert!(matches!(e, DecodeError::Unsupported { len: 3, .. }));
}
