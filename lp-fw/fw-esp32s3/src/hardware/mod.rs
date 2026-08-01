//! Board-level hardware drivers.
//!
//! Mirrors fw-esp32c6's `hardware` module. Each submodule is gated to exactly
//! its callers: harness builds (`fw_harness`) link only what their own
//! entrypoint reaches, so an over-broad gate here surfaces as a dead-code
//! error under `-D warnings` rather than as a silent extra dependency.

#[cfg(any(not(fw_harness), feature = "test_button"))]
pub mod button;
