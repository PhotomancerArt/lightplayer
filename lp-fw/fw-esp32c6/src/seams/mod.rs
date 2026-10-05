//! Emulator seams on the C6 (plan `lp2025/2026-10-05-1026-emulator-seams`,
//! M0 spike).
//!
//! A seam is a named `#[no_mangle] extern "C"` function in the one shipped
//! firmware. On silicon its body runs. The emulator finds [`seam_table`] in
//! the flash image, checks its identity, and — only when a seam is engaged —
//! answers the call itself. See `lp-base/lp-seam`.

mod seam_table;
#[cfg(feature = "spike_seam_wake_probe")]
pub mod wake_probe;
mod ws281x_wait_step;

pub use ws281x_wait_step::ws281x_wait_step;
