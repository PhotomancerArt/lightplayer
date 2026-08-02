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
//!
//! A fourth module, [`frame_dump`], exists only under the `frame-dump` feature:
//! it is the serial transcript of what the channels transmitted, and it hangs
//! off the app driver's write path. Nothing else depends on it, and it is
//! independent of `ws281x_telemetry` — that one taps the same write path to
//! answer a question about refill budget rather than about pixel values.

pub mod shared_driver;
pub mod v3_rmt;

mod esp32v3_rmt_ws281x_driver;

#[cfg(feature = "frame-dump")]
pub mod frame_dump;

pub use esp32v3_rmt_ws281x_driver::Esp32V3RmtWs281xDriver;
