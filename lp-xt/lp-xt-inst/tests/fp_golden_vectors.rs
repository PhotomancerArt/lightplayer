//! Golden vectors for the FP / Boolean / special-register subset (M6 P1).
//!
//! Every byte sequence here was produced by assembling a one-instruction `.S`
//! file with `xtensa-esp32s3-elf-as` (esp-14.2.0_20240906) and reading the raw
//! little-endian bytes out of `objcopy -O binary`. **None of it was recalled
//! from memory** — the spike lesson was that 2 of 3 recalls were wrong. The
//! derivation procedure is written down in `lp-xt/fixtures/fp/README.md` and is
//! re-runnable.
//!
//! Each `dec` call asserts three things at once: the bytes decode to the
//! expected `Inst`, the length is right, and `encode` reproduces the exact input
//! bytes.

use lp_xt_inst::*;

fn a(n: u8) -> Reg {
    Reg::new(n)
}
fn f(n: u8) -> FReg {
    FReg::new(n)
}
fn b(n: u8) -> BReg {
    BReg::new(n)
}

/// Decode one instruction, asserting it round-trips to the exact input bytes.
#[track_caller]
fn dec(bytes: &[u8]) -> Inst {
    let (inst, len) = decode(bytes).expect("decode");
    assert_eq!(len, bytes.len(), "length for {inst:?}");
    assert_eq!(encode(&inst), bytes, "round-trip for {inst:?}");
    inst
}

/// Decode one instruction and assert its objdump-style rendering, at pc 0.
#[track_caller]
fn dis(bytes: &[u8], expect: &str) {
    assert_eq!(format_instruction(bytes, 0), expect);
}

#[test]
fn fp0_three_operand() {
    // add.s   f0, f1, f2
    assert_eq!(
        dec(&[0x20, 0x01, 0x0a]),
        Inst::FpRrr(FpRrrOp::AddS, f(0), f(1), f(2))
    );
    // sub.s   f0, f1, f2
    assert_eq!(
        dec(&[0x20, 0x01, 0x1a]),
        Inst::FpRrr(FpRrrOp::SubS, f(0), f(1), f(2))
    );
    // mul.s   f0, f1, f2
    assert_eq!(
        dec(&[0x20, 0x01, 0x2a]),
        Inst::FpRrr(FpRrrOp::MulS, f(0), f(1), f(2))
    );
    // madd.s  f0, f1, f2
    assert_eq!(
        dec(&[0x20, 0x01, 0x4a]),
        Inst::FpRrr(FpRrrOp::MaddS, f(0), f(1), f(2))
    );
    // msub.s  f0, f1, f2
    assert_eq!(
        dec(&[0x20, 0x01, 0x5a]),
        Inst::FpRrr(FpRrrOp::MsubS, f(0), f(1), f(2))
    );
    // maddn.s f0, f1, f2
    assert_eq!(
        dec(&[0x20, 0x01, 0x6a]),
        Inst::FpRrr(FpRrrOp::MaddnS, f(0), f(1), f(2))
    );
    // divn.s  f0, f1, f2
    assert_eq!(
        dec(&[0x20, 0x01, 0x7a]),
        Inst::FpRrr(FpRrrOp::DivnS, f(0), f(1), f(2))
    );

    // The high-register corner, so the r/s/t field split is pinned in both
    // directions: add.s f15, f14, f13 / sub.s f7, f8, f9 / mul.s f1, f2, f3.
    assert_eq!(
        dec(&[0xd0, 0xfe, 0x0a]),
        Inst::FpRrr(FpRrrOp::AddS, f(15), f(14), f(13))
    );
    assert_eq!(
        dec(&[0x90, 0x78, 0x1a]),
        Inst::FpRrr(FpRrrOp::SubS, f(7), f(8), f(9))
    );
    assert_eq!(
        dec(&[0x30, 0x12, 0x2a]),
        Inst::FpRrr(FpRrrOp::MulS, f(1), f(2), f(3))
    );
    // madd.s f10, f11, f12 / msub.s f4, f5, f6
    assert_eq!(
        dec(&[0xc0, 0xab, 0x4a]),
        Inst::FpRrr(FpRrrOp::MaddS, f(10), f(11), f(12))
    );
    assert_eq!(
        dec(&[0x60, 0x45, 0x5a]),
        Inst::FpRrr(FpRrrOp::MsubS, f(4), f(5), f(6))
    );

    dis(&[0xd0, 0xfe, 0x0a], "add.s\tf15, f14, f13");
}

