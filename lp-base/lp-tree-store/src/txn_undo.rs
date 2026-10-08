//! A transaction's undo log for the path table: the first prior row of each
//! path hash the transaction touched (≈ 24 B per changed path), so `abort`
//! (or a failed per-call write) puts the table back exactly.

use alloc::vec::Vec;
use core::mem::size_of;

use crate::path_table::{PathSlot, PathTable};

#[derive(Default)]
pub struct TxnUndo {
    rows: Vec<(u64, PathSlot)>,
}

impl TxnUndo {
    /// Remember `slot` as `hash`'s prior row unless one is remembered.
    pub fn save(&mut self, hash: u64, slot: PathSlot) {
        if let Err(i) = self.rows.binary_search_by(|r| r.0.cmp(&hash)) {
            self.rows.insert(i, (hash, slot));
        }
    }

    pub fn restore(&mut self, table: &mut PathTable) {
        for (h, slot) in core::mem::take(&mut self.rows) {
            table.set(h, slot);
        }
    }

    pub fn clear(&mut self) {
        self.rows = Vec::new();
    }

    pub fn ram_bytes(&self) -> usize {
        self.rows.capacity() * size_of::<(u64, PathSlot)>()
    }
}
