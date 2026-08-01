//! Output for the classic-ESP32 app layer.
//!
//! The provider itself is chip-agnostic and comes from `fw-esp32-common`
//! unchanged; only the *driver* below it is chip-side. That driver is
//! [`rmt::Esp32V3RmtWs281xDriver`]: real RMT output on up to four channels at
//! once (two RMT memory blocks each — see
//! [`rmt::v3_rmt::BLOCKS_PER_CHANNEL`]), backed by the portable `lp-ws281x`
//! transmitter.

pub mod rmt;

pub use fw_esp32_common::output::provider::Esp32OutputProvider;
pub use rmt::Esp32V3RmtWs281xDriver;
