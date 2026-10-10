//! A discardable stand-in: the device card's picture, cached in the block.
//!
//! E7 found the card's steady read rebuilding its geometry (the control
//! layout, 20 B a lamp, and the mapping points, 16 B a lamp) on every read
//! though nothing changed. The real cache does not exist yet; this stands
//! in for it with the same shape — three buffers, [`TENANT_BYTES`] in all,
//! sized as the card's geometry and samples at ~340 lamps, filled with a
//! pattern the checkout verifies — so the experiment can count how often a
//! cache living in the block is purged and rebuilt under the real
//! sequence of reads and compiles.

extern crate alloc;

use alloc::vec::Vec;

use lp_lender::Tenant;

/// The stand-in's size.
pub const TENANT_BYTES: usize = 12 * 1024;
const PIECES: usize = 3;

/// The card-picture stand-in.
#[derive(Default)]
pub struct CardTenant {
    pieces: Option<Vec<Vec<u8>>>,
    pinned: bool,
    generation: u8,
    /// Checkouts that found it resident.
    pub hits: u32,
    /// Checkouts that found it purged (or never built).
    pub misses: u32,
    /// Rebuilds made.
    pub rebuilds: u32,
    /// Rebuilds the lender refused (the block had no room).
    pub rebuilds_refused: u32,
    /// Rebuilds whose bytes did not all land in the block.
    pub rebuilds_outside: u32,
    /// Checkouts whose pattern was wrong (must stay 0).
    pub corrupt: u32,
    /// Purges suffered.
    pub purged: u32,
}

impl CardTenant {
    /// "Am I still here?" — the fallible checkout. Verifies the pattern.
    pub fn checkout(&mut self) -> bool {
        let generation = self.generation;
        match &self.pieces {
            Some(pieces) => {
                self.pinned = true;
                let intact = pieces
                    .iter()
                    .all(|piece| piece.iter().all(|byte| *byte == generation));
                self.pinned = false;
                if !intact {
                    self.corrupt += 1;
                }
                self.hits += 1;
                true
            }
            None => {
                self.misses += 1;
                false
            }
        }
    }

    /// Rebuild, while the block is lent to this rebuild (so every piece
    /// lands in the block).
    pub fn rebuild(&mut self) {
        self.generation = self.generation.wrapping_add(1).max(1);
        let generation = self.generation;
        let piece = TENANT_BYTES / PIECES;
        let mut pieces = Vec::with_capacity(PIECES);
        for _ in 0..PIECES {
            pieces.push(alloc::vec![generation; piece]);
        }
        self.pieces = Some(pieces);
        self.rebuilds += 1;
    }

    /// Whether every piece sits inside `region` (`(start, size)`).
    pub fn inside(&self, region: (usize, usize)) -> bool {
        let (start, size) = region;
        self.pieces.as_ref().is_some_and(|pieces| {
            pieces.iter().all(|piece| {
                let at = piece.as_ptr() as usize;
                at >= start && at + piece.len() <= start + size
            })
        })
    }
}

impl Tenant for CardTenant {
    fn resident(&self) -> u32 {
        self.pieces
            .as_ref()
            .map_or(0, |pieces| pieces.iter().map(|p| p.len() as u32).sum())
    }

    fn pinned(&self) -> bool {
        self.pinned
    }

    fn purge(&mut self) -> u32 {
        let freed = self.resident();
        if self.pieces.take().is_some() {
            self.purged += 1;
        }
        freed
    }
}
