//! Discovery: finding everything the program can run, before it runs it.
//!
//! P3 translated *a sample* of the image. This is the pass that replaces "a
//! supplied block set" with "everything the program can run" (M7 JD6), and it
//! is a **symbol-seeded, width-following sweep**:
//!
//! - **Seeds** are supplied by the caller and are, on this machine, the ELF
//!   entry point, the reset and trap vectors, and every function symbol in an
//!   executable region — of the *app* and of the *mask ROM*, because 3.5–11 %
//!   of the instructions these images retire are mask-ROM code and a sweep
//!   without it leaves that share to the interpreter by construction.
//! - **The sweep follows decoded instruction widths**, never a stride. This
//!   is the single most important sentence in the file: **48.99 % of real
//!   block starts sit at 2 mod 4** (the spike's census of 37,609 starts on
//!   `render-basic`), so a 4-byte scan desyncs on half the image and a 2-byte
//!   one manufactures instructions out of the second half of 32-bit
//!   encodings. A symbol is a known-good start; a decoded width is the only
//!   honest way to get to the next one.
//! - **Edges** are followed where they are static: a conditional branch names
//!   its target *and* its fall-through, a `jal` names its target. P1b measured
//!   51.3 % of block ends as statically followable this way (38.7 % not-taken
//!   branches, 12.6 % direct jumps). The other half are `jalr` / `c.jr`
//!   (25.7 %), which name nothing — their targets are reached because they
//!   land on function symbols, or on addresses some other edge already found.
//!
//! # Why a seed is a place to look and not a block to have
//!
//! Seeds are taken **one at a time and explored to exhaustion**, in the order
//! the caller gave them. P3 measured the alternative: queueing every seed
//! first spends the whole budget on seeds, follows no edge at all, and
//! produces 511 blocks of one instruction each. The order matters for the
//! same reason — with a bounded budget it decides what the budget is spent
//! on.
//!
//! # Nothing here can be wrong, only short (JD7)
//!
//! Every way the walk can fail ends a block rather than guessing:
//!
//! - a word [`crate::decode::decode`] does not recognise ends the block
//!   *before* it, as [`BlockEnd::Undecodable`], and the interpreter runs it;
//! - a fetch the caller cannot serve does the same;
//! - running into another block's start ends the block as [`BlockEnd::Fall`];
//! - a `Fall` or an edge to a pc that is not in the set is not an error — the
//!   emitted code exits to the interpreter at that pc.
//!
//! A walk over a whole image **will** meet data-in-text where the spike's
//! census never did: the census supplied real observed block starts, and a
//! symbol table does not. A jump table, a literal pool, a `.rodata` island
//! inside a `.text` span and a symbol that names data all decode into
//! nonsense or into nothing, and all of them cost coverage and none of them
//! can cost correctness. [`DiscoverStats::undecodable`] and
//! [`DiscoverStats::empty_starts`] are what make that visible rather than
//! silent.

extern crate alloc;

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::vec::Vec;

use crate::blocks::{Block, BlockEnd, BlockSet, MAX_BLOCK_INSTS};
use crate::decode::{Inst, decode};

/// What a walk found, and what it had to give up on.
///
/// Kept because a coverage shortfall has to be explainable: `undecodable`
/// and `empty_starts` are the walk's own account of the data it met in text,
/// and `truncated` says whether the number below is the image or the budget.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DiscoverStats {
    /// Seeds the caller supplied.
    pub seeds: usize,
    /// Distinct block starts the walk reached, before empty ones are dropped.
    pub starts: usize,
    /// Blocks with at least one instruction — what was translated.
    pub blocks: usize,
    /// Instructions across those blocks.
    pub insts: usize,
    /// Blocks that ended because the next word did not decode, or could not
    /// be fetched. Data-in-text, an unsupported extension, or a `SYSTEM`
    /// instruction — all of them the interpreter's (JD7).
    pub undecodable: usize,
    /// Starts whose *first* word did not decode, so the block held nothing to
    /// run and was dropped. A symbol that names data lands here.
    pub empty_starts: usize,
    /// The budget stopped the walk before the image did.
    pub truncated: bool,
}

