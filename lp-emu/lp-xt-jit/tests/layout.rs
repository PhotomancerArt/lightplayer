//! The exchange layout, the decoded form and the block walk — everything this
//! crate decides that needs no engine to check.

use lp_emu_jit::blocks::DecodedInst;
use lp_emu_jit::host::ExchangeLayout;
use lp_xt_inst::{Inst, NullaryNarrowOp, NullaryOp, Reg, SpecialReg, SrOp};
use lp_xt_jit::blocks::BlockEnd;
use lp_xt_jit::decode::{Decode, Decoded};
use lp_xt_jit::{LAYOUT, extra};

/// The Xtensa layout is XD8's data half and a **wire format**: the emitted
/// module folds these offsets in as constants and the driver reads the same
/// bytes back. If the two ever disagreed both sides would still be
/// self-consistent and the machine would be quietly wrong, which is why the
/// numbers are asserted rather than derived twice.
#[test]
fn the_xtensa_layout_is_the_ar_file_and_eight_words() {
    assert_eq!(
        LAYOUT.regs_words, 64,
        "the physical AR file, not the window"
    );
    assert_eq!(LAYOUT.extra_words, lp_xt_jit::EXTRA_WORDS);
    assert_eq!(LAYOUT.head_bytes(), 4 * (64 + 8));

    // The register file is physical and starts at +0, like every layout's.
    assert_eq!(LAYOUT.reg(0), 0);
    assert_eq!(LAYOUT.reg(63), 4 * 63);

    // The extras sit between the file and the protocol's own fields, touching
    // neither.
    assert_eq!(extra(extra::WINDOW_BASE), 4 * 64);
    assert_eq!(extra(extra::DIRTY), 4 * 71);
    assert_eq!(extra(extra::DIRTY) + 4, LAYOUT.cycle());

    // The protocol's own fields, past 288 bytes of architectural state.
    assert_eq!(LAYOUT.cycle(), 288);
    assert_eq!(LAYOUT.instret(), 296);
    assert_eq!(LAYOUT.flags(), 304);
    assert_eq!(LAYOUT.status(), 308);
    assert_eq!(LAYOUT.cross(), 312);
    assert_eq!(LAYOUT.indirect_miss(), 320);
    assert_eq!(LAYOUT.exit_why(), 328);
    for field in [
        LAYOUT.cycle(),
        LAYOUT.instret(),
        LAYOUT.cross(),
        LAYOUT.indirect_miss(),
    ] {
        assert_eq!(field % 8, 0, "{field} is an i64 field and is not aligned");
    }

    // Longer than RV32's, and nothing else about it differs.
    assert!(LAYOUT.len() > ExchangeLayout::RV32.len());
    assert_eq!(
        LAYOUT.len() - LAYOUT.head_bytes() as u32,
        ExchangeLayout::RV32.len() - ExchangeLayout::RV32.head_bytes() as u32,
        "the protocol's own tail is the same for every architecture"
    );
}

/// Byte granularity, against RV32's two — all four `pc mod 4` residues are
/// live on Xtensa, so a coarser table would miss most block starts.
#[test]
fn the_indirect_target_granularity_is_the_byte() {
    assert_eq!(Decoded::SLOT_SHIFT, 0);
    // The same answer, for the same reason, as the hart's own entry table:
    // two addresses one byte apart are two different slots on both sides.
    use lp_xt_emu::mach::translated::entry_slot;
    assert_ne!(entry_slot(0x4000_0004), entry_slot(0x4000_0005));
}

fn decoded(inst: &Inst) -> Decode {
    lp_xt_jit::decode::decode(&lp_xt_inst::encode(inst))
}

/// The three-way split the module docs state: body, terminator, and
/// undecodable-for-the-translator.
#[test]
fn the_classification_is_p01s_with_the_three_moved_across() {
    // Body: the block runs through it.
    let Decode::Ok(d) = decoded(&Inst::Nullary(NullaryOp::Nop)) else {
        panic!("`nop` is a body instruction");
    };
    assert_eq!(d.width, 3);
    assert!(!d.control);

    let Decode::Ok(d) = decoded(&Inst::NullaryN(NullaryNarrowOp::NopN)) else {
        panic!("`nop.n` is a body instruction");
    };
    assert_eq!(d.width, 2, "the density rule's two bytes");
    assert!(!d.control);

    // Terminator: the block ends after it.
    let Decode::Ok(d) = decoded(&Inst::Nullary(NullaryOp::Ret)) else {
        panic!("`ret` is a terminator");
    };
    assert!(d.control);

    // Undecodable for the translator: the block ends BEFORE it and the
    // interpreter runs it. `isync` and the whole `rsr`/`wsr`/`xsr` family are
    // terminators for the block cache and undecodable here — see the module
    // docs for why the two classifications differ on exactly these.
    assert!(matches!(
        decoded(&Inst::Nullary(NullaryOp::Isync)),
        Decode::Undecodable { .. }
    ));
    assert!(matches!(
        decoded(&Inst::Sr(SrOp::Wsr, SpecialReg::Lbeg, Reg::new(2))),
        Decode::Undecodable { .. }
    ));
    assert!(matches!(
        decoded(&Inst::Nullary(NullaryOp::Ill)),
        Decode::Undecodable { .. }
    ));
}

