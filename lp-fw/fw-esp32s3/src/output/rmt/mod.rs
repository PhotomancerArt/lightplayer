//! WS281x output over the ESP32-S3 RMT peripheral.
//!
//! Three layers, deliberately separated:
//!
//! * [`s3_rmt`] — the chip. Seven register operations implementing
//!   [`lp_ws281x::RmtHw`], and the only place in this firmware that knows an
//!   ESP32-S3 address.
//! * [`shared_driver`] — the one [`lp_ws281x::Ws281xDriver`] instance and the
//!   RMT interrupt trampoline that feeds it. All sequencing lives in the core
//!   crate, tested on the host.
//! * [`esp32s3_rmt_ws281x_driver`] — the `lpc-hardware` seam: endpoints, leases,
//!   and open-time pin binding.
//!
//! The first two are compiled into harness builds as well, because the
//! `test_loopback` harness drives exactly the same backend and driver the app
//! path does — a self-test against a different transmitter would prove nothing.
//!
//! A fourth module, [`frame_dump`], exists only under the `frame-dump` feature:
//! it is the serial transcript of what the channels transmitted, and it hangs
//! off the app driver's write path. Nothing else depends on it.

pub mod s3_rmt;
pub mod shared_driver;

#[cfg(not(fw_harness))]
mod esp32s3_rmt_ws281x_driver;

#[cfg(all(not(fw_harness), feature = "frame-dump"))]
pub mod frame_dump;

#[cfg(not(fw_harness))]
pub use esp32s3_rmt_ws281x_driver::Esp32S3RmtWs281xDriver;
