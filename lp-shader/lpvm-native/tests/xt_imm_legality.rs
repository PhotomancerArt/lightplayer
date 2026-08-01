//! Boundary / property tests for the per-opcode immediate-legality table
//! (`lpvm_native::isa::xt::imm`).
//!
//! Three layers:
//! 1. generic boundary properties driven off the table itself (min/max legal,
//!    one step beyond illegal, scale respected);
//! 2. pinned literal ranges mirroring the gas probes (`xtensa-esp32s3-elf-as`
//!    accept/reject at each boundary) so the table cannot drift silently;
//! 3. cross-checks against `lp-xt-inst`: boundary immediates encode/decode
//!    round-trip, and just-out-of-range immediates demonstrably *corrupt*
//!    (silent truncation) — which is exactly why the table must gate.

use lp_xt_inst::{
    AluRrr, B4CONST, B4CONSTU, BrRr, BrZ, CallOp, Inst, LoadOp, Reg, StoreOp, b4const_index,
    b4constu_index, decode, disasm::l32r_target, encode,
};
use lpvm_native::isa::xt::imm::{
    Fallback, ImmOp, ImmRule, PcRel, extui_legal, fallback, is_legal, spec,
};

fn r(n: u8) -> Reg {
    Reg::new(n)
}

/// Encode then decode, asserting the byte length matched.
fn roundtrip(inst: Inst) -> Inst {
    let bytes = encode(&inst);
    let (decoded, len) = decode(&bytes).expect("boundary encoding must decode");
    assert_eq!(len, bytes.len(), "decode length mismatch for {inst:?}");
    decoded
}

/// Table self-consistency: every Range is well-formed and scale-aligned.
#[test]
fn table_is_self_consistent() {
    for &op in ImmOp::ALL {
        match spec(op).rule {
            ImmRule::Range { min, max, step } => {
                assert!(step >= 1, "{op:?}: step must be positive");
                assert!(min <= max, "{op:?}: empty range");
                assert_eq!(min % step, 0, "{op:?}: min not a multiple of step");
                assert_eq!(max % step, 0, "{op:?}: max not a multiple of step");
            }
            ImmRule::Set(set) => assert!(!set.is_empty(), "{op:?}: empty set"),
            ImmRule::NoImmForm => {
                // The whole point of these entries: an explicit "does not
                // exist", with a mandatory materialization fallback.
                assert_eq!(fallback(op), Fallback::ConstThenReg, "{op:?}");
            }
        }
    }
}

/// Generic boundary property: min and max are legal; one step beyond either
/// end is not; off-scale values inside the range are not.
#[test]
fn range_boundaries_accept_and_reject() {
    for &op in ImmOp::ALL {
        if let ImmRule::Range { min, max, step } = spec(op).rule {
            // addi.n legally excludes 0 mid-range; boundaries still hold.
            assert!(is_legal(op, min), "{op:?}: min {min} must be legal");
            assert!(is_legal(op, max), "{op:?}: max {max} must be legal");
            assert!(
                !is_legal(op, min - step),
                "{op:?}: below min must be illegal"
            );
            assert!(
                !is_legal(op, max + step),
                "{op:?}: above max must be illegal"
            );
            if step > 1 {
                assert!(!is_legal(op, min + 1), "{op:?}: off-scale must be illegal");
                assert!(!is_legal(op, max - 1), "{op:?}: off-scale must be illegal");
            }
        }
    }
}

