//! The sweep's rules, one test each (`discover.rs`'s numbered list), on
//! images assembled with `lp_xt_inst::encode` so the bytes are the real
//! encodings and not hand-typed ones.

use std::collections::BTreeSet;

use lp_xt_inst::{
    BrRr, BrZ, CallOp, CallxOp, Inst, LoopOp, NullaryNarrowOp, NullaryOp, Reg, SpecialReg, SrOp,
};
use lp_xt_jit::blocks::BlockEnd;
use lp_xt_jit::decode::{Decode, Edges, decode, edges};
use lp_xt_jit::discover::{Bounds, Extent, discover, discover_from};

/// A guest image: `(address, bytes)` runs, served byte by byte.
struct Image {
    runs: Vec<(u32, Vec<u8>)>,
}

impl Image {
    fn new() -> Self {
        Self { runs: Vec::new() }
    }

    /// Assemble `insts` at `at`, and say where the next byte would go.
    fn place(&mut self, at: u32, insts: &[Inst]) -> u32 {
        let mut bytes = Vec::new();
        for inst in insts {
            bytes.extend_from_slice(&lp_xt_inst::encode(inst));
        }
        let end = at + bytes.len() as u32;
        self.runs.push((at, bytes));
        end
    }

    fn raw(&mut self, at: u32, bytes: &[u8]) -> u32 {
        self.runs.push((at, bytes.to_vec()));
        at + bytes.len() as u32
    }

    fn fetch(&self) -> impl FnMut(u32) -> Option<u8> + '_ {
        move |pc| {
            self.runs.iter().find_map(|(at, bytes)| {
                pc.checked_sub(*at)
                    .and_then(|o| bytes.get(o as usize))
                    .copied()
            })
        }
    }
}

fn a(n: u8) -> Reg {
    Reg::new(n)
}

fn starts_of(found: &lp_xt_jit::discover::Discovered) -> Vec<u32> {
    found.set.blocks.iter().map(|b| b.pc).collect()
}

/// The target formulas in `decode::edges` against the disassembler's, which
/// resolves the same fields for `objdump`-style output. Both live in crates
/// this one depends on; they must not disagree about where an edge goes.
#[test]
fn edges_agree_with_the_disassembler_at_every_residue() {
    let target_in = |pc: u32, inst: &Inst| -> u32 {
        let text = lp_xt_inst::disasm::format_inst(inst, pc);
        let hex = text
            .rsplit(", ")
            .next()
            .and_then(|s| s.strip_prefix("0x"))
            .or_else(|| text.rsplit('\t').next().and_then(|s| s.strip_prefix("0x")))
            .unwrap_or_else(|| panic!("no target in `{text}`"));
        u32::from_str_radix(hex, 16).unwrap()
    };
    for pc in [0x4000_1000u32, 0x4000_1001, 0x4000_1002, 0x4000_1003] {
        let cases: Vec<(Inst, u8)> = vec![
            (Inst::J(-40), 3),
            (Inst::J(100), 3),
            (Inst::BranchZ(BrZ::Beqz, a(2), -8), 3),
            (Inst::BranchRr(BrRr::Bne, a(2), a(3), 12), 3),
            (Inst::BranchZN(true, a(2), 6), 2),
            (Inst::BranchBiI(true, a(2), 3, -100), 3),
            (Inst::Call(CallOp::Call8, 3), 3),
            (Inst::Call(CallOp::Call0, -7), 3),
        ];
        for (inst, width) in cases {
            let d = match decode(&lp_xt_inst::encode(&inst)) {
                Decode::Ok(d) => d,
                other => panic!("{inst:?} did not decode: {other:?}"),
            };
            assert_eq!(d.width, width, "{inst:?}");
            let ours = match edges(pc, &d) {
                Edges::Jump(t) | Edges::Branch { target: t, .. } => t,
                Edges::Call {
                    target: Some(t), ..
                } => t,
                other => panic!("{inst:?}: {other:?}"),
            };
            assert_eq!(ours, target_in(pc, &inst), "{inst:?} at {pc:#x}");
        }
        // `loop` names `LEND`, and the disassembler prints exactly that.
        let inst = Inst::Loop(LoopOp::Loopnez, a(3), 9);
        let Decode::Ok(d) = decode(&lp_xt_inst::encode(&inst)) else {
            panic!("loopnez")
        };
        let Edges::Loop { body, end } = edges(pc, &d) else {
            panic!("not a loop")
        };
        assert_eq!(body, pc + 3);
        assert_eq!(end, target_in(pc, &inst));
        assert_eq!(end, lp_xt_inst::disasm::loop_end(pc, 9));
    }
}

