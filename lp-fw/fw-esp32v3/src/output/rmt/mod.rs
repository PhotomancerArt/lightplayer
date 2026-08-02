//! WS281x output over the classic-ESP32 RMT peripheral.
//!
//! Three layers, deliberately separated — the same split fw-esp32s3 uses:
//!
//! * [`v3_rmt`] — the chip. Seven register operations implementing
//!   [`lp_ws281x::RmtHw`], and the only place in this firmware that knows a
//!   classic-ESP32 RMT address.
//! * [`shared_driver`] — the one [`lp_ws281x::Ws281xDriver`] instance, the RMT
//!   interrupt trampoline that feeds it, and the optional telemetry tap. All
//!   sequencing lives in the core crate, tested on the host.
//! * [`esp32v3_rmt_ws281x_driver`] — the `lpc-hardware` seam: endpoints,
//!   leases, open-time pin binding, and the manifest-index → RMT-slot mapping
//!   that two blocks per channel makes necessary.

pub mod shared_driver;
pub mod v3_rmt;

// The level-4 experiment (see its module docs): an alternative refill path
// behind `hli_refill`/`hli_stress`, absent from the shipping image.
#[cfg(any(feature = "hli_refill", feature = "hli_stress"))]
pub mod hli;

// The endpoint layer needs the whole `lpc-hardware` vocabulary, which only
// the server build links; the `hli_stress` harness compiles this module tree
// without it.
#[cfg(feature = "server")]
mod esp32v3_rmt_ws281x_driver;

#[cfg(feature = "server")]
pub use esp32v3_rmt_ws281x_driver::Esp32V3RmtWs281xDriver;
