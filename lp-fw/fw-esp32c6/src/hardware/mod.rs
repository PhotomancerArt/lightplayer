//! Board-level hardware drivers.
//!
//! Each submodule is gated to exactly its callers. Harness builds
//! (`fw_harness`) link only what their own entrypoint reaches, so an
//! over-broad gate here surfaces as dead-code errors under `-D warnings`
//! rather than as a silent extra dependency.

// `test_gpio_input` is here for the same reason `test_button` is: the
// `gpio-input` payload's whole claim is that a button read on the emulator is
// read through the PRODUCT's driver, so its harness calls this module. The
// driver itself is untouched (E-product).
#[cfg(any(not(fw_harness), feature = "test_button", feature = "test_gpio_input"))]
pub mod button;
// The radio *driver* is compiled out of P4 stress builds: there the radio
// stack belongs to the load generators in `stress.rs` instead.
#[cfg(all(
    feature = "radio",
    not(any(feature = "stress_s2", feature = "stress_s3")),
    any(
        not(fw_harness),
        feature = "test_espnow",
        feature = "test_espnow_broadcast"
    )
))]
pub mod espnow_radio_driver;
#[cfg(not(fw_harness))]
pub use fw_esp32_common::hardware::manifest_loader;