/// Rule 5: a branch names its target and its fall-through, a `j` its target.
#[test]
fn the_walk_follows_a_branch_and_a_jump() {
    const F: u32 = 0x4000_1000;
    // F+0: movi.n a2, 1        (2)
    // F+2: beqz a2, F+12       (3)  target = F+2+4+6 = F+12
    // F+5: addi a2, a2, 1      (3)
    // F+8: j F+14              (3)  target = F+8+4+2 = F+14
    // F+11: one pad byte
    // F+12: movi.n a3, 2       (2)  the `beqz` target
    // F+14: ret.n              (2)  the `j` target
    let mut img = Image::new();
    let end = img.place(
        F,
        &[
            Inst::MoviN(a(2), 1),
            Inst::BranchZ(BrZ::Beqz, a(2), 6),
            Inst::Addi(a(2), a(2), 1),
            Inst::J(2),
        ],
    );
    assert_eq!(end, F + 11);
    img.raw(F + 11, &[0x00]);
    img.place(
        F + 12,
        &[Inst::MoviN(a(3), 2), Inst::NullaryN(NullaryNarrowOp::RetN)],
    );
    let extents = [Extent {
        start: F,
        end: F + 16,
    }];
    let bounds = Bounds {
        extents: &extents,
        spans: &[],
    };
    let found = discover(&[F], bounds, usize::MAX, &mut img.fetch());
    assert_eq!(starts_of(&found), vec![F, F + 5, F + 12, F + 14]);
    let b = &found.set.blocks;
    assert_eq!(b[0].insts.len(), 2, "movi.n + beqz");
    assert_eq!(b[0].end, BlockEnd::Term);
    assert_eq!(b[1].insts.len(), 2, "addi + j");
    assert_eq!(b[2].insts.len(), 1, "movi.n, then the `j` target starts");
    assert_eq!(b[2].end, BlockEnd::Fall(F + 14));
    assert_eq!(b[3].insts.len(), 1, "ret.n");
    assert_eq!(found.stats.blocks, 4);
    assert_eq!(found.stats.empty_starts, 0);
    assert!(!found.stats.truncated);
}

/// Rule 5: `call8` names its target **and** its return address; `callx8`
/// names its return address only. Rule 3: each block is bounded by its own
/// symbol's extent, and the callee is found through the edge without being a
/// seed.
#[test]
fn a_call_names_its_target_and_its_return() {
    let mut img = Image::new();
    const F: u32 = 0x4000_1000;
    const G: u32 = 0x4000_1100;
    // F+0: entry a1, 32   (3)
    // F+3: call8 G        (3)  words = (G - 4 - (F+3 & !3)) >> 2
    // F+6: callx8 a4      (3)
    // F+9: retw.n         (2)
    let words = ((G - 4 - ((F + 3) & !3)) >> 2) as i32;
    let f_end = img.place(
        F,
        &[
            Inst::Entry(a(1), 32),
            Inst::Call(CallOp::Call8, words),
            Inst::Callx(CallxOp::Callx8, a(4)),
            Inst::NullaryN(NullaryNarrowOp::RetwN),
        ],
    );
    let g_end = img.place(
        G,
        &[
            Inst::Entry(a(1), 16),
            Inst::NullaryN(NullaryNarrowOp::RetwN),
        ],
    );
    let extents = [
        Extent {
            start: F,
            end: f_end,
        },
        Extent {
            start: G,
            end: g_end,
        },
    ];
    let bounds = Bounds {
        extents: &extents,
        spans: &[],
    };
    // Seeded at F only: G is reached by the call edge.
    let found = discover(&[F], bounds, usize::MAX, &mut img.fetch());
    assert_eq!(starts_of(&found), vec![F, F + 3, F + 6, F + 9, G, G + 3]);
    let b = &found.set.blocks;
    // `entry` is a start and a one-instruction block (it rotates).
    assert_eq!(b[0].insts.len(), 1);
    assert!(matches!(b[0].insts[0].1.inst, Inst::Entry(..)));
    assert_eq!(b[0].end, BlockEnd::Term);
    assert_eq!(b[1].insts.len(), 1, "the call8 ends its block");
    assert_eq!(b[2].insts.len(), 1, "the callx8 ends its block");
    assert_eq!(b[3].insts.len(), 1, "retw.n at the return address");
    assert_eq!(b[4].pc, G);
    assert_eq!(found.stats.seeds, 1);
    assert_eq!(found.stats.blocks, 6);
}

