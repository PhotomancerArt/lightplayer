//! Output for the classic-ESP32 app layer.
//!
//! The provider itself is chip-agnostic and comes from `fw-esp32-common`
//! unchanged; only the *driver* below it is chip-side. That driver is
//! [`rmt::Esp32V3RmtWs281xDriver`]: real RMT output on as many channels as
//! the board manifest declares, with the RMT memory split among them at
//! driver init (see [`rmt::v3_rmt::plan_for_declared`]), backed by the
//! portable `lp-ws281x` transmitter.

pub mod rmt;

pub use fw_esp32_common::output::provider::Esp32OutputProvider;
pub use rmt::Esp32V3RmtWs281xDriver;
