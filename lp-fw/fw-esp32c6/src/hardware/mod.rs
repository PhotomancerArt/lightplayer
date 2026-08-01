//! Board-level hardware drivers.
//!
//! Each submodule is gated to exactly its callers. Harness builds
//! (`fw_harness`) link only what their own entrypoint reaches, so an
//! over-broad gate here surfaces as dead-code errors under `-D warnings`
//! rather than as a silent extra dependency.

#[cfg(any(not(fw_harness), feature = "test_button"))]
pub mod button;
#[cfg(all(feature = "radio", any(not(fw_harness), feature = "test_espnow")))]
pub mod espnow_radio_driver;
#[cfg(not(fw_harness))]
pub use fw_esp32_common::hardware::manifest_loader;
