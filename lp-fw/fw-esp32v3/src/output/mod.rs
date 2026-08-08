//! Output for the classic-ESP32 app layer.
//!
//! The provider itself is chip-agnostic and comes from `fw-esp32-common`
//! unchanged; only the *driver* below it is chip-side. That driver is
//! [`rmt::Esp32V3RmtWs281xDriver`]: real RMT output on as many channels as
//! the board manifest declares, with the RMT memory split among them at
//! driver init (see [`rmt::v3_rmt::plan_for_declared`]), backed by the
//! portable `lp-ws281x` transmitter.

//! A board may also put the LED supply behind a GPIO ([`power_gate`]); the
//! same split applies there — the state machine is shared, the pad is ours.

pub mod power_gate;
pub mod rmt;

pub use fw_esp32_common::output::provider::Esp32OutputProvider;
pub use rmt::Esp32V3RmtWs281xDriver;