/// Bytes that decode to nothing end a block rather than being guessed at, and
/// the width handed back is the density rule's so the walk can step over them.
#[test]
fn an_undecodable_word_is_refused_with_the_density_rules_width() {
    // `op0 = 0xE` is a three-byte form; this particular word is not an
    // instruction the decoder knows.
    match lp_xt_jit::decode::decode(&[0xFE, 0xFF, 0xFF]) {
        Decode::Refused { width } | Decode::Undecodable { width } => assert_eq!(width, 3),
        Decode::Ok(d) => panic!("0xFFFFFE decoded as {:?}", d.inst),
    }
    // Nothing fetched at all: step by the longest an instruction can be, so
    // the walk cannot land inside one it could have decoded.
    match lp_xt_jit::decode::decode(&[]) {
        Decode::Refused { width } => assert_eq!(width, 3),
        other => panic!("an empty fetch is refused, not {other:?}"),
    }
}

/// The stub walk: follow decoded widths, cut at the classification's
/// boundaries, and never let two blocks overlap.
#[test]
fn the_walk_cuts_where_the_classification_says() {
    const BASE: u32 = 0x4000_0000;
    let mut image = Vec::new();
    image.extend_from_slice(&lp_xt_inst::encode(&Inst::NullaryN(NullaryNarrowOp::NopN)));
    image.extend_from_slice(&lp_xt_inst::encode(&Inst::Nullary(NullaryOp::Nop)));
    image.extend_from_slice(&lp_xt_inst::encode(&Inst::Nullary(NullaryOp::Ret)));
    // Past the terminator: a second block's worth, ending before an `isync`.
    image.extend_from_slice(&lp_xt_inst::encode(&Inst::Nullary(NullaryOp::Nop)));
    image.extend_from_slice(&lp_xt_inst::encode(&Inst::Nullary(NullaryOp::Isync)));
    let second = BASE + 2 + 3 + 3;

    let mut fetch = |pc: u32| {
        pc.checked_sub(BASE)
            .and_then(|o| image.get(o as usize))
            .copied()
    };
    let found = lp_xt_jit::discover::build(&[BASE, second], &mut fetch);

    assert_eq!(found.stats.blocks, 2);
    assert_eq!(found.stats.insts, 4, "the `isync` is not in a block");
    assert_eq!(found.set.blocks[0].pc, BASE);
    assert_eq!(found.set.blocks[0].insts.len(), 3);
    assert_eq!(found.set.blocks[0].end, BlockEnd::Term);
    assert_eq!(found.set.blocks[0].end_pc(), second);
    assert_eq!(found.set.blocks[1].pc, second);
    assert_eq!(
        found.set.blocks[1].end,
        BlockEnd::Undecodable(second + 3),
        "the block ends at the `isync`, and the interpreter runs it"
    );
    assert_eq!(found.stats.undecodable, 1);
}

/// A start whose next instruction is another supplied start falls into it
/// rather than decoding the same bytes twice — two modules answering for one
/// pc is the one thing a block set may never do.
#[test]
fn a_block_falls_into_the_next_start_rather_than_overlapping_it() {
    const BASE: u32 = 0x4000_0000;
    let mut image = Vec::new();
    for _ in 0..3 {
        image.extend_from_slice(&lp_xt_inst::encode(&Inst::Nullary(NullaryOp::Nop)));
    }
    image.extend_from_slice(&lp_xt_inst::encode(&Inst::Nullary(NullaryOp::Ret)));

    let mut fetch = |pc: u32| {
        pc.checked_sub(BASE)
            .and_then(|o| image.get(o as usize))
            .copied()
    };
    let found = lp_xt_jit::discover::build(&[BASE, BASE + 3], &mut fetch);
    assert_eq!(found.set.blocks[0].insts.len(), 1);
    assert_eq!(found.set.blocks[0].end, BlockEnd::Fall(BASE + 3));
    assert_eq!(found.set.blocks[1].insts.len(), 3);
    assert_eq!(found.set.blocks[1].end, BlockEnd::Term);
}