/// Pinned literal ranges — mirrors the gas boundary probes exactly. If the
/// table drifts, this fails with the opcode named.
#[test]
fn pinned_ranges_match_assembler_probes() {
    // (op, legal values, illegal values) — each legal/illegal pair was
    // accept/reject-probed against xtensa-esp32s3-elf-as.
    let cases: &[(ImmOp, &[i32], &[i32])] = &[
        (ImmOp::Addi, &[-128, 0, 127], &[-129, 128]),
        (ImmOp::AddiN, &[-1, 1, 15], &[-2, 0, 16]),
        (
            ImmOp::Addmi,
            &[-32768, -256, 0, 256, 32512],
            &[-33024, -255, 255, 32768],
        ),
        (ImmOp::Movi, &[-2048, 0, 2047], &[-2049, 2048]),
        (ImmOp::MoviN, &[-32, 0, 95], &[-33, 96]),
        (ImmOp::L8ui, &[0, 255], &[-1, 256]),
        (ImmOp::S8i, &[0, 255], &[-1, 256]),
        (ImmOp::L16ui, &[0, 2, 510], &[-2, 1, 511, 512]),
        (ImmOp::L16si, &[0, 510], &[1, 512]),
        (ImmOp::S16i, &[0, 510], &[1, 512]),
        (ImmOp::L32i, &[0, 4, 1020], &[-4, 1, 2, 1021, 1024]),
        (ImmOp::S32i, &[0, 1020], &[2, 1024]),
        (ImmOp::L32iN, &[0, 60], &[-4, 2, 61, 64]),
        (ImmOp::S32iN, &[0, 60], &[2, 64]),
        (
            ImmOp::L32rDisp,
            &[-262144, -4],
            &[-262148, -262143, -3, 0, 4],
        ),
        (ImmOp::EntryFrame, &[0, 8, 32760], &[-8, 7, 32761, 32768]),
        (ImmOp::SlliSa, &[1, 31], &[0, 32]),
        (ImmOp::SrliSa, &[0, 15], &[-1, 16]),
        (ImmOp::SraiSa, &[0, 31], &[-1, 32]),
        (ImmOp::SsaiSa, &[0, 31], &[-1, 32]),
        (ImmOp::ExtuiShift, &[0, 31], &[-1, 32]),
        (ImmOp::ExtuiWidth, &[1, 16], &[0, 17]),
        (ImmOp::SextBit, &[7, 22], &[6, 23]),
        (ImmOp::BbiBit, &[0, 31], &[-1, 32]),
        (ImmOp::Branch8Disp, &[-128, 0, 127], &[-129, 128]),
        (ImmOp::Branch12Disp, &[-2048, 0, 2047], &[-2049, 2048]),
        (ImmOp::Branch6NDisp, &[0, 63], &[-1, -4, 64]),
        (ImmOp::JDisp, &[-131072, 0, 131071], &[-131073, 131072]),
        (
            ImmOp::CallDisp,
            &[-524288, 0, 4, 524284],
            &[-524292, -2, 2, 524288],
        ),
        (
            ImmOp::FpLsiOffset,
            &[0, 4, 1020],
            &[-4, 1, 2, 1021, 1024, 4096],
        ),
    ];
    for &(op, legal, illegal) in cases {
        for &v in legal {
            assert!(is_legal(op, v), "{op:?}: {v} must be legal");
        }
        for &v in illegal {
            assert!(!is_legal(op, v), "{op:?}: {v} must be illegal");
        }
    }
}

/// THE key Xtensa fact: no andi/ori/xori. Every immediate is illegal and the
/// only lowering is materialize + register op.
#[test]
fn bitwise_immediates_do_not_exist() {
    for op in [ImmOp::AndImm, ImmOp::OrImm, ImmOp::XorImm] {
        assert!(matches!(spec(op).rule, ImmRule::NoImmForm));
        for v in [-2048, -1, 0, 1, 7, 255, 2047, i32::MAX] {
            assert!(!is_legal(op, v), "{op:?}: no immediate form may accept {v}");
        }
        assert_eq!(fallback(op), Fallback::ConstThenReg);
    }
}

/// b4const / b4constu legality is exactly what the encoder can represent.
#[test]
fn b4const_sets_match_encoder_representability() {
    for v in -300..=300 {
        assert_eq!(
            is_legal(ImmOp::BranchB4Const, v),
            b4const_index(v).is_some(),
            "b4const disagreement at {v}"
        );
        assert_eq!(
            is_legal(ImmOp::BranchB4Constu, v),
            b4constu_index(v).is_some(),
            "b4constu disagreement at {v}"
        );
    }
    // The unsigned set's two large members (and their neighbors).
    for (v, legal) in [(32768, true), (65536, true), (32767, false), (65535, false)] {
        assert_eq!(is_legal(ImmOp::BranchB4Constu, v), legal, "b4constu {v}");
    }
    // gas probes: beqi 0 and bltui 1 rejected; beqi 256 / bltui 32768 accepted.
    assert!(!is_legal(ImmOp::BranchB4Const, 0));
    assert!(!is_legal(ImmOp::BranchB4Constu, 1));
    assert!(is_legal(ImmOp::BranchB4Const, 256));
    // The sets referenced by the table are the encoder's own.
    assert_eq!(spec(ImmOp::BranchB4Const).rule, ImmRule::Set(&B4CONST));
    assert_eq!(spec(ImmOp::BranchB4Constu).rule, ImmRule::Set(&B4CONSTU));
}