/// Rule 3: a symbol whose `st_size` ends mid-way through a run of decodable
/// bytes stops the walk at the extent — as a `Fall` when a symbol starts
/// exactly there, as `Undecodable` when none does.
#[test]
fn an_extent_ends_the_block_where_the_symbol_does() {
    let mut img = Image::new();
    const F: u32 = 0x4000_2000;
    // Twelve decodable bytes: six `movi.n`. The symbol claims eight.
    img.place(F, &[Inst::MoviN(a(2), 1); 6]);
    let extents = [Extent {
        start: F,
        end: F + 8,
    }];
    let found = discover(
        &[F],
        Bounds {
            extents: &extents,
            spans: &[],
        },
        usize::MAX,
        &mut img.fetch(),
    );
    assert_eq!(found.set.blocks.len(), 1);
    assert_eq!(found.set.blocks[0].insts.len(), 4);
    assert_eq!(found.set.blocks[0].end, BlockEnd::Undecodable(F + 8));
    assert_eq!(found.stats.extent_ends, 1);

    // The same bytes with a second symbol starting at F+8: the block falls
    // into it, and the second symbol — a seed like any other — is walked.
    let extents = [
        Extent {
            start: F,
            end: F + 8,
        },
        Extent {
            start: F + 8,
            end: F + 12,
        },
    ];
    let found = discover(
        &[F, F + 8],
        Bounds {
            extents: &extents,
            spans: &[],
        },
        usize::MAX,
        &mut img.fetch(),
    );
    assert_eq!(starts_of(&found), vec![F, F + 8]);
    assert_eq!(found.set.blocks[0].end, BlockEnd::Fall(F + 8));
    assert_eq!(found.set.blocks[1].insts.len(), 2);

    // A 3-byte instruction straddling the extent is not the block's either.
    let mut img = Image::new();
    img.place(F, &[Inst::MoviN(a(2), 1), Inst::Addi(a(2), a(2), 1)]);
    let extents = [Extent {
        start: F,
        end: F + 4,
    }];
    let found = discover(
        &[F],
        Bounds {
            extents: &extents,
            spans: &[],
        },
        usize::MAX,
        &mut img.fetch(),
    );
    assert_eq!(found.set.blocks[0].insts.len(), 1);
    assert_eq!(found.set.blocks[0].end, BlockEnd::Undecodable(F + 2));
}