#[test]
fn fp1_unary_group() {
    // mov.s    f0, f1
    assert_eq!(
        dec(&[0x00, 0x01, 0xfa]),
        Inst::FpRr(FpRrOp::MovS, f(0), f(1))
    );
    // abs.s    f0, f1
    assert_eq!(
        dec(&[0x10, 0x01, 0xfa]),
        Inst::FpRr(FpRrOp::AbsS, f(0), f(1))
    );
    // neg.s    f0, f1
    assert_eq!(
        dec(&[0x60, 0x01, 0xfa]),
        Inst::FpRr(FpRrOp::NegS, f(0), f(1))
    );
    // div0.s   f0, f1
    assert_eq!(
        dec(&[0x70, 0x01, 0xfa]),
        Inst::FpRr(FpRrOp::Div0S, f(0), f(1))
    );
    // recip0.s f0, f1
    assert_eq!(
        dec(&[0x80, 0x01, 0xfa]),
        Inst::FpRr(FpRrOp::Recip0S, f(0), f(1))
    );
    // sqrt0.s  f0, f1
    assert_eq!(
        dec(&[0x90, 0x01, 0xfa]),
        Inst::FpRr(FpRrOp::Sqrt0S, f(0), f(1))
    );
    // rsqrt0.s f0, f1
    assert_eq!(
        dec(&[0xa0, 0x01, 0xfa]),
        Inst::FpRr(FpRrOp::Rsqrt0S, f(0), f(1))
    );
    // nexp01.s f0, f1
    assert_eq!(
        dec(&[0xb0, 0x01, 0xfa]),
        Inst::FpRr(FpRrOp::Nexp01S, f(0), f(1))
    );
    // mkdadj.s f0, f1
    assert_eq!(
        dec(&[0xd0, 0x01, 0xfa]),
        Inst::FpRr(FpRrOp::MkdadjS, f(0), f(1))
    );
    // addexp.s  f0, f1
    assert_eq!(
        dec(&[0xe0, 0x01, 0xfa]),
        Inst::FpRr(FpRrOp::AddexpS, f(0), f(1))
    );
    // addexpm.s f0, f1
    assert_eq!(
        dec(&[0xf0, 0x01, 0xfa]),
        Inst::FpRr(FpRrOp::AddexpmS, f(0), f(1))
    );

    // const.s f0, 3 — the constant index rides in the `s` field, not a register.
    assert_eq!(dec(&[0x30, 0x03, 0xfa]), Inst::ConstS(f(0), 3));
    // const.s f7, 0 / const.s f7, 15
    assert_eq!(dec(&[0x30, 0x70, 0xfa]), Inst::ConstS(f(7), 0));
    assert_eq!(dec(&[0x30, 0x7f, 0xfa]), Inst::ConstS(f(7), 15));

    // rfr a0, f1 / wfr f0, a1 — note the operand kinds swap sides.
    assert_eq!(dec(&[0x40, 0x01, 0xfa]), Inst::Rfr(a(0), f(1)));
    assert_eq!(dec(&[0x50, 0x01, 0xfa]), Inst::Wfr(f(0), a(1)));
    // rfr a15, f14 / wfr f15, a14
    assert_eq!(dec(&[0x40, 0xfe, 0xfa]), Inst::Rfr(a(15), f(14)));
    assert_eq!(dec(&[0x50, 0xfe, 0xfa]), Inst::Wfr(f(15), a(14)));

    dis(&[0x40, 0xfe, 0xfa], "rfr\ta15, f14");
    dis(&[0x50, 0xfe, 0xfa], "wfr\tf15, a14");
    dis(&[0x30, 0x7f, 0xfa], "const.s\tf7, 15");
}

