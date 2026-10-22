//! The BLE controller configuration the S3 brings the radio up with.
//!
//! The S3's controller is BTDM (`esp-radio`'s
//! `ble/os_adapter_esp32c3_s3.rs`), not the C6's NPL, so its `Config` is a
//! different struct with different knobs; this is the S3's half of the
//! injection `fw_esp32_radio::ble::start` takes (the C6's is
//! `fw-esp32c6/src/ble/ble_controller_config.rs`).
//!
//! Two settings, both the C6's rules in BTDM's words:
//!
//! - **`max_connections`** is the link slot count by name, as on the C6 (the
//!   default is 6): LightPlayer holds at most [`RADIO_LINK_SLOTS`] centrals,
//!   and every connection the controller can hold is memory it keeps.
//! - **`task_cpu`** is core 0 by name (it is the default): this firmware never
//!   starts core 1, so a controller task pinned there would never run.
//!
//! Everything else is esp-radio's default, which is ESP-IDF's
//! `BT_CONTROLLER_INIT_CONFIG_DEFAULT` for this chip; a cut beyond these two
//! is a measured change, as E13 was on the C6.

use esp_radio::ble::Config;
use fw_esp32_common::radio_link::RADIO_LINK_SLOTS;

/// What the S3 runs.
pub fn ble_controller_config() -> Config {
    Config::default()
        .with_max_connections(RADIO_LINK_SLOTS as u8)
        .with_task_cpu(esp_hal::system::Cpu::ProCpu)
}
