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
    /// The walk could not decode the word at `pc`, so the block ends before it.
    /// The universal escape (JD7); [`Unknown`] says how translated code gets
    /// past it.
    Undecodable(u32, Unknown),
}

/// How translated code gets past a word [`crate::decode::decode`] refused.
///
/// Refusing to decode is never wrong (JD7), but it used to be expensive: the
/// stay ended and the interpreter was re-entered for one instruction. M7b P6
/// hands most of them to [`crate::host::HostOps::step_one`] instead — the
/// interpreter's own `step_once`, at the same pc, with the register file and
/// the counters flushed — and then resolves the pc it reports through the
/// indirect-target table, exactly as a `jalr` does. What `step_one` cannot be
/// asked for is named here rather than left to the emitter to work out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unknown {
    /// `step_one` may run it and the stay may carry on at the pc it reports.
    Step,
    /// The stay must **end** before it and let the hart run it out in its own
    /// loop.
    ///
    /// One encoding needs this and it is `fence.i`: it is the guest publishing
    /// code, and the flush it asks for lands on a block cache and a translated
    /// core that are **lifted out of the hart** for the length of a stay
    /// (`mach/mod.rs::run_slice_cached`). The hart drains both immediately
    /// after the uncacheable arm runs one; a stay would carry on running the
    /// bytes it was translated from until it happened to end. The whole
    /// `MISC-MEM` opcode takes this answer rather than the one encoding,
    /// because leaving is always exact and the class costs one exit a run.
    Leave,
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

    /// The same blocks in a different **order**, with the index rebuilt to
    /// match.
    ///
    /// A global block index is a position in [`BlockSet::blocks`] and nothing
    /// else: the emitter chunks that vector, the selector divides by the chunk
    /// size, an edge looks its target up in [`BlockSet::index`], and the
    /// indirect page map is written from the same enumeration. So permuting
    /// the vector and rebuilding the index permutes **every** one of them
    /// together, and the guest cannot tell: no transcript, no counter and no
    /// exit pc is a function of where a block sits.
    ///
    /// `order[j]` is the block that should end up at position `j`. It must be
    /// a permutation of `0..blocks.len()`; anything else is a caller's bug.
    ///
    /// # Panics
    ///
    /// Panics when `order` is not a permutation of this set's positions.
    #[must_use]
    pub fn permuted(&self, order: &[usize]) -> Self {
        assert_eq!(
            order.len(),
            self.blocks.len(),
            "a layout order names every block exactly once"
        );
        let mut seen = alloc::vec![false; self.blocks.len()];
        for &from in order {
            assert!(
                !core::mem::replace(&mut seen[from], true),
                "a layout order names block {from} twice"
            );
        }
        let blocks: Vec<Block> = order
            .iter()
            .map(|&from| self.blocks[from].clone())
            .collect();
        let index = blocks
            .iter()
            .enumerate()
            .map(|(i, b)| (b.pc, i))
            .collect::<BTreeMap<_, _>>();
        // `entries` is the set of pcs a hart may enter at. It is a SET: the
        // order it is in is not part of what it means, and the hart sorts its
        // own table anyway. Kept as the blocks now stand so that two sets that
        // differ only in layout have the same `entries` contents.
        let entries = blocks.iter().map(|b| b.pc).collect();
        Self {
            blocks,
            index,
            entries,
        }
    }
}

/// How the emitter orders the block set before it chunks it into functions.
///
/// The chunks are contiguous runs of this vector, so the order is what decides
/// **which blocks share a wasm function** — and therefore which guest edges
/// are a branch inside one body and which are a cross through the selector.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum BlockOrder {
    /// Guest address order, which is what [`crate::discover`] produces.
    #[default]
    Address,
    /// A depth-first trace layout: each block followed by the successor it
    /// most often runs into, and a call's body placed next to its call site.
    /// See [`adjacency_order`].
    Adjacency,
}

/// A block's successors, most-likely first, as guest pcs.
///
/// Derived from the same three things the emitter emits from — the block end,
/// and for a terminator the last instruction — so a successor here is an edge
/// the emitted code really has. An indirect jump has no static successor and
/// contributes none.
fn successors(b: &Block) -> impl Iterator<Item = u32> + use<'_> {
    let mut out = [None; 2];
    match b.end {
        BlockEnd::Fall(pc) => out[0] = Some(pc),
        BlockEnd::Undecodable(..) => {}
        BlockEnd::Term => {
            if let Some(&(pc, d)) = b.insts.last() {
                match d.inst {
                    // The not-taken edge first: the emitter falls straight
                    // into the block laid out next, for free, and a taken
                    // branch is a `br` whatever the layout.
                    crate::decode::Inst::Branch { imm, .. } => {
                        out[0] = Some(pc.wrapping_add(u32::from(d.width)));
                        out[1] = Some(pc.wrapping_add(imm as u32));
                    }
                    crate::decode::Inst::Jal { rd, imm } => {
                        out[0] = Some(pc.wrapping_add(imm as u32) & !1);
                        // A linking `jal` is a call, so the instruction after
                        // it is where control comes back to. Laying the callee
                        // next and the return site after it is what puts a
                        // call and its body in one wasm function.
                        if rd != 0 {
                            out[1] = Some(pc.wrapping_add(u32::from(d.width)));
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    out.into_iter().flatten()
}

/// A trace layout of `set`: the order a walk of its own edges lays it out in.
///
/// Address order is what a walk produces and it is not what a run executes.
/// This follows each block into the successor it most often runs into —
/// fall-through, then the taken branch, then a call's own body — and only
/// starts a new trace when the current one runs into a block already placed.
/// Unplaced blocks are picked up afterwards in address order, so the result is
/// a permutation of the whole set and is **deterministic**: the same block set
/// lays out the same way on every host and in every engine, which is what lets
/// it be an identity oracle rather than a source of drift.
///
/// It is a layout and nothing else. Every block in, every block out, the same
/// bytes translated the same way — see [`BlockSet::permuted`] for why the
/// guest cannot tell.
#[must_use]
pub fn adjacency_order(set: &BlockSet) -> Vec<usize> {
    let n = set.blocks.len();
    let mut placed = alloc::vec![false; n];
    let mut out = Vec::with_capacity(n);
    // Seeds waiting for a trace of their own, most recent first, so a callee
    // is laid out immediately after the trace that called it.
    let mut pending: Vec<usize> = Vec::new();
    for seed in 0..n {
        if placed[seed] {
            continue;
        }
        pending.push(seed);
        while let Some(start) = pending.pop() {
            let mut at = start;
            loop {
                if placed[at] {
                    break;
                }
                placed[at] = true;
                out.push(at);
                let mut next = None;
                // Walked in reverse so that the *first* successor is the one
                // the trace continues into and the rest are picked up in
                // order after it.
                for pc in successors(&set.blocks[at])
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                {
                    let Some(&j) = set.index.get(&pc) else {
                        continue;
                    };
                    if placed[j] {
                        continue;
                    }
                    if let Some(prev) = next.replace(j) {
                        pending.push(prev);
                    }
                }
                match next {
                    Some(j) => at = j,
                    None => break,
                }
            }
        }
    }
    debug_assert_eq!(
        out.len(),
        n,
        "a layout order names every block exactly once"
    );
    out
}
