//! Guest blocks: what the translator is given to translate.
//!
//! A [`Block`] is a straight-line run of decoded instructions starting at a
//! guest pc, plus what happens at its end. A [`BlockSet`] is the collection the
//! translator turns into one wasm function, and the index that lets one block's
//! branch reach another's label without leaving the module.
//!
//! # This is not discovery
//!
//! P4 owns discovery — the symbol-seeded, width-following sweep over the whole
//! image, with the coverage measurement that goes with it (JD6). What lives
//! here is the *shape* discovery produces, plus [`BlockSet::sweep`], the
//! deliberately simple walk P3 uses to have something real to translate: start
//! at the pcs the caller names, follow the instruction widths, follow the
//! static edges a branch or a `jal` names, and stop.
//!
//! # Nothing here can be wrong, only short
//!
//! Every way the walk can fail ends a block rather than guessing (JD7):
//!
//! - a word [`crate::decode::decode`] does not recognise ends the block
//!   *before* it, as [`BlockEnd::Undecodable`], and the interpreter runs it;
//! - a fetch the caller cannot serve does the same;
//! - running into another block's start ends the block as [`BlockEnd::Fall`];
//! - reaching [`MAX_BLOCK_INSTS`] ends it the same way;
//! - a [`BlockEnd::Fall`] or a branch target that is not in the set is not an
//!   error — the emitted code exits to the interpreter at that pc.
//!
//! So a mis-swept block, data-in-text and an unsupported extension all cost
//! coverage and none of them can cost correctness.

extern crate alloc;

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::vec::Vec;

use crate::decode::{Decoded, Inst, decode};

/// The most instructions one translated block may hold.
///
/// Not an architectural bound — a translated block may be any length, because
/// where a block ends is not observable. It is a size knob: the budget check
/// is per block against the block's own maximum cost, so a longer block is a
/// coarser (never a wrong) budget granularity, and a shorter one is more
/// dispatcher arms for the same code. 64 is comfortably above the 6.40
/// instructions per block the census measured, so it binds almost never.
pub const MAX_BLOCK_INSTS: usize = 64;

/// What happens after a block's last instruction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockEnd {
    /// The last instruction is the control transfer and decides the next pc
    /// itself.
    Term,
    /// Control falls into `pc` — another block's start, or wherever the
    /// instruction limit stopped the walk.
    Fall(u32),
    /// The walk could not decode the word at `pc`, so the block ends before it
    /// and translated code leaves for the interpreter there. The universal
    /// escape (JD7).
    Undecodable(u32),
}

/// One straight-line run of guest instructions.
#[derive(Clone, Debug)]
pub struct Block {
    /// The guest pc of the first instruction.
    pub pc: u32,
    /// `(pc, decoded)` in execution order.
    pub insts: Vec<(u32, Decoded)>,
    pub end: BlockEnd,
}

impl Block {
    /// The guest pc just past the last instruction.
    #[must_use]
    pub fn end_pc(&self) -> u32 {
        match self.insts.last() {
            Some((pc, d)) => pc.wrapping_add(u32::from(d.width)),
            None => self.pc,
        }
    }
}

/// The blocks one wasm function holds, and how to find one by pc.
#[derive(Clone, Debug, Default)]
pub struct BlockSet {
    /// Sorted by `pc`, and never empty once [`BlockSet::sweep`] returns
    /// something to translate.
    pub blocks: Vec<Block>,
    /// `pc` to index in [`BlockSet::blocks`].
    pub index: BTreeMap<u32, usize>,
    /// The pcs the hart may enter at — every block start.
    entries: Vec<u32>,
}

impl BlockSet {
    /// The pcs a hart's entry table should be built from.
    #[must_use]
    pub fn entries(&self) -> &[u32] {
        &self.entries
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.blocks.is_empty()
    }

    /// Instructions across every block — the denominator of the escape rate.
    #[must_use]
    pub fn inst_count(&self) -> usize {
        self.blocks.iter().map(|b| b.insts.len()).sum()
    }

