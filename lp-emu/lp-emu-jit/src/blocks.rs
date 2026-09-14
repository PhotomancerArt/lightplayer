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

use lp_emu_core::InstClass;

use crate::decode::Decoded;

/// What this crate needs to know about one decoded guest instruction,
/// whatever the architecture.
///
/// Three questions and no more, and each is asked in exactly one place: the
/// width is how a block's byte span is computed, the class is what the budget
/// check charges, and `is_control` is what ends a block. Everything else about
/// an instruction — its operands, what it does, whether the emitter has an arm
/// for it — belongs to the emitter that owns the type, not here (M7 XD6/XD7:
/// wasm is the IR and the two architectures share the ABI, not a decoded
/// form).
pub trait DecodedInst: Copy {
    /// `log2` of how many guest bytes share one indirect-target slot.
    ///
    /// **1 on RV32**: RVC makes every block start two-byte aligned, and 48.99 %
    /// of real starts sit at 2 mod 4, so a four-byte table would miss half of
    /// them and a two-byte one misses none. **0 on Xtensa**: instructions are
    /// two or three bytes at any alignment and all four residues are live in
    /// equal measure (25.37 / 25.02 / 24.74 / 24.87 % over 721,066
    /// instructions of the firmware image), so the table is indexed by the byte
    /// — the same answer `lp_xt_emu::mach::translated::ENTRY_TABLE_BITS` gives
    /// for the hart's own entry table, for the same reason.
    ///
    /// It costs 4 B per `1 << SLOT_SHIFT` guest bytes on a page that holds a
    /// block start; see [`crate::dispatch::target_table_bytes`].
    const SLOT_SHIFT: u32;

    /// Bytes this instruction occupies.
    fn width(&self) -> u8;

    /// The cost class the budget check charges for it.
    fn class(&self) -> InstClass;

    /// Does it decide the next pc itself? A block ends after one of these.
    fn is_control(&self) -> bool;
}

impl DecodedInst for Decoded {
    const SLOT_SHIFT: u32 = 1;

    fn width(&self) -> u8 {
        self.width
    }

    fn class(&self) -> InstClass {
        self.class
    }

    fn is_control(&self) -> bool {
        Decoded::is_control(self)
    }
}

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
///
/// Generic over the decoded form since M7 XD6, and **defaulted to the RV32
/// one**, so every RV32 caller — this crate's own walk and emitter, and the
/// C6's driver — names `Block` exactly as it always did.
#[derive(Clone, Debug)]
pub struct Block<D = Decoded> {
    /// The guest pc of the first instruction.
    pub pc: u32,
    /// `(pc, decoded)` in execution order.
    pub insts: Vec<(u32, D)>,
    pub end: BlockEnd,
}

impl<D: DecodedInst> Block<D> {
    /// The guest pc just past the last instruction.
    #[must_use]
    pub fn end_pc(&self) -> u32 {
        match self.insts.last() {
            Some((pc, d)) => pc.wrapping_add(u32::from(d.width())),
            None => self.pc,
        }
    }
}

/// The blocks one wasm function holds, and how to find one by pc.
#[derive(Clone, Debug)]
pub struct BlockSet<D = Decoded> {
    /// Sorted by `pc`, and never empty once a walk found something to
    /// translate.
    pub blocks: Vec<Block<D>>,
    /// `pc` to index in [`BlockSet::blocks`].
    pub index: BTreeMap<u32, usize>,
    /// The pcs the hart may enter at — every block start.
    entries: Vec<u32>,
}

// Derived `Default` would demand `D: Default`, which a decoded instruction has
// no reason to be: an empty set holds none of them.
impl<D> Default for BlockSet<D> {
    fn default() -> Self {
        Self {
            blocks: Vec::new(),
            index: BTreeMap::new(),
            entries: Vec::new(),
        }
    }
}

impl<D> BlockSet<D> {
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
    pub fn from_blocks(blocks: Vec<Block<D>>, index: BTreeMap<u32, usize>) -> Self {
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
