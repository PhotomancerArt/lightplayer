//! What a taker asks the block for.

use crate::LoanKind;

/// One job's demand on the block, estimated by the job itself.
///
/// The lender does not estimate: a read's figures come from
/// `lpa_server::read_cost::ReadCost` (E7's per-request estimate), a
/// whole-file read's from the file's size, the update's from its fixed
/// window. Where a job has no estimate (the compile, today), it asks for the
/// whole block.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ask {
    /// Who is asking.
    pub kind: LoanKind,
    /// The largest single allocation the job makes: the block must hold one
    /// free run at least this long.
    pub largest: u32,
    /// The job's working set: everything it holds at once. What does not fit
    /// in the block overflows to the general heap (the lender's
    /// `overflow_allowance` bounds how much it may count on there).
    pub total: u32,
}

impl Ask {
    /// An ask for `largest` in one run and `total` in all.
    pub const fn new(kind: LoanKind, largest: u32, total: u32) -> Self {
        Self {
            kind,
            largest,
            total,
        }
    }

    /// An ask for the whole block (a job with no estimate of its own).
    pub const fn whole(kind: LoanKind, capacity: u32) -> Self {
        Self::new(kind, capacity, capacity)
    }
}