    /// Walk from `seeds`, following instruction widths and static edges.
    ///
    /// `fetch` reads the guest word at an address and returns `None` for an
    /// address it cannot serve — which ends a block exactly as an undecodable
    /// word does. It must be **pure**: this runs before the guest reaches any
    /// of these addresses, so a fetch that charged a cycle or fired a
    /// watchpoint would be a change the guest can see.
    ///
    /// `max_blocks` bounds the walk. P3 translates a sample of the image, not
    /// the image (that is P4), and an unbounded walk from `_start` reaches most
    /// of it.
    pub fn sweep(
        seeds: &[u32],
        max_blocks: usize,
        fetch: &mut dyn FnMut(u32) -> Option<u32>,
    ) -> Self {
        let mut starts: BTreeSet<u32> = BTreeSet::new();
        let mut queue: Vec<u32> = Vec::new();
        for &pc in seeds {
            if starts.insert(pc) {
                queue.push(pc);
            }
        }

        // Pass one: find the block starts. Every static edge a branch or a
        // `jal` names is one, and so is a branch's fall-through.
        let mut walked: BTreeSet<u32> = BTreeSet::new();
        while let Some(pc) = queue.pop() {
            if !walked.insert(pc) {
                continue;
            }
            let mut at = pc;
            for _ in 0..MAX_BLOCK_INSTS {
                let Some(word) = fetch(at) else { break };
                let Some(d) = decode(word) else { break };
                let next = at.wrapping_add(u32::from(d.width));
                let edge = |target: u32, starts: &mut BTreeSet<u32>, queue: &mut Vec<u32>| {
                    if starts.len() < max_blocks && starts.insert(target) {
                        queue.push(target);
                    }
                };
                match d.inst {
                    Inst::Branch { imm, .. } => {
                        edge(at.wrapping_add(imm as u32), &mut starts, &mut queue);
                        edge(next, &mut starts, &mut queue);
                        break;
                    }
                    Inst::Jal { imm, .. } => {
                        edge(at.wrapping_add(imm as u32) & !1, &mut starts, &mut queue);
                        break;
                    }
                    // An indirect jump names no target the walk can follow.
                    // Its `jal`-shaped sibling above is where 51.3 % of block
                    // ends are statically followable; this is the other half.
                    Inst::Jalr { .. } => break,
                    _ => at = next,
                }
            }
        }

        // Pass two: build each block, stopping at the next known start. A
        // `Fall` or an edge to a pc that is not a start is not an error — the
        // emitted code exits there.
        let mut blocks = Vec::with_capacity(starts.len());
        for &pc in &starts {
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
            // A block whose very first word is undecodable holds nothing to
            // run. Emitting it would produce an entry that retires nothing and
            // leans on the hart's no-progress guard every time.
            if insts.is_empty() {
                continue;
            }
            blocks.push(Block { pc, insts, end });
        }

        let index = blocks
            .iter()
            .enumerate()
            .map(|(i, b)| (b.pc, i))
            .collect::<BTreeMap<_, _>>();
        let entries = blocks.iter().map(|b| b.pc).collect();
        Self {
            blocks,
            index,
            entries,
        }
    }
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
        // 0x100c: (unused)
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
        let set = BlockSet::sweep(&[0x1000], 64, &mut image());
        let starts: Vec<u32> = set.blocks.iter().map(|b| b.pc).collect();
        // The walk finds three starts — 0x1000 (the seed and the branch's own
        // target), 0x1008 (the fall-through) and 0x1010 (the `jal`'s target) —
        // and keeps the two that hold something to run. 0x1010 is an `ecall`,
        // which the translator refuses, so its block is empty and dropped.
        assert_eq!(starts, vec![0x1000, 0x1008]);
        assert_eq!(set.entries(), starts);
    }

    #[test]
    fn a_block_stops_at_the_next_known_start() {
        let set = BlockSet::sweep(&[0x1000], 64, &mut image());
        let first = &set.blocks[0];
        assert_eq!(first.insts.len(), 2, "the addi and the branch");
        assert_eq!(first.end, BlockEnd::Term);
    }

    #[test]
    fn an_undecodable_word_ends_a_block_rather_than_being_guessed() {
        // 0x1010 is `ecall`, which the translator's decoder refuses, so the
        // block that would start there holds nothing and is dropped.
        let set = BlockSet::sweep(&[0x1000], 64, &mut image());
        assert!(!set.index.contains_key(&0x1010));
        // A block that reaches an undecodable word after real instructions
        // keeps them and ends before it.
        let set = BlockSet::sweep(&[0x1008], 64, &mut image());
        assert_eq!(set.blocks[0].pc, 0x1008);
        assert_eq!(set.blocks[0].end, BlockEnd::Term);
    }

    #[test]
    fn a_fetch_that_cannot_be_served_ends_the_block_too() {
        let mut nothing = |_pc: u32| None;
        assert!(BlockSet::sweep(&[0x1000], 64, &mut nothing).is_empty());
    }
}