/// Rule 7: an instruction the translator refuses (`wsr`, `isync`) ends the
/// block before it, and the walk steps over it by the decoder's width to the
/// next start — the rest of the function is not lost.
#[test]
fn a_refused_instruction_is_stepped_over_by_its_own_width() {
    let mut img = Image::new();
    const F: u32 = 0x4000_3000;
    // F+0: movi.n a2, 1       (2)
    // F+2: wsr a2, lbeg       (3)  refused by the translator
    // F+5: isync              (3)  refused by the translator
    // F+8: movi.n a3, 2       (2)
    // F+10: ret.n             (2)
    let end = img.place(
        F,
        &[
            Inst::MoviN(a(2), 1),
            Inst::Sr(SrOp::Wsr, SpecialReg::Lbeg, a(2)),
            Inst::Nullary(NullaryOp::Isync),
            Inst::MoviN(a(3), 2),
            Inst::NullaryN(NullaryNarrowOp::RetN),
        ],
    );
    assert_eq!(end, F + 12);
    let extents = [Extent { start: F, end }];
    let found = discover(
        &[F],
        Bounds {
            extents: &extents,
            spans: &[],
        },
        usize::MAX,
        &mut img.fetch(),
    );
    // The block ends before the `wsr`; the walk steps to F+5, which is the
    // `isync` — a start holding only a refused instruction, so no block —
    // and from there to F+8, which holds the rest of the function. F+2
    // itself is never a start: the refused instruction is the interpreter's.
    assert_eq!(starts_of(&found), vec![F, F + 8]);
    assert_eq!(found.stats.starts, 3);
    assert_eq!(found.set.blocks[0].insts.len(), 1);
    assert_eq!(found.set.blocks[0].end, BlockEnd::Undecodable(F + 2));
    assert_eq!(found.set.blocks[1].insts.len(), 2);
    assert_eq!(found.set.blocks[1].end, BlockEnd::Term);
    assert_eq!(found.stats.undecodable, 1);
    assert_eq!(found.stats.empty_starts, 1);
    assert_eq!(found.stats.refused, 0);
}

/// Rule 7's exception: `ill` is refused **and** not stepped over. Zeroed
/// memory decodes as `ill`, three bytes at a time, and a walk that stepped
/// over it would make an empty start of every zero word until the bound.
#[test]
fn ill_is_not_stepped_over_so_zeroed_memory_is_not_marched_through() {
    let mut img = Image::new();
    const F: u32 = 0x4008_9000;
    let mid = img.place(F, &[Inst::MoviN(a(2), 1)]);
    assert!(matches!(
        decode(&[0, 0, 0]),
        Decode::Undecodable {
            inst: Inst::Nullary(NullaryOp::Ill),
            ..
        }
    ));
    let end = img.raw(mid, &[0u8; 300]);
    let spans = [(F, end)];
    let found = discover(
        &[F],
        Bounds {
            extents: &[],
            spans: &spans,
        },
        usize::MAX,
        &mut img.fetch(),
    );
    assert_eq!(found.stats.starts, 1, "no start past the ill");
    assert_eq!(found.set.blocks[0].end, BlockEnd::Undecodable(mid));
    assert_eq!(found.stats.refused, 1);
    assert_eq!(found.stats.undecodable, 0);
}

/// Rule 7's second half: bytes the decoder does not decode at all end the
/// walk. No bounded skip — the width of an unknown encoding is a guess, and
/// the sweep never guesses one.
#[test]
fn bytes_the_decoder_refuses_end_the_walk() {
    let mut img = Image::new();
    const F: u32 = 0x4000_4000;
    let mid = img.place(F, &[Inst::MoviN(a(2), 1)]);
    // `op0 = 0xe` is a reserved format on the LX6.
    let refused = [0x0eu8, 0x00, 0x00];
    assert!(
        matches!(decode(&refused), Decode::Refused { .. }),
        "the probe bytes must be ones the decoder refuses"
    );
    let after = img.raw(mid, &refused);
    let end = img.place(
        after,
        &[Inst::MoviN(a(3), 2), Inst::NullaryN(NullaryNarrowOp::RetN)],
    );
    let extents = [Extent { start: F, end }];
    let found = discover(
        &[F],
        Bounds {
            extents: &extents,
            spans: &[],
        },
        usize::MAX,
        &mut img.fetch(),
    );
    assert_eq!(
        starts_of(&found),
        vec![F],
        "nothing past the refused word is a start"
    );
    assert_eq!(found.set.blocks[0].end, BlockEnd::Undecodable(mid));
    assert_eq!(found.stats.refused, 1);
    // A seed past it is walked like any other.
    let found = discover(
        &[F, after],
        Bounds {
            extents: &extents,
            spans: &[],
        },
        usize::MAX,
        &mut img.fetch(),
    );
    assert_eq!(starts_of(&found), vec![F, after]);
}