/// The joint extui constraint (shift + width <= 32), as enforced by gas.
#[test]
fn extui_joint_constraint() {
    assert!(extui_legal(0, 1));
    assert!(extui_legal(0, 16));
    assert!(extui_legal(16, 16)); // gas: accept
    assert!(extui_legal(31, 1)); // gas: accept
    assert!(extui_legal(24, 8)); // gas: accept
    assert!(!extui_legal(17, 16)); // gas: "operands sum to greater than 32"
    assert!(!extui_legal(25, 8)); // gas: reject
    assert!(!extui_legal(32, 1)); // shift out of range
    assert!(!extui_legal(0, 17)); // width out of range
    assert!(!extui_legal(0, 0)); // width out of range
}

/// Boundary immediates survive an lp-xt-inst encode/decode round-trip for
/// every opcode the shared encoder models.
#[test]
fn boundary_values_roundtrip_through_encoder() {
    let cases: [Inst; 32] = [
        Inst::Addi(r(2), r(3), -128),
        Inst::Addi(r(2), r(3), 127),
        Inst::AddiN(r(2), r(3), -1),
        Inst::AddiN(r(2), r(3), 15),
        Inst::Addmi(r(2), r(3), -32768),
        Inst::Addmi(r(2), r(3), 32512),
        Inst::Movi(r(2), -2048),
        Inst::Movi(r(2), 2047),
        Inst::MoviN(r(2), -32),
        Inst::MoviN(r(2), 95),
        Inst::Load(LoadOp::L8ui, r(2), r(3), 255),
        Inst::Load(LoadOp::L16ui, r(2), r(3), 510),
        Inst::Load(LoadOp::L16si, r(2), r(3), 510),
        Inst::Load(LoadOp::L32i, r(2), r(3), 1020),
        Inst::Store(StoreOp::S8i, r(2), r(3), 255),
        Inst::Store(StoreOp::S16i, r(2), r(3), 510),
        Inst::Store(StoreOp::S32i, r(2), r(3), 1020),
        Inst::L32iN(r(2), r(3), 60),
        Inst::S32iN(r(2), r(3), 60),
        Inst::Entry(r(1), 0),
        Inst::Entry(r(1), 32760),
        Inst::Slli(r(2), r(3), 1),
        Inst::Slli(r(2), r(3), 31),
        Inst::Srli(r(2), r(3), 15),
        Inst::Srai(r(2), r(3), 31),
        Inst::Ssai(31),
        Inst::Extui(r(2), r(3), 31, 1),
        Inst::Extui(r(2), r(3), 16, 16),
        Inst::Sext(r(2), r(3), 7),
        Inst::Sext(r(2), r(3), 22),
        Inst::BranchBiI(true, r(2), 31, -128),
        Inst::BranchBiI(false, r(2), 0, 127),
    ];
    for inst in cases {
        assert_eq!(roundtrip(inst), inst, "boundary round-trip failed");
    }
}

/// Branch/jump/call displacement fields round-trip at the extremes the table
/// declares legal (raw stored offsets, per lp-xt-inst's conventions).
#[test]
fn pc_relative_boundaries_roundtrip() {
    // RRI8 branches: stored offset == displacement from PC + 4.
    for off in [-128, 127] {
        let inst = Inst::BranchRr(BrRr::Beq, r(2), r(3), off);
        assert_eq!(roundtrip(inst), inst);
    }
    // BRI12: -2048..=2047.
    for off in [-2048, 2047] {
        let inst = Inst::BranchZ(BrZ::Beqz, r(2), off);
        assert_eq!(roundtrip(inst), inst);
    }
    // beqz.n/bnez.n: unsigned forward 0..=63.
    for off in [0u32, 63] {
        let inst = Inst::BranchZN(true, r(2), off);
        assert_eq!(roundtrip(inst), inst);
    }
    // J: signed 18-bit byte offset.
    for off in [-131072, 131071] {
        let inst = Inst::J(off);
        assert_eq!(roundtrip(inst), inst);
    }
    // CALL8: signed 18-bit WORD offset — the table's byte displacement / 4.
    for byte_disp in [-524288i32, 524284] {
        assert!(is_legal(ImmOp::CallDisp, byte_disp));
        let inst = Inst::Call(CallOp::Call8, byte_disp / 4);
        assert_eq!(roundtrip(inst), inst);
    }
}