/// The FP1 selector slots `t = 2` and `t = 0xC` have no mnemonic in
/// `xtensa-esp32s3-elf-objdump`, so they must stay unsupported rather than being
/// guessed at.
#[test]
fn fp1_unassigned_slots_stay_unsupported() {
    for t in [0x2u8, 0xc] {
        let w = 0x00fa_0000u32 | ((t as u32) << 4); // op1=0xA, op2=0xF, s=r=0
        let bytes = [w as u8, (w >> 8) as u8, (w >> 16) as u8];
        assert!(
            matches!(
                decode(&bytes).unwrap_err(),
                DecodeError::Unsupported { len: 3, .. }
            ),
            "FP1 selector t={t:#x} must not decode"
        );
    }
}

#[test]
fn fp_compares_write_boolean_registers() {
    // un.s  b0, f1, f2
    assert_eq!(
        dec(&[0x20, 0x01, 0x1b]),
        Inst::FpCmp(FpCmpOp::UnS, b(0), f(1), f(2))
    );
    // oeq.s b0, f1, f2
    assert_eq!(
        dec(&[0x20, 0x01, 0x2b]),
        Inst::FpCmp(FpCmpOp::OeqS, b(0), f(1), f(2))
    );
    // ueq.s b0, f1, f2
    assert_eq!(
        dec(&[0x20, 0x01, 0x3b]),
        Inst::FpCmp(FpCmpOp::UeqS, b(0), f(1), f(2))
    );
    // olt.s b0, f1, f2
    assert_eq!(
        dec(&[0x20, 0x01, 0x4b]),
        Inst::FpCmp(FpCmpOp::OltS, b(0), f(1), f(2))
    );
    // ult.s b0, f1, f2
    assert_eq!(
        dec(&[0x20, 0x01, 0x5b]),
        Inst::FpCmp(FpCmpOp::UltS, b(0), f(1), f(2))
    );
    // ole.s b0, f1, f2
    assert_eq!(
        dec(&[0x20, 0x01, 0x6b]),
        Inst::FpCmp(FpCmpOp::OleS, b(0), f(1), f(2))
    );
    // ule.s b0, f1, f2
    assert_eq!(
        dec(&[0x20, 0x01, 0x7b]),
        Inst::FpCmp(FpCmpOp::UleS, b(0), f(1), f(2))
    );
    // oeq.s b15, f14, f13
    assert_eq!(
        dec(&[0xd0, 0xfe, 0x2b]),
        Inst::FpCmp(FpCmpOp::OeqS, b(15), f(14), f(13))
    );

    dis(&[0xd0, 0xfe, 0x2b], "oeq.s\tb15, f14, f13");
}

#[test]
fn fp_conditional_moves() {
    // moveqz.s f0, f1, a2
    assert_eq!(
        dec(&[0x20, 0x01, 0x8b]),
        Inst::FpMovAr(FpMovArOp::MoveqzS, f(0), f(1), a(2))
    );
    // movnez.s f0, f1, a2
    assert_eq!(
        dec(&[0x20, 0x01, 0x9b]),
        Inst::FpMovAr(FpMovArOp::MovnezS, f(0), f(1), a(2))
    );
    // movltz.s f0, f1, a2
    assert_eq!(
        dec(&[0x20, 0x01, 0xab]),
        Inst::FpMovAr(FpMovArOp::MovltzS, f(0), f(1), a(2))
    );
    // movgez.s f0, f1, a2
    assert_eq!(
        dec(&[0x20, 0x01, 0xbb]),
        Inst::FpMovAr(FpMovArOp::MovgezS, f(0), f(1), a(2))
    );
    // movf.s f0, f1, b2 / movt.s f0, f1, b2 — the third operand is a BR here.
    assert_eq!(
        dec(&[0x20, 0x01, 0xcb]),
        Inst::FpMovBr(FpMovBrOp::MovfS, f(0), f(1), b(2))
    );
    assert_eq!(
        dec(&[0x20, 0x01, 0xdb]),
        Inst::FpMovBr(FpMovBrOp::MovtS, f(0), f(1), b(2))
    );
    // movt.s f15, f14, b13
    assert_eq!(
        dec(&[0xd0, 0xfe, 0xdb]),
        Inst::FpMovBr(FpMovBrOp::MovtS, f(15), f(14), b(13))
    );

    dis(&[0xd0, 0xfe, 0xdb], "movt.s\tf15, f14, b13");
    dis(&[0x20, 0x01, 0x8b], "moveqz.s\tf0, f1, a2");
}

