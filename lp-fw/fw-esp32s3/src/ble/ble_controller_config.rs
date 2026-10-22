//! The BLE controller configuration the S3 brings the radio up with.
//!
//! The S3's controller is BTDM (`esp-radio`'s
//! `ble/os_adapter_esp32c3_s3.rs`), not the C6's NPL, so its `Config` is a
//! different struct with different knobs; this is the S3's half of the
//! injection `fw_esp32_radio::ble::start` takes (the C6's is
//! `fw-esp32c6/src/ble/ble_controller_config.rs`).
//!
//! Two settings:
//!
//! - **`max_connections`** is BTDM's `ble_max_act`: the controller's
//!   **activities**, every advertising set and every connection together —
//!   not the C6's connection count. So it is the link slots plus one for the
//!   advertiser that keeps a free slot reachable (3; the default is 6). Set to
//!   the slots alone (2), the controller refused every advertising restart
//!   once one central had connected (`[ble] advertising failed; retrying in
//!   1 s`, FS3, 2026-10-21:
//!   `docs/defects/2026-10-21-the-s3-stops-advertising-after-its-first-connection.md`).
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
        .with_max_connections(RADIO_LINK_SLOTS as u8 + 1)
        .with_task_cpu(esp_hal::system::Cpu::ProCpu)
}
