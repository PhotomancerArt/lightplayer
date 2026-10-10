//! Why a taker did not get the block.

use crate::LoanKind;

/// Why [`crate::Lender::try_lend`] refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// Another taker holds the block.
    Busy {
        /// Who holds it.
        holder: LoanKind,
    },
    /// The block is free, but kept for a higher-priority taker refused
    /// earlier (until it is served or its reservation expires).
    Reserved {
        /// Who it is kept for.
        for_kind: LoanKind,
    },
    /// The job could never fit: its largest ask is longer than the block, or
    /// its working set is more than the block and the overflow allowance
    /// together. A refusal in words, not a reset.
    TooBig {
        /// The job's largest single ask.
        largest: u32,
        /// The job's working set.
        total: u32,
        /// The block's size.
        capacity: u32,
    },
    /// The job fits the block, but not as it is now, even with every
    /// unpinned tenant purged: something the block cannot drop (a survivor
    /// of an earlier loan, a spilled long-lived allocation, a pinned tenant)
    /// is in the way.
    NoRoom {
        /// The block's longest free run when refused.
        largest_free: u32,
        /// The block's free bytes when refused.
        free: u32,
    },
}

impl Refusal {
    /// Whether waiting could help (the block is held, kept, or full of
    /// things that come and go), as opposed to a job too big for the block.
    pub fn is_transient(&self) -> bool {
        !matches!(self, Refusal::TooBig { .. })
    }
}
