//! Building blocks from a **supplied** list of starts.
//!
//! This is deliberately a stub. The real Xtensa sweep — symbol extents as the
//! primary bound, every `l32r` target struck out as data, `LEND` a terminator
//! with a static back-edge to `LBEG`, the call/`entry` seeds, and the third
//! path from the guest's own code-write spans — is XD9 and P05's whole job.
//!
//! What lives here is the other half of that job, which P05 will keep: given
//! the starts, follow decoded widths and cut blocks at the classification's
//! boundaries. It is the same shape M7 P3 stood up on the RV32 side before its
//! own sweep existed, and it is enough to install a module and prove the seam.
//!
//! Nothing here can be *wrong*, only short: a start that is not in the list is
//! a pc the interpreter runs, and a block that ends early is a stay that
//! exits early. That property is what makes a supplied list safe to ship
//! ahead of a sweep.

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::vec::Vec;

use crate::blocks::{Block, BlockEnd, BlockSet, MAX_BLOCK_INSTS};
use crate::decode::{Decode, Decoded};

/// What one walk found.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DiscoverStats {
    /// Starts the caller supplied.
    pub seeds: usize,
    /// Blocks built.
    pub blocks: usize,
    /// Instructions in them.
    pub insts: usize,
    /// Blocks that ended because the instruction after them was one the
    /// translator does not decode.
    pub undecodable: usize,
    /// Blocks that ended at [`MAX_BLOCK_INSTS`] rather than at a boundary.
    pub capped: usize,
    /// Seeds the fetch could not serve at all, so no block was built.
    pub unfetchable: usize,
}

/// A walk's result.
#[derive(Clone, Debug, Default)]
pub struct Discovered {
    pub set: BlockSet,
    pub stats: DiscoverStats,
}

/// Build one block per supplied start.
///
/// `fetch` reads one guest byte, purely: this runs before the guest reaches
/// any of these addresses, so a fetch that charged a cycle or fired a
/// watchpoint would be a change the guest can see. `None` ends the block
/// exactly as an undecodable instruction does.
///
/// A block ends **after** a terminator, **before** an instruction the
/// translator does not decode, at [`MAX_BLOCK_INSTS`], at the next supplied
/// start (so two blocks never overlap), and at an address that would wrap.
#[must_use]
pub fn build(starts: &[u32], fetch: &mut dyn FnMut(u32) -> Option<u8>) -> Discovered {
    let mut stats = DiscoverStats {
        seeds: starts.len(),
        ..DiscoverStats::default()
    };
    // Sorted and unique: `BlockSet::from_blocks` requires it, and the stop set
    // below needs to answer "is there a start here" for a *later* address.
    let unique: BTreeSet<u32> = starts.iter().copied().collect();
    let mut blocks: Vec<Block> = Vec::with_capacity(unique.len());
    let mut index = BTreeMap::new();

    for &pc in &unique {
        let mut insts: Vec<(u32, Decoded)> = Vec::new();
        let mut at = pc;
        let end = loop {
            if insts.len() >= MAX_BLOCK_INSTS {
                stats.capped += 1;
                break BlockEnd::Fall(at);
            }
            // Another block starts here, so this one falls into it rather than
            // decoding the same instruction twice.
            if at != pc && unique.contains(&at) {
                break BlockEnd::Fall(at);
            }
            let mut bytes = [0u8; 3];
            let mut got = 0usize;
            for (i, slot) in bytes.iter_mut().enumerate() {
                match at.checked_add(i as u32).and_then(&mut *fetch) {
                    Some(b) => {
                        *slot = b;
                        got = i + 1;
                    }
                    None => break,
                }
            }
            if got == 0 {
                stats.undecodable += 1;
                break BlockEnd::Undecodable(at);
            }
            match crate::decode::decode(&bytes[..got]) {
                Decode::Ok(d) => {
                    let Some(next) = at.checked_add(u32::from(d.width)) else {
                        insts.push((at, d));
                        break BlockEnd::Undecodable(at);
                    };
                    let control = d.control;
                    insts.push((at, d));
                    if control {
                        break BlockEnd::Term;
                    }
                    at = next;
                }
                Decode::Undecodable { .. } | Decode::Refused { .. } => {
                    stats.undecodable += 1;
                    break BlockEnd::Undecodable(at);
                }
            }
        };
        // A start whose very first instruction could not be decoded has no
        // block: an empty one would be an entry the module cannot run.
        if insts.is_empty() {
            stats.unfetchable += 1;
            continue;
        }
        stats.insts += insts.len();
        index.insert(pc, blocks.len());
        blocks.push(Block { pc, insts, end });
    }
    stats.blocks = blocks.len();
    Discovered {
        set: BlockSet::from_blocks(blocks, index),
        stats,
    }
}