/// Rule 8's stop set: a second walk neither claims nor follows a block an
/// installed module already holds.
#[test]
fn a_known_start_stops_the_walk_and_is_never_claimed() {
    let mut img = Image::new();
    const F: u32 = 0x4000_5000;
    // F+0: movi.n a2, 1 (2)  F+2: movi.n a3, 1 (2)  F+4: j F  (3)
    let end = img.place(
        F,
        &[Inst::MoviN(a(2), 1), Inst::MoviN(a(3), 1), Inst::J(-8)],
    );
    let extents = [Extent { start: F, end }];
    let bounds = Bounds {
        extents: &extents,
        spans: &[],
    };
    let whole = discover(&[F], bounds, usize::MAX, &mut img.fetch());
    assert_eq!(whole.set.blocks.len(), 1);
    assert_eq!(whole.set.blocks[0].insts.len(), 3);

    let known: BTreeSet<u32> = [F + 2].into_iter().collect();
    let part = discover_from(&[F], bounds, usize::MAX, &known, &mut img.fetch());
    assert_eq!(starts_of(&part), vec![F]);
    assert_eq!(part.set.blocks[0].insts.len(), 1);
    assert_eq!(part.set.blocks[0].end, BlockEnd::Fall(F + 2));

    let known: BTreeSet<u32> = [F].into_iter().collect();
    let none = discover_from(&[F], bounds, usize::MAX, &known, &mut img.fetch());
    assert!(none.set.is_empty());
}

/// A host bound binds and says so.
#[test]
fn the_budget_is_reported_when_it_binds() {
    let mut img = Image::new();
    const F: u32 = 0x4000_6000;
    let end = img.place(
        F,
        &[
            Inst::MoviN(a(2), 1),
            Inst::BranchZ(BrZ::Beqz, a(2), 3),
            Inst::MoviN(a(3), 1),
            Inst::NullaryN(NullaryNarrowOp::NopN),
            Inst::NullaryN(NullaryNarrowOp::RetN),
        ],
    );
    let extents = [Extent { start: F, end }];
    let found = discover(
        &[F],
        Bounds {
            extents: &extents,
            spans: &[],
        },
        1,
        &mut img.fetch(),
    );
    assert!(found.stats.truncated);
    assert_eq!(found.stats.starts, 1);
}

/// Rule 3 and rule 8 together: a start in no symbol is bounded by its
/// executable span, capped at the next symbol's start; a start in no span
/// holds nothing.
#[test]
fn a_start_outside_every_symbol_is_bounded_by_its_span() {
    let mut img = Image::new();
    const H: u32 = 0x4008_8000;
    const S: u32 = 0x4008_8010;
    img.place(H, &[Inst::MoviN(a(2), 1); 8]);
    img.place(
        S,
        &[Inst::MoviN(a(3), 1), Inst::NullaryN(NullaryNarrowOp::RetN)],
    );
    let extents = [Extent {
        start: S,
        end: S + 4,
    }];
    let spans = [(H, H + 0x100)];
    let bounds = Bounds {
        extents: &extents,
        spans: &spans,
    };
    let found = discover(&[H], bounds, usize::MAX, &mut img.fetch());
    assert_eq!(
        starts_of(&found),
        vec![H, S],
        "the hole falls into the symbol"
    );
    assert_eq!(found.set.blocks[0].insts.len(), 8);
    assert_eq!(found.set.blocks[0].end, BlockEnd::Fall(S));
    // Outside every span: nothing to run.
    let found = discover(&[0x4000_0000], bounds, usize::MAX, &mut img.fetch());
    assert!(found.set.is_empty());
    assert_eq!(found.stats.empty_starts, 1);
}
