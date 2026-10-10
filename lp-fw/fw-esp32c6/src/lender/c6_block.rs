//! `lp_lender::Block` over the C6's lend region.

use lp_lender::{Block, BlockClose, LoanId, LoanKind};

use super::borrower;

/// The block, measured through the esp-alloc fork's lend counters.
#[derive(Default)]
pub struct C6Block {
    open_used: u32,
    open_overflow: u32,
}

impl C6Block {
    /// The block's used bytes now.
    pub fn used(&self) -> u32 {
        esp_alloc::HEAP.lend_used_free().0 as u32
    }
}

impl Block for C6Block {
    fn capacity(&self) -> u32 {
        let (used, free) = esp_alloc::HEAP.lend_used_free();
        (used + free) as u32
    }

    fn free(&self) -> u32 {
        esp_alloc::HEAP.lend_used_free().1 as u32
    }

    fn largest_free(&self) -> u32 {
        esp_alloc::HEAP.lend_largest_free() as u32
    }

    fn open(&mut self, _loan: LoanId, _kind: LoanKind) {
        self.open_used = self.used();
        self.open_overflow = esp_alloc::HEAP.lend_stats().overflow_bytes;
        esp_alloc::HEAP.lend_reset_peak();
        borrower::open();
    }

    fn close(&mut self, _loan: LoanId) -> BlockClose {
        borrower::close();
        let stats = esp_alloc::HEAP.lend_stats();
        let used = self.used();
        BlockClose {
            peak_in_block: stats.peak_used.saturating_sub(self.open_used),
            overflow: stats.overflow_bytes.saturating_sub(self.open_overflow),
            // Net: what the loan left in the block less what it freed there
            // (a compile frees the program the last compile left).
            survivors: used.saturating_sub(self.open_used),
            largest_free_after: self.largest_free(),
        }
    }
}
