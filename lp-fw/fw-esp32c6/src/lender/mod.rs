//! RESEARCH (`research/ram-e11`, experiment E11 of
//! `lp2025/2026-10-09-1203-ram-research`; **never shipped**): one lender for
//! the big block, on the C6.
//!
//! - [`lend_region`]: the block is an `esp-alloc` region carved from the main
//!   heap ([`LEND_BYTES`]), registered fourth. The general heap reaches it
//!   last; the borrower's allocations reach it first (the fork's
//!   `lend-region`).
//! - [`borrower`]: who the borrower is — the embassy task that was being
//!   polled when the loan opened, on the thread that polled it. Read from
//!   embassy-executor's `trace` hooks, so a Bluetooth task polled on the
//!   same executor while a read awaits its send is not the borrower.
//! - [`c6_block`]: `lp_lender::Block` over the region.
//! - [`card_tenant`]: a discardable stand-in living in the block while it is
//!   idle, checked out after every project read.
//! - [`lender_edge`]: the one `lp_lender::Lender`, the server's
//!   `BigBlockHook`, the compile's lp-perf marker hook and the heartbeat's
//!   `[e11]` line.
//! - [`link_standin`]: with `e11_link_standin`, a link-shaped allocation
//!   from a task of its own, mid-project.

#[cfg(feature = "e11_lender")]
pub mod borrower;
#[cfg(feature = "e11_lender")]
pub mod c6_block;
#[cfg(feature = "e11_lender")]
pub mod card_tenant;
#[cfg(feature = "e11_lender")]
pub mod lend_region;
#[cfg(feature = "e11_lender")]
pub mod lender_edge;
#[cfg(feature = "e11_link_standin")]
pub mod link_standin;

/// The block, `(start, size)`; `(0, 0)` in an image without a lender.
#[allow(dead_code, reason = "read by the link stand-in")]
pub fn block_region() -> (usize, usize) {
    #[cfg(feature = "e11_lender")]
    {
        lend_region::region()
    }
    #[cfg(not(feature = "e11_lender"))]
    {
        (0, 0)
    }
}

/// The block's size: E7's choker compile peaks at 20.6–29.5 KB above its
/// start (the catalog up to 32.6 KB, meteor), its largest single asks are
/// 6–16 KB, a first sync's skeleton read is 25 KB, and the choker's SVG
/// read whole is 27,091 B. 32 KiB holds every choker job and the SVG; a
/// 512-lamp editor read (50 KB) overflows, and a 36 KiB OTA window would
/// not fit (OTA is design-only here).
#[cfg(all(feature = "e11_lender", not(feature = "e11_lend_dram2")))]
pub const LEND_BYTES: usize = 32 * 1024;
/// Variant `e11_lend_dram2`: the block is `dram2_seg`, 64 KiB.
#[cfg(all(feature = "e11_lender", feature = "e11_lend_dram2"))]
pub const LEND_BYTES: usize = 64 * 1024;
