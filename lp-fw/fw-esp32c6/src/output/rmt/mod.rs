//! WS281x output over the ESP32-C6 RMT peripheral, on the shared
//! [`lp_ws281x`] driver core.
//!
//! Three layers, deliberately separated:
//!
//! * [`c6_rmt`] — the chip. Seven register operations implementing
//!   [`lp_ws281x::RmtHw`], and the only place in this module that knows an
//!   ESP32-C6 address.
//! * [`shared_driver`] — the one [`lp_ws281x::Ws281xDriver`] instance and the
//!   RMT interrupt trampoline that feeds it, bound at `Priority::max()`, plus
//!   the optional telemetry tap. All sequencing lives in the core crate,
//!   tested on the host.
//! * [`esp32c6_rmt_ws281x_driver`] — the `lpc-hardware` seam: endpoints,
//!   leases, and open-time pin binding.
//!
//! `led_channel` sits beside them for the hardware harnesses, which drive a
//! strip without a registry; it is compiled only for those builds.
//!
//! # What this replaced
//!
//! Until roadmap M5/P2 this chip ran a WS281x driver of its own — its own ISR,
//! its own ping-pong refill over a fixed four-block window, and
//! `fw-esp32-common`'s `rmt_state` — the ancestor `lp-ws281x` was extracted
//! from and written to replace. That code is gone: the C6 now runs the same
//! core as the ESP32-S3 and the classic ESP32, which is what makes a bug fixed
//! once fixed everywhere, and what gives this chip its second output.
//!
//! See ADR `2026-07-31-lp-ws281x-multi-channel-driver-adoption`: new chips
//! implement [`lp_ws281x::RmtHw`], they do not grow a driver of their own.

pub mod c6_rmt;
pub mod shared_driver;

/// The registry seam needs `lpc-hardware`, which harness builds without the
/// `server` feature do not pull in — and no harness constructs it in any case,
/// since they replace the app entrypoint. The two layers below it have no such
/// dependency and always compile.
#[cfg(all(not(fw_harness), feature = "lpc-hardware"))]
pub mod esp32c6_rmt_ws281x_driver;

#[cfg(all(not(fw_harness), feature = "lpc-hardware"))]
pub use esp32c6_rmt_ws281x_driver::Esp32C6RmtWs281xDriver;

/// The harnesses' single-strip API. `main.rs` compiles this module only for the
/// app and the five harnesses that light a strip, so `fw_harness` here means
/// exactly those five: in the shipping image it would be a second way to send
/// a frame, which is the thing this migration removed.
#[cfg(fw_harness)]
pub mod led_channel;

#[cfg(fw_harness)]
pub use led_channel::LedChannel;
