//! Chip-generic output layer: the trait-driven output provider every firmware
//! shares.
//!
//! `rmt_state` used to live here too — the lock-free per-channel state the
//! ESP32-C6's own WS281x driver shared with its interrupt handler. That driver
//! is gone (roadmap M5/P2: the C6 runs `lp-ws281x` like every other chip), and
//! with its last consumer went the module: `lp_ws281x::ChannelState` is where
//! per-channel state lives now, one implementation for all three targets.

pub mod power_gate;
pub mod provider;
#[cfg(feature = "server")]
pub mod wire_stats_source;
