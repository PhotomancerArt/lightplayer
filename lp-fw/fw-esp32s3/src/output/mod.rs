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

pub mod rmt;

#[cfg(not(fw_harness))]
pub use fw_esp32_common::output::provider::Esp32OutputProvider;
#[cfg(not(fw_harness))]
pub use rmt::Esp32S3RmtWs281xDriver;