/// The L32R field is one-extended: the table's displacement range maps exactly
/// onto lp-xt-inst's `l32r_target` at both field extremes.
#[test]
fn l32r_field_extremes_match_table_range() {
    let pc = 0x4000_0000u32; // aligned, so base = (PC + 3) & !3 == PC
    // Field 0xFFFF => word offset -1 => byte displacement -4 (table max).
    assert_eq!(l32r_target(pc, 0xFFFF), pc.wrapping_sub(4));
    assert!(is_legal(ImmOp::L32rDisp, -4));
    // Field 0x0000 => word offset -65536 => byte displacement -262144 (table min).
    assert_eq!(l32r_target(pc, 0x0000), pc.wrapping_sub(262144));
    assert!(is_legal(ImmOp::L32rDisp, -262144));
    // Field 0x7FFF (a "positive-looking" i16) is still backward: -32769 words.
    assert_eq!(l32r_target(pc, 0x7FFF), pc.wrapping_sub(32769 * 4));
    assert!(is_legal(ImmOp::L32rDisp, -(32769 * 4)));
    // Raw field round-trips at both extremes.
    for field in [0x0000u16, 0x7FFF, 0x8000, 0xFFFF] {
        let inst = Inst::L32r(r(2), field);
        assert_eq!(roundtrip(inst), inst);
    }
    // Backward-only: no positive displacement is legal.
    assert!(!is_legal(ImmOp::L32rDisp, 0));
    assert!(!is_legal(ImmOp::L32rDisp, 4));
}

/// The shared encoder does NOT validate: just-out-of-range immediates encode
/// to *different* values (silent truncation/aliasing). This is the reason the
/// table must gate every immediate before encoding.
#[test]
fn encoder_silently_truncates_out_of_range_values() {
    // addi 128 aliases to -128 (imm8 wraps).
    let bad = Inst::Addi(r(2), r(3), 128);
    assert_eq!(roundtrip(bad), Inst::Addi(r(2), r(3), -128));
    assert!(!is_legal(ImmOp::Addi, 128));
    // entry 32768 aliases to frame 0 (imm12 << 3 wraps).
    let bad = Inst::Entry(r(1), 32768);
    assert_eq!(roundtrip(bad), Inst::Entry(r(1), 0));
    assert!(!is_legal(ImmOp::EntryFrame, 32768));
    // l32i offset 1021 aliases down to 1020 (scale-4 floor).
    let bad = Inst::Load(LoadOp::L32i, r(2), r(3), 1021);
    assert_eq!(roundtrip(bad), Inst::Load(LoadOp::L32i, r(2), r(3), 1020));
    assert!(!is_legal(ImmOp::L32i, 1021));
    // movi 2048 aliases to -2048 (imm12 wraps).
    let bad = Inst::Movi(r(2), 2048);
    assert_eq!(roundtrip(bad), Inst::Movi(r(2), -2048));
    assert!(!is_legal(ImmOp::Movi, 2048));
}

/// Fallback docs stay wired to the table (the emitter's actual lowerings).
#[test]
fn fallbacks_are_the_documented_lowerings() {
    assert_eq!(fallback(ImmOp::Addi), Fallback::AddmiSplit);
    assert_eq!(fallback(ImmOp::Movi), Fallback::LiteralPool);
    assert_eq!(fallback(ImmOp::Addmi), Fallback::ConstThenReg);
    assert_eq!(fallback(ImmOp::L32i), Fallback::AddressScratch);
    assert_eq!(fallback(ImmOp::S8i), Fallback::AddressScratch);
    assert_eq!(fallback(ImmOp::Branch8Disp), Fallback::InvertOverJ);
    assert_eq!(fallback(ImmOp::Branch12Disp), Fallback::InvertOverJ);
    assert_eq!(fallback(ImmOp::Branch6NDisp), Fallback::WideForm);
    assert_eq!(fallback(ImmOp::JDisp), Fallback::IndirectViaL32r);
    assert_eq!(fallback(ImmOp::CallDisp), Fallback::IndirectViaL32r);
    assert_eq!(fallback(ImmOp::EntryFrame), Fallback::HardError);
    assert_eq!(fallback(ImmOp::L32rDisp), Fallback::HardError);
    assert_eq!(fallback(ImmOp::SrliSa), Fallback::OtherOpcode); // extui
    assert_eq!(fallback(ImmOp::SlliSa), Fallback::OtherOpcode); // mov / movi 0
    assert_eq!(fallback(ImmOp::SextBit), Fallback::OtherOpcode); // slli+srai
    assert_eq!(fallback(ImmOp::BranchB4Const), Fallback::ConstThenReg);
    assert_eq!(fallback(ImmOp::BranchB4Constu), Fallback::ConstThenReg);
    assert_eq!(fallback(ImmOp::FpLsiOffset), Fallback::AddressScratch);
}

