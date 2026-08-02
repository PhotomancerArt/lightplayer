//! The recovery-frame guard around node execution.
//!
//! ## What this file is, and what it stopped being
//!
//! Until 2026-08-02 it was the panic boundary: `catch_node_panic` wrapped a
//! node call in the `unwinding` crate's `catch_unwind`, turned a caught panic
//! into a [`NodeError`], and told the ledger about it via
//! `record_recovered_crash` — layer-1 recovery. That is gone with the unwind
//! tier (ADR `2026-08-02-rv32-firmwares-are-abort-tier`): no target catches
//! panics any more, so a panic during node execution is terminal and the next
//! boot reports it from the RTC breadcrumb.
//!
//! What remains is the half that was **never gated on `panic-recovery`**, and
//! it is the more load-bearing half: [`catch_node_panic_framed`] pushes a
//! recovery frame, so the blame ledger knows what was running when the device
//! died, and it denies entry to a path that repeated crashes have gated red.
//! Frame stack, blame, yellow → red escalation, hierarchical parent gating and
//! safe mode all still work exactly as
//! `docs/adr/2026-07-04-crash-recovery-model.md` describes — the only change is
//! that a crash costs a reboot to record instead of being caught in place.

use super::NodeError;

/// Run `f` inside a recovery frame.
///
/// Entering the frame is what makes the work attributable: the persistent frame
/// stack blames this node if the device panics hard or hangs (watchdog), and a
/// path gated red after repeated crashes is denied up front with a user-legible
/// [`NodeError`] instead of executing.
///
/// On targets without an installed recovery global (host, browser) the frame
/// guard is inert and this is just `f()`.
pub fn catch_node_panic_framed<T>(
    kind: lp_recovery::FrameKind,
    name: &str,
    f: impl FnOnce() -> Result<T, NodeError>,
) -> Result<T, NodeError> {
    let _guard = match lp_recovery::enter(kind, name) {
        Ok(guard) => guard,
        Err(denied) => return Err(NodeError::msg(alloc::format!("{denied}"))),
    };
    f()
}