#[test]
fn conversions_carry_a_scale_immediate() {
    // round.s  a0, f1, 3
    assert_eq!(
        dec(&[0x30, 0x01, 0x8a]),
        Inst::FpToInt(FpToIntOp::RoundS, a(0), f(1), 3)
    );
    // trunc.s  a0, f1, 3
    assert_eq!(
        dec(&[0x30, 0x01, 0x9a]),
        Inst::FpToInt(FpToIntOp::TruncS, a(0), f(1), 3)
    );
    // floor.s  a0, f1, 3
    assert_eq!(
        dec(&[0x30, 0x01, 0xaa]),
        Inst::FpToInt(FpToIntOp::FloorS, a(0), f(1), 3)
    );
    // ceil.s   a0, f1, 3
    assert_eq!(
        dec(&[0x30, 0x01, 0xba]),
        Inst::FpToInt(FpToIntOp::CeilS, a(0), f(1), 3)
    );
    // utrunc.s a0, f1, 3
    assert_eq!(
        dec(&[0x30, 0x01, 0xea]),
        Inst::FpToInt(FpToIntOp::UtruncS, a(0), f(1), 3)
    );
    // float.s  f0, a1, 3 / ufloat.s f0, a1, 3
    assert_eq!(
        dec(&[0x30, 0x01, 0xca]),
        Inst::IntToFp(IntToFpOp::FloatS, f(0), a(1), 3)
    );
    assert_eq!(
        dec(&[0x30, 0x01, 0xda]),
        Inst::IntToFp(IntToFpOp::UfloatS, f(0), a(1), 3)
    );

    // The immediate's field boundaries: round.s a15, f14, 0 and ..., 15.
    assert_eq!(
        dec(&[0x00, 0xfe, 0x8a]),
        Inst::FpToInt(FpToIntOp::RoundS, a(15), f(14), 0)
    );
    assert_eq!(
        dec(&[0xf0, 0xfe, 0x8a]),
        Inst::FpToInt(FpToIntOp::RoundS, a(15), f(14), 15)
    );
    // float.s f15, a14, 15
    assert_eq!(
        dec(&[0xf0, 0xfe, 0xca]),
        Inst::IntToFp(IntToFpOp::FloatS, f(15), a(14), 15)
    );

    dis(&[0xf0, 0xfe, 0x8a], "round.s\ta15, f14, 15");
    dis(&[0xf0, 0xfe, 0xca], "float.s\tf15, a14, 15");
}

#[test]
fn fp_loads_and_stores() {
    // lsi  f0, a1, 8 — the imm8 field holds offset/4.
    assert_eq!(
        dec(&[0x03, 0x01, 0x02]),
        Inst::FpLsi(FpLsiOp::Lsi, f(0), a(1), 8)
    );
    // ssi  f0, a1, 8
    assert_eq!(
        dec(&[0x03, 0x41, 0x02]),
        Inst::FpLsi(FpLsiOp::Ssi, f(0), a(1), 8)
    );
    // lsip f0, a1, 8
    assert_eq!(
        dec(&[0x03, 0x81, 0x02]),
        Inst::FpLsi(FpLsiOp::Lsip, f(0), a(1), 8)
    );
    // ssip f0, a1, 8
    assert_eq!(
        dec(&[0x03, 0xc1, 0x02]),
        Inst::FpLsi(FpLsiOp::Ssip, f(0), a(1), 8)
    );
    // Offset field boundaries: lsi f15, a14, 0 and lsi/ssi f15, a14, 1020.
    assert_eq!(
        dec(&[0xf3, 0x0e, 0x00]),
        Inst::FpLsi(FpLsiOp::Lsi, f(15), a(14), 0)
    );
    assert_eq!(
        dec(&[0xf3, 0x0e, 0xff]),
        Inst::FpLsi(FpLsiOp::Lsi, f(15), a(14), 1020)
    );
    assert_eq!(
        dec(&[0xf3, 0x4e, 0xff]),
        Inst::FpLsi(FpLsiOp::Ssi, f(15), a(14), 1020)
    );

    // lsx / lsxp / ssx / ssxp f0, a1, a2
    assert_eq!(
        dec(&[0x20, 0x01, 0x08]),
        Inst::FpLsx(FpLsxOp::Lsx, f(0), a(1), a(2))
    );
    assert_eq!(
        dec(&[0x20, 0x01, 0x18]),
        Inst::FpLsx(FpLsxOp::Lsxp, f(0), a(1), a(2))
    );
    assert_eq!(
        dec(&[0x20, 0x01, 0x48]),
        Inst::FpLsx(FpLsxOp::Ssx, f(0), a(1), a(2))
    );
    assert_eq!(
        dec(&[0x20, 0x01, 0x58]),
        Inst::FpLsx(FpLsxOp::Ssxp, f(0), a(1), a(2))
    );
    // lsx f15, a14, a13
    assert_eq!(
        dec(&[0xd0, 0xfe, 0x08]),
        Inst::FpLsx(FpLsxOp::Lsx, f(15), a(14), a(13))
    );

    dis(&[0xf3, 0x0e, 0xff], "lsi\tf15, a14, 1020");
    dis(&[0xd0, 0xfe, 0x08], "lsx\tf15, a14, a13");
}

