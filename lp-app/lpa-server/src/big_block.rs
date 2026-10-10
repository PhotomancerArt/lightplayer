//! RESEARCH (`research/ram-e11`, experiment E11 of
//! `lp2025/2026-10-09-1203-ram-research`; not for main as written): the
//! server's takers of the big block.
//!
//! With a [`BigBlockHook`] installed ([`crate::LpServer::set_big_block`]),
//! a project read and a whole-file read ask the embedder's lender
//! (`lp_lender::Lender`) for the block instead of passing the chip's fixed
//! read gate. A read's ask is E7's per-request estimate
//! ([`crate::read_cost::ReadCost`]); a whole-file read's is the file's size.
//! The loan is held until the reply has been written, and returned on every
//! path out.

extern crate alloc;

use alloc::format;
use alloc::string::String;

use lp_lender::{Ask, LoanKind, Refusal};

/// The embedder's lender, as two calls on the server's own thread.
#[derive(Clone, Copy, Debug)]
pub struct BigBlockHook {
    /// Ask for the block. `Ok` = lent, until [`BigBlockHook::release`].
    pub lend: fn(Ask) -> Result<(), Refusal>,
    /// Return the block lent by the last successful [`BigBlockHook::lend`].
    pub release: fn(),
}

/// The words a refused taker's reply carries: transient refusals say retry,
/// a job too big for the block says so.
pub fn refusal_message(kind: LoanKind, refusal: &Refusal) -> String {
    let what = match kind {
        LoanKind::WholeFile => "file read",
        _ => "read",
    };
    match refusal {
        Refusal::Busy { holder } => format!(
            "{what} refused: board memory busy (the big block is lent to a {}); retry shortly",
            holder.name()
        ),
        Refusal::Reserved { for_kind } => format!(
            "{what} refused: board memory busy (the big block is kept for a {}); retry shortly",
            for_kind.name()
        ),
        Refusal::NoRoom { largest_free, free } => format!(
            "{what} refused: board memory busy (the big block has {free} B free, \
             {largest_free} B in one run); retry shortly"
        ),
        Refusal::TooBig {
            largest,
            total,
            capacity,
        } => format!(
            "{what} refused: too big for this board ({total} B, {largest} B in one piece; \
             the big block is {capacity} B)"
        ),
    }
}