/// The float spill offset is the second silent-corruption hazard in the frame
/// story, after the window-overflow one — and unlike that one it has no
/// hardware alibi: `lp_xt_inst`'s encoder computes `lsi`/`ssi`'s field as
/// `(offset / 4) & 0xff` with **no range check**.
///
/// A float spill slot at byte offset 1024 therefore encodes as field 0 and
/// addresses `[base + 0]`: a valid address holding some other live value.
/// Nothing downstream can tell that apart from a correct spill — no fault, no
/// misalignment, just a wrong number. `is_legal(FpLsiOffset, …)` is the gate
/// that has to catch it before the encoder sees it, so the aliasing is pinned
/// here rather than assumed.
#[test]
fn out_of_range_float_spill_offsets_alias_and_must_be_rejected() {
    use lp_xt_inst::fp::{FReg, FpLsiOp};

    let f = FReg::new(1);
    for (bad, aliases_to) in [(1024u32, 0u32), (1028, 4), (2044, 1020)] {
        assert!(
            !is_legal(ImmOp::FpLsiOffset, bad as i32),
            "{bad} must be rejected before it reaches the encoder"
        );
        assert_eq!(
            roundtrip(Inst::FpLsi(FpLsiOp::Ssi, f, r(1), bad)),
            Inst::FpLsi(FpLsiOp::Ssi, f, r(1), aliases_to),
            "ssi offset {bad} silently became {aliases_to}"
        );
    }
    // Sub-word offsets floor rather than fault, the same way `l32i`'s do.
    assert!(!is_legal(ImmOp::FpLsiOffset, 1021));
    assert_eq!(
        roundtrip(Inst::FpLsi(FpLsiOp::Lsi, f, r(1), 1021)),
        Inst::FpLsi(FpLsiOp::Lsi, f, r(1), 1020)
    );

    // The whole legal range does round-trip exactly, so the gate is not
    // over-tight either.
    for off in (0..=1020u32).step_by(4) {
        assert!(is_legal(ImmOp::FpLsiOffset, off as i32));
        let inst = Inst::FpLsi(FpLsiOp::Lsi, f, r(2), off);
        assert_eq!(roundtrip(inst), inst, "lsi offset {off}");
    }
}

/// PC-relative bases are declared correctly for every displacement entry.
#[test]
fn pc_relative_bases() {
    assert_eq!(spec(ImmOp::Branch8Disp).pc_rel, PcRel::NextPc);
    assert_eq!(spec(ImmOp::Branch12Disp).pc_rel, PcRel::NextPc);
    assert_eq!(spec(ImmOp::Branch6NDisp).pc_rel, PcRel::NextPc);
    assert_eq!(spec(ImmOp::JDisp).pc_rel, PcRel::NextPc);
    assert_eq!(spec(ImmOp::CallDisp).pc_rel, PcRel::AlignedNextPc);
    assert_eq!(spec(ImmOp::L32rDisp).pc_rel, PcRel::AlignedPcPlus3);
    // Everything else is not PC-relative.
    for &op in ImmOp::ALL {
        if !matches!(
            op,
            ImmOp::Branch8Disp
                | ImmOp::Branch12Disp
                | ImmOp::Branch6NDisp
                | ImmOp::JDisp
                | ImmOp::CallDisp
                | ImmOp::L32rDisp
        ) {
            assert_eq!(spec(op).pc_rel, PcRel::None, "{op:?}");
        }
    }
}

/// The emitter's own immediate paths agree with the table (the two must not
/// drift while `emit.rs` still carries inline ranges).
#[test]
fn emitter_add_imm_thresholds_match_table() {
    // add_imm uses addi for -128..=127...
    assert!(is_legal(ImmOp::Addi, -128) && is_legal(ImmOp::Addi, 127));
    // ...addmi for multiples of 256 in -32768..=32512...
    assert!(is_legal(ImmOp::Addmi, -32768) && is_legal(ImmOp::Addmi, 32512));
    assert!(!is_legal(ImmOp::Addmi, -33024) && !is_legal(ImmOp::Addmi, 32768));
    // ...and iconst uses movi for -2048..=2047 (else pool).
    assert!(is_legal(ImmOp::Movi, -2048) && is_legal(ImmOp::Movi, 2047));
    // The Rrr And/Or/Xor register forms the NoImmForm fallback relies on exist
    // in the shared encoder (and round-trip).
    for op in [AluRrr::And, AluRrr::Or, AluRrr::Xor] {
        let inst = Inst::Rrr(op, r(4), r(5), r(6));
        assert_eq!(roundtrip(inst), inst);
    }
}