#[test]
fn boolean_register_reads() {
    // movf a0, a1, b2 / movt a0, a1, b2 — pull a compare result into an AR.
    assert_eq!(
        dec(&[0x20, 0x01, 0xc3]),
        Inst::MovBool(false, a(0), a(1), b(2))
    );
    assert_eq!(
        dec(&[0x20, 0x01, 0xd3]),
        Inst::MovBool(true, a(0), a(1), b(2))
    );
    // movt a15, a14, b13
    assert_eq!(
        dec(&[0xd0, 0xfe, 0xd3]),
        Inst::MovBool(true, a(15), a(14), b(13))
    );

    // bt b3, . / bf b3, . — the assembler emits offset -4 for a self-target,
    // because the branch formula is pc + 4 + offset.
    assert_eq!(dec(&[0x76, 0x13, 0xfc]), Inst::BranchBool(true, b(3), -4));
    assert_eq!(dec(&[0x76, 0x03, 0xfc]), Inst::BranchBool(false, b(3), -4));
    // bt b15, .
    assert_eq!(dec(&[0x76, 0x1f, 0xfc]), Inst::BranchBool(true, b(15), -4));

    dis(&[0xd0, 0xfe, 0xd3], "movt\ta15, a14, b13");
    dis(&[0x76, 0x13, 0xfc], "bt\tb3, 0x0");
}

/// The loop family shares the BI1 slot with `bt`/`bf` and must stay unsupported.
#[test]
fn bi1_loop_family_stays_unsupported() {
    // op0=6, n=3, m=1 (t nibble = 7), r = 8/9/0xA -> loop / loopnez / loopgtz.
    for r in [0x8u32, 0x9, 0xa] {
        let w = 0x6 | (7 << 4) | (r << 12);
        let bytes = [w as u8, (w >> 8) as u8, (w >> 16) as u8];
        assert!(
            matches!(
                decode(&bytes).unwrap_err(),
                DecodeError::Unsupported { len: 3, .. }
            ),
            "BI1 r={r:#x} (loop family) must not decode"
        );
    }
}