/// The result of one walk.
#[derive(Clone, Debug, Default)]
pub struct Discovered {
    pub set: BlockSet,
    pub stats: DiscoverStats,
}

/// Walk the image from `seeds`, following instruction widths and static
/// edges.
///
/// `fetch` reads the guest word at an address and returns `None` for an
/// address it cannot serve — which ends a block exactly as an undecodable
/// word does. It must be **pure**: this runs before the guest reaches any of
/// these addresses, so a fetch that charged a cycle or fired a watchpoint
/// would be a change the guest can see.
///
/// `max_blocks` bounds the walk. It is a *host* bound, not a design one:
/// every wasm engine refuses a large enough function body and they refuse at
/// wildly different sizes, so the caller finds the largest set its host will
/// build. [`DiscoverStats::truncated`] says whether it bound.
#[must_use]
pub fn discover(
    seeds: &[u32],
    max_blocks: usize,
    fetch: &mut dyn FnMut(u32) -> Option<u32>,
) -> Discovered {
    let mut stats = DiscoverStats {
        seeds: seeds.len(),
        ..DiscoverStats::default()
    };
    let starts = find_starts(seeds, max_blocks, fetch, &mut stats);
    let set = build(&starts, fetch, &mut stats);
    Discovered { set, stats }
}

/// How long an instruction is, from its low bits alone.
///
/// RISC-V encodes length in the low bits and nowhere else: `bb != 11` is a
/// 16-bit instruction, `bbb11` with `bbb != 111` a 32-bit one. Both are true
/// of encodings this translator refuses to *decode*, which is exactly why
/// this exists — the walk has to step over an `ecall` without pretending to
/// understand it. No RV32IMC encoding is longer than 32 bits, so the ≥48-bit
/// forms are not modelled; a word that claims one is stepped over as four
/// bytes, which is a walk that finds nothing rather than a walk that is
/// wrong.
fn encoded_width(word: u32) -> u32 {
    if word & 0b11 == 0b11 { 4 } else { 2 }
}

/// How many words in a row the walk will step over without decoding one.
///
/// See the call site: this is what separates "an instruction the translator
/// refuses" from "data", with no way to tell them apart except how many of
/// them there are in a row.
const MAX_SKIP: usize = 4;

