//! Rule 8, the third path: the guest's own code, found from the spans it
//! stored into executable memory.
//!
//! A constructed span: a RAM region the guest "wrote" a function into with
//! aligned word stores. The seeds are every word-aligned address in the span
//! that decodes; the walk from them, bounded by the span, finds the function
//! — and, with the installed starts as the stop set, claims nothing twice.

use std::collections::BTreeSet;

use lp_xt_inst::{Inst, NullaryNarrowOp, Reg};
use lp_xt_jit::blocks::BlockEnd;
use lp_xt_jit::discover::{Bounds, discover_from, word_seeds};

fn a(n: u8) -> Reg {
    Reg::new(n)
}

const REGION: u32 = 0x4008_8000;
const LEN: u32 = 0x100;
const FN_AT: u32 = REGION + 0x10;

/// The region as bytes: zero, with the function stored at `FN_AT`.
fn region() -> Vec<u8> {
    let mut ram = vec![0u8; LEN as usize];
    let mut at = (FN_AT - REGION) as usize;
    // entry a1, 32 (3); movi.n a2, 7 (2); retw.n (2) — 7 bytes, two words.
    for inst in [
        Inst::Entry(a(1), 32),
        Inst::MoviN(a(2), 7),
        Inst::NullaryN(NullaryNarrowOp::RetwN),
    ] {
        let bytes = lp_xt_inst::encode(&inst);
        ram[at..at + bytes.len()].copy_from_slice(&bytes);
        at += bytes.len();
    }
    ram
}

#[test]
fn the_walk_from_a_written_span_finds_the_function() {
    let ram = region();
    let mut fetch = |pc: u32| {
        pc.checked_sub(REGION)
            .and_then(|o| ram.get(o as usize))
            .copied()
    };
    // The dirty span the bus recorded: the two words the stores covered.
    let spans = [(FN_AT, FN_AT + 8)];
    let seeds = word_seeds(&spans, &|w| w, &mut fetch);
    assert!(seeds.contains(&FN_AT), "the function's first word decodes");
    assert!(
        seeds.iter().all(|s| s % 4 == 0),
        "word-aligned only: the region is written by aligned word stores"
    );
    let bounds = Bounds {
        extents: &[],
        spans: &spans,
    };
    let found = discover_from(&seeds, bounds, usize::MAX, &BTreeSet::new(), &mut fetch);
    let b = found
        .set
        .index
        .get(&FN_AT)
        .map(|&i| &found.set.blocks[i])
        .expect("a block at the function");
    assert_eq!(b.insts.len(), 1, "entry is its own block");
    assert_eq!(b.end, BlockEnd::Term);
    let body = found
        .set
        .index
        .get(&(FN_AT + 3))
        .map(|&i| &found.set.blocks[i])
        .expect("the block after entry");
    assert_eq!(body.insts.len(), 2, "movi.n + retw.n");
    assert_eq!(body.end, BlockEnd::Term);
    // Nothing outside the span was claimed.
    assert!(
        found
            .set
            .blocks
            .iter()
            .all(|b| b.pc >= FN_AT && b.pc < FN_AT + 8)
    );
}

#[test]
fn installed_starts_are_not_claimed_a_second_time() {
    let ram = region();
    let mut fetch = |pc: u32| {
        pc.checked_sub(REGION)
            .and_then(|o| ram.get(o as usize))
            .copied()
    };
    let spans = [(FN_AT, FN_AT + 8)];
    let seeds = word_seeds(&spans, &|w| w, &mut fetch);
    let bounds = Bounds {
        extents: &[],
        spans: &spans,
    };
    let known: BTreeSet<u32> = [FN_AT, FN_AT + 3].into_iter().collect();
    let again = discover_from(&seeds, bounds, usize::MAX, &known, &mut fetch);
    assert!(!again.set.index.contains_key(&FN_AT));
    assert!(!again.set.index.contains_key(&(FN_AT + 3)));
}

/// The S3's shape (P09): the span is recorded at the **write** address and
/// the code executes at another; `exec_of` is the map, and the seeds are
/// execute addresses.
#[test]
fn exec_of_maps_write_addresses_to_execute_addresses() {
    let ram = region();
    const ALIAS: u32 = 0x6f_0000;
    let mut fetch = |pc: u32| {
        pc.checked_sub(REGION + ALIAS)
            .and_then(|o| ram.get(o as usize))
            .copied()
    };
    let write_spans = [(FN_AT, FN_AT + 8)];
    let seeds = word_seeds(&write_spans, &|w| w + ALIAS, &mut fetch);
    assert!(seeds.contains(&(FN_AT + ALIAS)));
    let exec_spans = [(FN_AT + ALIAS, FN_AT + ALIAS + 8)];
    let found = discover_from(
        &seeds,
        Bounds {
            extents: &[],
            spans: &exec_spans,
        },
        usize::MAX,
        &BTreeSet::new(),
        &mut fetch,
    );
    assert!(found.set.index.contains_key(&(FN_AT + ALIAS)));
}
