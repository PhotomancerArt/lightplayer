//! The block itself, as the edge implements it.

use crate::{LoanId, LoanKind};

/// The big block, implemented by the edge (on the C6: an `esp-alloc`
/// region the general heap reaches last, and that the holder's allocations
/// reach first).
///
/// Every method is called synchronously from [`crate::Lender`] on the
/// caller's thread, at a safe point: never from inside the allocator.
pub trait Block {
    /// The block's size in bytes.
    fn capacity(&self) -> u32;
    /// Free bytes in the block now.
    fn free(&self) -> u32;
    /// The longest free run in the block now.
    fn largest_free(&self) -> u32;
    /// Start routing `loan`'s holder's allocations into the block.
    fn open(&mut self, loan: LoanId, kind: LoanKind);
    /// Stop routing them, and say what the loan did.
    fn close(&mut self, loan: LoanId) -> BlockClose;
}

/// What one loan did to the block, measured by the edge at its close.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BlockClose {
    /// The holder's peak bytes in the block during the loan.
    pub peak_in_block: u32,
    /// The holder's bytes that did not fit in the block and went to the
    /// general heap instead.
    pub overflow: u32,
    /// The holder's bytes still live in the block after the loan: what it
    /// keeps (a compiled program, a read's retained result). They pin the
    /// block until freed.
    pub survivors: u32,
    /// The block's longest free run after the close.
    pub largest_free_after: u32,
}
