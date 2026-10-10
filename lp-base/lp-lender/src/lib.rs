//! **lp-lender** — RESEARCH (experiment E11 of
//! `lp2025/2026-10-09-1203-ram-research`; branch `research/ram-e11`, never
//! for `main` as written): one component that owns the large contiguous
//! block and lends it to one taker at a time.
//!
//! The shape:
//!
//! - **One block, one holder.** A [`Lender`] answers [`Lender::try_lend`]
//!   with a [`Loan`] or a [`Refusal`]. While a loan is held every other
//!   taker is refused [`Refusal::Busy`]; a refused taker of higher priority
//!   than the holder leaves a [`Reservation`] so the next grant is its own.
//! - **Priority** is [`LoanKind`]'s order: over-the-air inflate, then the
//!   shader compile, then a whole-file read, then a project read.
//! - **Asks are the job's own estimate** ([`Ask`]): its largest single
//!   allocation and its working set. A read's comes from
//!   `lpa_server::read_cost` (E7); the lender never re-estimates.
//! - **Loans live inside a tick.** [`Lender::begin_tick`] is the safe point:
//!   a loan still held there from an earlier tick is a leak, and is counted
//!   (a multi-tick kind, OTA, is exempt).
//! - **Discardable tenants** ([`Tenant`]) live in the block while nobody has
//!   it. A grant that does not fit beside them purges them, cheapest first,
//!   only as many as it needs (the Linux shrinker's count-then-scan), never
//!   a pinned one; the holder learns it was purged at its next checkout and
//!   rebuilds (Chromium `DiscardableMemory::Lock`, Apple
//!   `beginContentAccess`).
//!
//! The lender is sans-IO: it never touches memory. The edge implements
//! [`Block`] (route the holder's allocations into the block, measure it)
//! and [`Tenant`] (count and drop a cache), and the lender calls them
//! synchronously, on the caller's thread, at a safe point.
//!
//! `no_std`, no `alloc`, no dependencies.

#![no_std]

mod ask;
mod block;
mod lender;
mod lender_stats;
mod loan;
mod loan_kind;
mod refusal;
mod tenant;

pub use ask::Ask;
pub use block::{Block, BlockClose};
pub use lender::{Lender, Reservation};
pub use lender_stats::{KindStats, LenderStats};
pub use loan::{Loan, LoanId};
pub use loan_kind::LoanKind;
pub use refusal::Refusal;
pub use tenant::Tenant;
