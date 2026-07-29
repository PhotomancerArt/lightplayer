//! The guest ↔ host syscall ABI.
//!
//! A guest triggers a host call by executing the `SYSCALL` instruction with:
//!
//! | register | meaning                          |
//! |----------|----------------------------------|
//! | `a2`     | syscall number (constants below) |
//! | `a3`     | first argument                   |
//! | `a4`     | second argument                  |
//! | `a5`     | third argument (currently unused)|
//!
//! The host writes the result back into `a2` before resuming the guest.
//! `SYSCALL` does not rotate the register window, so these are the *current*
//! window's registers on both sides.
//!
//! The guest-side mirror of these constants lives in
//! `lp-xt/lp-xt-emu-guest/src/syscall.rs` — the two must stay in sync (the
//! fixture tests catch drift: every fixture prints and exits through this ABI).

/// Terminate the run. `a3` = exit code; the run completes with
/// `RunOutcome::Ok(code)`. Does not return to the guest.
pub const SYS_EXIT: u32 = 1;

/// Write bytes to the host-collected output stream. `a3` = guest pointer,
/// `a4` = length in bytes. Returns the length written (or [`ERR`] if the
/// range is not readable guest memory).
pub const SYS_WRITE: u32 = 2;

/// Report a panic. `a3` = message pointer, `a4` = message length. The host
/// records the message and terminates the run with exit code
/// [`PANIC_EXIT_CODE`]. Does not return to the guest.
pub const SYS_PANIC: u32 = 3;

/// Result value returned in `a2` for a failed or unknown syscall.
pub const ERR: u32 = u32::MAX;

/// Exit code the host synthesizes for a `SYS_PANIC` termination (mirrors the
/// Rust panic process-exit convention).
pub const PANIC_EXIT_CODE: u32 = 101;
