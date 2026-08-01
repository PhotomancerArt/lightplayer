//! Output for the ESP32-S3 app layer.
//!
//! The provider itself is chip-agnostic and comes from `fw-esp32-common`
//! unchanged; only the *driver* below it is chip-side. That driver is
//! [`rmt::Esp32S3RmtWs281xDriver`]: real RMT output on up to four channels at
//! once, backed by the portable `lp-ws281x` transmitter.
//!
//! [`rmt`] is compiled into harness builds too — the `test_loopback` harness
//! drives the same backend and the same shared driver, which is what makes its
//! verdict mean anything about the app path.
//!
//! Under the `frame-dump` feature that driver's write path also prints each
//! transmitted frame to serial ([`rmt::frame_dump`]). An LED cannot be diffed;
//! the transcript can, which is how the M4 hardware walk states its result as
//! "192 of 192 bytes" instead of "it lit up".

pub mod rmt;

#[cfg(not(fw_harness))]
pub use fw_esp32_common::output::provider::Esp32OutputProvider;
#[cfg(not(fw_harness))]
pub use rmt::Esp32S3RmtWs281xDriver;
