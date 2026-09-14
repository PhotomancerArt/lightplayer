//! Rule 4: every word an `l32r` names is data and is never a block start.
//!
//! Literal pools sit inside `.text`, immediately before the function that
//! reads them (study §2.3), and no `.literal` section survives the link to say
//! so. The `l32r` targets the sweep decodes are the only pool map there is.

use lp_xt_inst::{Inst, NullaryNarrowOp, Reg};
use lp_xt_jit::blocks::BlockEnd;
use lp_xt_jit::discover::{Bounds, Extent, discover};

fn a(n: u8) -> Reg {
    Reg::new(n)
}

/// The raw `l32r` field naming the word at `literal` from an `l32r` at `pc`.
fn l32r_field(pc: u32, literal: u32) -> u16 {
    let base = (pc + 3) & !3;
    let words = (i64::from(literal) - i64::from(base)) / 4;
    assert!((-65536..0).contains(&words), "l32r reaches backwards only");
    (words + 65536) as u16
}

struct Image {
    runs: Vec<(u32, Vec<u8>)>,
}

impl Image {
    fn place(&mut self, at: u32, insts: &[Inst]) -> u32 {
        let mut bytes = Vec::new();
        for inst in insts {
            bytes.extend_from_slice(&lp_xt_inst::encode(inst));
        }
        let end = at + bytes.len() as u32;
        self.runs.push((at, bytes));
        end
    }

    fn word(&mut self, at: u32, w: u32) {
        self.runs.push((at, w.to_le_bytes().to_vec()));
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

/// A function with its pool before it: the walk from the symbol does not
/// enter the pool, and a seed landing in the pool is dropped and counted.
#[test]
fn a_seed_in_a_literal_pool_is_dropped_and_the_pool_is_never_entered() {
    let mut img = Image { runs: Vec::new() };
    const POOL: u32 = 0x400d_1000;
    const F: u32 = 0x400d_1010;
    // Two literals that happen to decode as instructions: `0x400d_2000` is
    // bytes 00 20 0d 40, and `0x0000_0000` is `ill`.
    img.word(POOL, 0x400d_2000);
    img.word(POOL + 4, 0x0000_0000);
    // F+0: entry a1, 32          (3)
    // F+3: l32r a2, POOL         (3)
    // F+6: l32r a3, POOL+4       (3)
    // F+9: retw.n                (2)
    let end = img.place(
        F,
        &[
            Inst::Entry(a(1), 32),
            Inst::L32r(a(2), l32r_field(F + 3, POOL)),
            Inst::L32r(a(3), l32r_field(F + 6, POOL + 4)),
            Inst::NullaryN(NullaryNarrowOp::RetwN),
        ],
    );
    let extents = [Extent { start: F, end }];
    let bounds = Bounds {
        extents: &extents,
        spans: &[],
    };
    // POOL is seeded too — a zero-sized label, say — and dropped.
    let found = discover(&[F, POOL], bounds, usize::MAX, &mut img.fetch());
    let starts: Vec<u32> = found.set.blocks.iter().map(|b| b.pc).collect();
    assert_eq!(starts, vec![F, F + 3]);
    assert_eq!(
        found.set.blocks[1].insts.len(),
        3,
        "two l32r and the retw.n"
    );
    assert_eq!(found.set.blocks[1].end, BlockEnd::Term);
    assert_eq!(found.stats.literals, 2);
    assert_eq!(found.stats.literal_starts_dropped, 1);
    assert!(!found.set.index.contains_key(&POOL));
    assert_eq!(found.stats.empty_starts, 0);
}

/// A block that walks onto a literal word ends before it: the word after a
/// function's last instruction can be the next function's pool.
#[test]
fn a_block_that_walks_onto_a_literal_ends_before_it() {
    let mut img = Image { runs: Vec::new() };
    const G: u32 = 0x400d_2000;
    const POOL: u32 = 0x400d_2004;
    const F: u32 = 0x400d_2010;
    // G: movi.n a2, 1 (2); nop.n (2) — then the pool, with no extent to say
    // where G ends (a symbol whose st_size the caller could not use).
    img.place(
        G,
        &[Inst::MoviN(a(2), 1), Inst::NullaryN(NullaryNarrowOp::NopN)],
    );
    img.word(POOL, 0x400d_3000);
    let f_end = img.place(
        F,
        &[
            Inst::L32r(a(2), l32r_field(F, POOL)),
            Inst::NullaryN(NullaryNarrowOp::RetN),
        ],
    );
    let extents = [Extent {
        start: F,
        end: f_end,
    }];
    let bounds = Bounds {
        extents: &extents,
        spans: &[],
    };
    // F first so its `l32r` has named the pool by the time G is walked; in
    // pass two the set is complete whatever the order.
    let found = discover(&[F, G], bounds, usize::MAX, &mut img.fetch());
    let g = &found.set.blocks[0];
    assert_eq!(g.pc, G);
    assert_eq!(g.insts.len(), 2);
    assert_eq!(g.end, BlockEnd::Undecodable(POOL));
    assert_eq!(found.stats.data_ends, 1);
    assert!(!found.set.index.contains_key(&POOL));
}