/// Pass one: where do blocks start?
///
/// Every static edge a branch or a `jal` names is a start, and so is a
/// branch's fall-through — and so is the instruction after a run that hit
/// [`MAX_BLOCK_INSTS`], because otherwise pass two would end that block
/// falling into an address the module has no label for.
fn find_starts(
    seeds: &[u32],
    max_blocks: usize,
    fetch: &mut dyn FnMut(u32) -> Option<u32>,
    stats: &mut DiscoverStats,
) -> BTreeSet<u32> {
    let mut starts: BTreeSet<u32> = BTreeSet::new();
    let mut queue: Vec<u32> = Vec::new();
    let mut walked: BTreeSet<u32> = BTreeSet::new();
    let mut truncated = false;

    // A seed is a place to start looking, not a block to have: each is
    // explored to exhaustion before the next is even added.
    let mut seeds = seeds.iter().copied();
    loop {
        let Some(pc) = queue.pop().or_else(|| {
            seeds.find(|&pc| {
                if starts.len() >= max_blocks {
                    truncated = true;
                    return false;
                }
                starts.insert(pc)
            })
        }) else {
            break;
        };
        if !walked.insert(pc) {
            continue;
        }
        let mut at = pc;
        let mut edge = |target: u32, starts: &mut BTreeSet<u32>, queue: &mut Vec<u32>| {
            if starts.len() >= max_blocks {
                truncated = true;
                return;
            }
            if starts.insert(target) {
                queue.push(target);
            }
        };
        let mut room = MAX_BLOCK_INSTS;
        let mut skipped = 0usize;
        loop {
            let Some(word) = fetch(at) else { break };
            let Some(d) = decode(word) else {
                // The word ends this block (JD7) — but it does **not** end
                // the walk. An encoding this translator refuses is still an
                // instruction the interpreter runs, and RISC-V puts its
                // length in the low bits of the word whatever the rest of it
                // means, so the address after it is a real block start and
                // is where translated code will be re-entered.
                //
                // This is worth 26 points of coverage on `render-basic`:
                // every `csrr`, `ecall`, `mret`, `wfi`, `fence.i` and atomic
                // in the middle of a function would otherwise hide the whole
                // rest of that function from the walk, and functions that
                // read a CSR early are exactly the hot ones — the scheduler,
                // the interrupt path and the cycle-counter reads.
                //
                // On real data-in-text the length bits mean nothing, and the
                // step is then a guess. It is a *safe* guess — the hart
                // enters translated code only at its own pc, so a block
                // nothing jumps to is dead weight and can never run — but an
                // unbounded one walks the whole of RAM: zeroed memory does
                // not decode, so every two bytes of it becomes another start.
                // Measured on a bare machine: 259,843 starts, of which
                // 259,723 held nothing, and 420 ms of walk.
                //
                // So the step is bounded. A refused *instruction* comes in
                // ones and twos among decodable ones — a `csrr`, an `ecall`,
                // an atomic — and a run longer than that is data, which the
                // walk stops at rather than marching through.
                if skipped == MAX_SKIP {
                    break;
                }
                skipped += 1;
                let next = at.wrapping_add(encoded_width(word));
                edge(next, &mut starts, &mut queue);
                // Walked here rather than queued-and-restarted, so the count
                // above is a bound on the whole run and not on each step of
                // it. Marking it walked is what stops the queue re-entering
                // the same march with the counter reset.
                walked.insert(next);
                at = next;
                continue;
            };
            skipped = 0;
            let next = at.wrapping_add(u32::from(d.width));
            match d.inst {
                Inst::Branch { imm, .. } => {
                    edge(at.wrapping_add(imm as u32), &mut starts, &mut queue);
                    edge(next, &mut starts, &mut queue);
                    break;
                }
                Inst::Jal { rd, imm } => {
                    edge(at.wrapping_add(imm as u32) & !1, &mut starts, &mut queue);
                    // A **call** names a second static edge, and it is the
                    // one a walk is likeliest to forget: the address it
                    // returns to. `rd != 0` is what makes `jal` a call rather
                    // than a jump, and the link register is a promise that
                    // control comes back here.
                    if rd != 0 {
                        edge(next, &mut starts, &mut queue);
                    }
                    break;
                }
                // An indirect jump names no *target* the walk can follow —
                // that is the 25.7 % of block ends symbols are the seeds for.
                // But an indirect **call** still names its return address,
                // and on these images that is where a quarter of the run
                // lives: every `auipc ra` / `jalr ra` pair is a call, and
                // without this the whole remainder of the calling function is
                // invisible to the walk. Measured on `render-basic`: 26
                // points of coverage.
                Inst::Jalr { rd, .. } => {
                    if rd != 0 {
                        edge(next, &mut starts, &mut queue);
                    }
                    break;
                }
                _ => {}
            }
            room -= 1;
            if room == 0 {
                // The block is about to be cut by its length rather than by
                // a control transfer, so where it is cut is a start too —
                // otherwise pass two ends it falling into an address the
                // module has no label for, and the stay ends one block early
                // every time round.
                edge(next, &mut starts, &mut queue);
                break;
            }
            at = next;
        }
    }

    stats.starts = starts.len();
    stats.truncated = truncated;
    starts
}

