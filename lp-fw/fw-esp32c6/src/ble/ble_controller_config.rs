//! The BLE controller configuration the product brings the radio up with.
//!
//! Until RAM experiment E13 (`lp2025/2026-10-09-1203-ram-research`) the
//! product passed `Default::default()` to `BleConnector::new`: esp-radio's
//! `Config`, sized after ESP-IDF's `BT_CONTROLLER_INIT_CONFIG_DEFAULT`, for a
//! controller that may hold two connections and scan, sync to periodic
//! advertisers and run extended advertising. LightPlayer advertises (legacy
//! connectable) and holds at most [`RADIO_LINK_SLOTS`] centrals; it never
//! scans, syncs, uses a filter accept list, or resolves private addresses.
//!
//! [`ble_controller_config`] is the one place those numbers live. With the
//! `radio_cfg_probe` feature (research, never shipped) the build reads
//! `LP_BLE_CFG` — `key=value,key=value` over the field names below — so one
//! source tree can be built at several configurations and measured on the
//! emulator. Without it the function returns what is written here.

use esp_radio::ble::Config;

/// What the product runs, field by field. See the E13 report for what each
/// one sizes and why this value.
pub fn ble_controller_config() -> Config {
    let config = Config::default();
    #[cfg(feature = "radio_cfg_probe")]
    let config = probe::apply(config, option_env!("LP_BLE_CFG").unwrap_or(""));
    config
}

#[cfg(feature = "radio_cfg_probe")]
mod probe {
    use esp_radio::ble::Config;

    /// Apply `key=value,…` over `config`. An unknown key is a build-time
    /// panic in the probe image, never a silent no-op.
    pub fn apply(mut config: Config, spec: &str) -> Config {
        for pair in spec.split(',').filter(|p| !p.is_empty()) {
            let (key, value) = pair.split_once('=').expect("LP_BLE_CFG: key=value");
            let n: u32 = value.parse().expect("LP_BLE_CFG: integer value");
            config = match key {
                "max_connections" => config.with_max_connections(n as u16),
                "acl_buf_size" => config.with_acl_buf_size(n as u16),
                "acl_buf_count" => config.with_acl_buf_count(n as u16),
                "hci_evt_buf_size" => config.with_hci_evt_buf_size(n as u16),
                "hci_high_buffer_count" => config.with_hci_high_buffer_count(n as u16),
                "hci_low_buffer_count" => config.with_hci_low_buffer_count(n as u16),
                "whitelist_size" => config.with_whitelist_size(n as u8),
                "ll_resolv_list_size" => config.with_ll_resolv_list_size(n as u16),
                "ll_sync_list_cnt" => config.with_ll_sync_list_cnt(n as u8),
                "ll_sync_cnt" => config.with_ll_sync_cnt(n as u8),
                "ll_rsp_dup_list_count" => config.with_ll_rsp_dup_list_count(n as u16),
                "ll_adv_dup_list_count" => config.with_ll_adv_dup_list_count(n as u16),
                "multi_adv_instances" => config.with_multi_adv_instances(n as u16),
                "ext_adv_max_size" => config.with_ext_adv_max_size(n as u16),
                "task_stack_size" => config.with_task_stack_size(n as u16),
                _ => panic!("LP_BLE_CFG: unknown key"),
            };
        }
        config
    }
}
