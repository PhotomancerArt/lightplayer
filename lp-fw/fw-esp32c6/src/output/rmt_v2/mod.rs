//! WS281x output over the ESP32-C6 RMT peripheral, on the shared
//! [`lp_ws281x`] driver core.
//!
//! Three layers, deliberately separated:
//!
//! * [`c6_rmt`] — the chip. Seven register operations implementing
//!   [`lp_ws281x::RmtHw`], and the only place in this module that knows an
//!   ESP32-C6 address.
//! * [`shared_driver`] — the one [`lp_ws281x::Ws281xDriver`] instance and the
//!   RMT interrupt trampoline that feeds it, bound at `Priority::max()`. All
//!   sequencing lives in the core crate, tested on the host.
//! * [`esp32c6_rmt_ws281x_driver`] — the `lpc-hardware` seam: endpoints,
//!   leases, and open-time pin binding.
//!
//! # Why `rmt_v2`, and what happens to it
//!
//! The legacy single-channel driver in `crate::output::rmt` still owns the
//! shipping image; this module is its replacement, compiled but not yet
//! constructed, so that the register work and the integration land as separate
//! reviewable steps (roadmap M5, phase P1). The next phase points the output
//! provider here, deletes the legacy driver together with
//! `fw-esp32-common`'s `rmt_state`, and renames this directory back to `rmt`.
//! Until then the two coexist and only one of them runs.
//!
//! See ADR `2026-07-31-lp-ws281x-multi-channel-driver-adoption`: new chips
//! implement [`lp_ws281x::RmtHw`], they do not grow a driver of their own.

pub mod c6_rmt;
pub mod shared_driver;

/// The registry seam needs `lpc-hardware`, which harness builds without the
/// `server` feature do not pull in. The two layers below it have no such
/// dependency and always compile.
#[cfg(feature = "lpc-hardware")]
pub mod esp32c6_rmt_ws281x_driver;

#[cfg(feature = "lpc-hardware")]
pub use esp32c6_rmt_ws281x_driver::Esp32C6RmtWs281xDriver;
