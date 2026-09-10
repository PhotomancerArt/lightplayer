//! Guest blocks: what the translator is given to translate.
//!
//! A [`Block`] is a straight-line run of decoded instructions starting at a
//! guest pc, plus what happens at its end. A [`BlockSet`] is the collection the
//! translator turns into one wasm function, and the index that lets one block's
//! branch reach another's label without leaving the module.
//!
//! # This is the shape, not the search
//!
//! [`crate::discover`] is the search — the symbol-seeded, width-following
//! sweep over the whole image with the coverage measurement that goes with it
//! (JD6). What lives here is the *shape* it produces and the index the
//! translator reads.
//!
//! # Nothing here can be wrong, only short
//!
//! Every way a walk can fail ends a block rather than guessing (JD7):
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

use alloc::collections::BTreeMap;
use alloc::vec::Vec;

use crate::decode::Decoded;

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
    /// Sorted by `pc`, and never empty once a walk found something to
    /// translate.
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

    /// Assemble a set from blocks already found, plus their pc index.
    ///
    /// The only constructor, and it is deliberately not a search: finding
    /// blocks is [`crate::discover`]'s job, and keeping the two apart is what
    /// lets the translator be tested on a hand-written set.
    ///
    /// `blocks` must be sorted by pc — the emitter lays bodies out in that
    /// order and a fall-through into the block laid out next costs nothing at
    /// all — and `index` must map each block's pc to its position.
    #[must_use]
    pub fn from_blocks(blocks: Vec<Block>, index: BTreeMap<u32, usize>) -> Self {
        debug_assert!(
            blocks.windows(2).all(|w| w[0].pc < w[1].pc),
            "a block set is sorted by pc and holds each pc once"
        );
        let entries = blocks.iter().map(|b| b.pc).collect();
        Self {
            blocks,
            index,
            entries,
        }
    }
}