#[test]
fn special_and_user_registers() {
    // rsr.br a0 / wsr.br a0 / xsr.br a0 — BR is SR 4.
    assert_eq!(
        dec(&[0x00, 0x04, 0x03]),
        Inst::Sr(SrOp::Rsr, SpecialReg::Br, a(0))
    );
    assert_eq!(
        dec(&[0x00, 0x04, 0x13]),
        Inst::Sr(SrOp::Wsr, SpecialReg::Br, a(0))
    );
    assert_eq!(
        dec(&[0x00, 0x04, 0x61]),
        Inst::Sr(SrOp::Xsr, SpecialReg::Br, a(0))
    );
    // rsr/wsr/xsr.cpenable a0 — CPENABLE is SR 224.
    assert_eq!(
        dec(&[0x00, 0xe0, 0x03]),
        Inst::Sr(SrOp::Rsr, SpecialReg::Cpenable, a(0))
    );
    assert_eq!(
        dec(&[0x00, 0xe0, 0x13]),
        Inst::Sr(SrOp::Wsr, SpecialReg::Cpenable, a(0))
    );
    assert_eq!(
        dec(&[0x00, 0xe0, 0x61]),
        Inst::Sr(SrOp::Xsr, SpecialReg::Cpenable, a(0))
    );
    // ... and at a15, so the `t` field is pinned.
    assert_eq!(
        dec(&[0xf0, 0xe0, 0x13]),
        Inst::Sr(SrOp::Wsr, SpecialReg::Cpenable, a(15))
    );

    // rur.fcr a0 / wur.fcr a0 — FCR is UR 232; note RUR and WUR pack the
    // register number into *different* field pairs.
    assert_eq!(
        dec(&[0x80, 0x0e, 0xe3]),
        Inst::Ur(UrOp::Rur, UserReg::Fcr, a(0))
    );
    assert_eq!(
        dec(&[0x00, 0xe8, 0xf3]),
        Inst::Ur(UrOp::Wur, UserReg::Fcr, a(0))
    );
    // rur.fsr a0 / wur.fsr a0 — FSR is UR 233.
    assert_eq!(
        dec(&[0x90, 0x0e, 0xe3]),
        Inst::Ur(UrOp::Rur, UserReg::Fsr, a(0))
    );
    assert_eq!(
        dec(&[0x00, 0xe9, 0xf3]),
        Inst::Ur(UrOp::Wur, UserReg::Fsr, a(0))
    );
    // rur.fcr a15 / wur.fcr a15
    assert_eq!(
        dec(&[0x80, 0xfe, 0xe3]),
        Inst::Ur(UrOp::Rur, UserReg::Fcr, a(15))
    );
    assert_eq!(
        dec(&[0xf0, 0xe8, 0xf3]),
        Inst::Ur(UrOp::Wur, UserReg::Fcr, a(15))
    );

    dis(&[0xf0, 0xe0, 0x13], "wsr.cpenable\ta15");
    dis(&[0x80, 0xfe, 0xe3], "rur.fcr\ta15");
    dis(&[0xf0, 0xe8, 0xf3], "wur.fcr\ta15");
}

/// Special registers outside the modeled set stay unsupported — this crate does
/// not claim a general SR model. `rsr.sar a3` / `wsr.sar a3` are the witnesses
/// (SAR is SR 3, and the emulator models `SAR` through `ssai`/`ssl`, not here).
#[test]
fn unmodeled_special_registers_stay_unsupported() {
    for bytes in [[0x30u8, 0x03, 0x03], [0x30, 0x03, 0x13]] {
        assert!(
            matches!(
                decode(&bytes).unwrap_err(),
                DecodeError::Unsupported { len: 3, .. }
            ),
            "SR 3 (sar) must not decode: {bytes:02x?}"
        );
    }
}

/// The whole subset as one stream, walked with the variable-length decoder — the
/// property that matters for disassembly and for objdiff.
#[test]
fn subset_walks_as_a_stream() {
    // Assembled as one .S file (`here:` at offset 0), not stitched together from
    // the single-instruction goldens above — so the `bt` offset is a real one.
    const STREAM: [u8; 24] = [
        0x00, 0xe0, 0x03, // +0x00  rsr.cpenable a0
        0x00, 0xe0, 0x13, // +0x03  wsr.cpenable a0
        0x00, 0x20, 0x00, // +0x06  isync
        0x03, 0x01, 0x02, // +0x09  lsi   f0, a1, 8
        0x20, 0x01, 0x0a, // +0x0c  add.s f0, f1, f2
        0x20, 0x01, 0x2b, // +0x0f  oeq.s b0, f1, f2
        0x76, 0x13, 0xea, // +0x12  bt    b3, here
        0x40, 0x01, 0xfa, // +0x15  rfr   a0, f1
    ];
    let mut bytes: &[u8] = &STREAM;
    let mut n = 0;
    while !bytes.is_empty() {
        let (inst, len) = decode(bytes).expect("decode in walk");
        assert_eq!(encode(&inst), &bytes[..len], "round-trip for {inst:?}");
        bytes = &bytes[len..];
        n += 1;
    }
    assert_eq!(n, 8, "the stream holds eight instructions");

    // The `bt` at +0x12 branches back to `here` at 0: pc + 4 + (-22) == 0.
    assert_eq!(
        decode(&STREAM[0x12..0x15]).unwrap().0,
        Inst::BranchBool(true, b(3), -22)
    );
    assert_eq!(format_instruction(&STREAM[0x12..0x15], 0x12), "bt\tb3, 0x0");
}