/// Pass two: build each block, stopping at the next known start.
fn build(
    starts: &BTreeSet<u32>,
    fetch: &mut dyn FnMut(u32) -> Option<u32>,
    stats: &mut DiscoverStats,
) -> BlockSet {
    let mut blocks = Vec::with_capacity(starts.len());
    for &pc in starts {
        let mut insts = Vec::new();
        let mut at = pc;
        let end = loop {
            if insts.len() == MAX_BLOCK_INSTS || (!insts.is_empty() && starts.contains(&at)) {
                break BlockEnd::Fall(at);
            }
            let Some(word) = fetch(at) else {
                break BlockEnd::Undecodable(at);
            };
            let Some(d) = decode(word) else {
                break BlockEnd::Undecodable(at);
            };
            insts.push((at, d));
            if d.is_control() {
                break BlockEnd::Term;
            }
            match at.checked_add(u32::from(d.width)) {
                Some(next) => at = next,
                // A block that would wrap the address space ends here.
                None => break BlockEnd::Fall(at),
            }
        };
        // A block whose very first word is undecodable holds nothing to run.
        // Emitting it would produce an entry that retires nothing and leans
        // on the hart's no-progress guard every time. A symbol that names
        // data is exactly this, and there is no way to tell the two apart
        // from the outside — which is why this is a counter and not a
        // complaint.
        if insts.is_empty() {
            stats.empty_starts += 1;
            continue;
        }
        if matches!(end, BlockEnd::Undecodable(_)) {
            stats.undecodable += 1;
        }
        blocks.push(Block { pc, insts, end });
    }

    stats.blocks = blocks.len();
    stats.insts = blocks.iter().map(|b| b.insts.len()).sum();
    let index = blocks
        .iter()
        .enumerate()
        .map(|(i, b)| (b.pc, i))
        .collect::<BTreeMap<_, _>>();
    BlockSet::from_blocks(blocks, index)
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    /// A tiny guest image at 0x1000: an `addi`, a backward `bne`, then a
    /// `jal` forward over a hole the decoder refuses.
    fn image() -> impl FnMut(u32) -> Option<u32> {
        // 0x1000: addi a0, a0, 1        0x00150513
        // 0x1004: bne  a0, a1, -4       0xfeb51ee3
        // 0x1008: jal  x0, +8           0x0080006f
        // 0x1010: ecall                 0x00000073  (the decoder refuses it)
        let words: [(u32, u32); 4] = [
            (0x1000, 0x0015_0513),
            (0x1004, 0xfeb5_1ee3),
            (0x1008, 0x0080_006f),
            (0x1010, 0x0000_0073),
        ];
        move |pc| words.iter().find(|(a, _)| *a == pc).map(|(_, w)| *w)
    }

    #[test]
    fn the_walk_follows_static_edges_and_splits_at_a_branch_target() {
        let found = discover(&[0x1000], 64, &mut image());
        let starts: Vec<u32> = found.set.blocks.iter().map(|b| b.pc).collect();
        // Four starts — 0x1000 (the seed and the branch's own target),
        // 0x1008 (the fall-through), 0x1010 (the `jal`'s target) and 0x1014
        // (past the `ecall` the walk steps over) — of which two hold
        // something to run. 0x1010 is an `ecall`, which the translator
        // refuses, and 0x1014 is off the end of the image; both blocks are
        // empty and dropped.
        assert_eq!(starts, vec![0x1000, 0x1008]);
        assert_eq!(found.set.entries(), starts);
        assert_eq!(found.stats.starts, 4);
        assert_eq!(found.stats.blocks, 2);
        assert_eq!(found.stats.empty_starts, 2);
        assert!(!found.stats.truncated);
    }

    #[test]
    fn a_block_stops_at_the_next_known_start() {
        let found = discover(&[0x1000], 64, &mut image());
        let first = &found.set.blocks[0];
        assert_eq!(first.insts.len(), 2, "the addi and the branch");
        assert_eq!(first.end, BlockEnd::Term);
    }

    #[test]
    fn an_undecodable_word_ends_a_block_rather_than_being_guessed() {
        let found = discover(&[0x1000], 64, &mut image());
        assert!(!found.set.index.contains_key(&0x1010));
        // A block that reaches an undecodable word after real instructions
        // keeps them and ends before it.
        let found = discover(&[0x1008], 64, &mut image());
        assert_eq!(found.set.blocks[0].pc, 0x1008);
        assert_eq!(found.set.blocks[0].end, BlockEnd::Term);
    }

    #[test]
    fn a_fetch_that_cannot_be_served_ends_the_block_too() {
        let mut nothing = |_pc: u32| None;
        assert!(discover(&[0x1000], 64, &mut nothing).set.is_empty());
    }

    #[test]
    fn the_budget_is_reported_when_it_binds() {
        let found = discover(&[0x1000], 1, &mut image());
        assert!(found.stats.truncated);
        assert_eq!(found.stats.starts, 1);
    }

    /// **48.99 % of real block starts sit at 2 mod 4.** A walk that swept on
    /// 4-byte boundaries would find the first of these two functions and
    /// desync on the second; one that follows decoded widths finds both.
    #[test]
    fn a_function_at_two_mod_four_is_found_and_does_not_desync() {
        // 0x1000: c.addi a0, 1     0x0505     (2 bytes)
        // 0x1002: addi   a1, a1, 2 0x00258593 (4 bytes, starting at 2 mod 4)
        // 0x1006: c.jr   ra        0x8082
        // 0x1008: c.li   a0, 3     0x4189  — the second function, at 8 mod 4 = 0
        // 0x100a: addi   a2, a2, 4 0x00460613
        // 0x100e: c.jr   ra        0x8082
        let halfwords: [(u32, u16); 6] = [
            (0x1000, 0x0505),
            (0x1002, 0x8593),
            (0x1004, 0x0025),
            (0x1006, 0x8082),
            (0x1008, 0x4189),
            (0x100a, 0x0613),
        ];
        let more: [(u32, u16); 2] = [(0x100c, 0x0046), (0x100e, 0x8082)];
        let mut fetch = move |pc: u32| {
            let at = |a: u32| {
                halfwords
                    .iter()
                    .chain(more.iter())
                    .find(|(x, _)| *x == a)
                    .map(|(_, h)| *h)
            };
            let lo = at(pc)?;
            let hi = at(pc.wrapping_add(2)).unwrap_or(0);
            Some(u32::from(lo) | (u32::from(hi) << 16))
        };
        // Seeded at both function symbols, which is what a symbol table gives.
        let found = discover(&[0x1000, 0x1008], 64, &mut fetch);
        let starts: Vec<u32> = found.set.blocks.iter().map(|b| b.pc).collect();
        assert_eq!(starts, vec![0x1000, 0x1008]);
        // Three instructions each: the walk read the 4-byte `addi` at 2 mod 4
        // as one instruction rather than as two halves.
        assert_eq!(found.set.blocks[0].insts.len(), 3);
        assert_eq!(found.set.blocks[1].insts.len(), 3);
        assert_eq!(found.set.blocks[0].insts[1].0, 0x1002);
        assert_eq!(found.set.blocks[0].insts[2].0, 0x1006);
    }

    /// Data in a `.text` span degrades to `Undecodable` rather than being
    /// mistranslated, and the blocks either side of it are still found.
    #[test]
    fn data_in_text_degrades_and_is_counted() {
        // 0x2000: addi a0, a0, 1   0x00150513
        // 0x2004: 0xffff_ffff      — a literal the decoder refuses
        // 0x2008: c.jr ra          0x8082   (a second symbol, past the pool)
        let words: [(u32, u32); 3] = [
            (0x2000, 0x0015_0513),
            (0x2004, 0xffff_ffff),
            (0x2008, 0x0000_8082),
        ];
        let mut fetch = move |pc: u32| words.iter().find(|(a, _)| *a == pc).map(|(_, w)| *w);
        let found = discover(&[0x2000, 0x2008], 64, &mut fetch);
        assert_eq!(found.set.blocks.len(), 2);
        assert_eq!(found.set.blocks[0].pc, 0x2000);
        assert_eq!(
            found.set.blocks[0].end,
            BlockEnd::Undecodable(0x2004),
            "the block ends before the literal and exits to the interpreter"
        );
        assert_eq!(found.stats.undecodable, 1);
        assert!(
            !found.set.index.contains_key(&0x2004),
            "the literal pool is not a block"
        );
    }
}
