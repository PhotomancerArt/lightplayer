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
// Power-off belongs to the product server loop only.
#[cfg(all(not(fw_harness), feature = "server"))]
pub mod power;
// The radio's driver config, the ESP-NOW driver and the radio's one bring-up
// live in `fw_esp32_radio::hardware`. Each is re-exported here only for the
// builds whose code in THIS crate names it (the driver's own uses of the
// other two are inside that crate now).
//
// The radio's driver config is shared by every bring-up that mirrors the
// product's: the ESP-NOW driver (`fw_esp32_radio`), and here the stress
// builds, the desk ESP-NOW meter and the BLE coexistence harness.
#[cfg(any(
    all(
        any(
            feature = "stress_s2",
            feature = "stress_s3",
            feature = "desk_espnow_meter"
        ),
        not(fw_harness)
    ),
    feature = "test_ble_coex"
))]
pub use fw_esp32_radio::hardware::espnow_controller_config;
// The radio *driver* is compiled out of P4 stress builds: there the radio
// stack belongs to the load generators in `stress.rs` instead.
#[cfg(all(
    feature = "radio",
    not(any(
        feature = "stress_s2",
        feature = "stress_s3",
        feature = "desk_espnow_meter"
    )),
    any(
        not(fw_harness),
        feature = "test_espnow",
        feature = "test_espnow_broadcast"
    )
))]
pub use fw_esp32_radio::hardware::espnow_radio_driver;
// The radio's one bring-up (`esp_radio::wifi::new`), shared by the ESP-NOW
// driver and the station: the app's boot path calls it (a harness's driver
// calls it itself, `Esp32EspNowRadioDriver::new`).
#[cfg(not(fw_harness))]
pub use fw_esp32_common::hardware::manifest_loader;
#[cfg(all(
    feature = "radio",
    not(any(
        feature = "stress_s2",
        feature = "stress_s3",
        feature = "desk_espnow_meter"
    )),
    not(fw_harness)
))]
pub use fw_esp32_radio::hardware::radio_hub;
