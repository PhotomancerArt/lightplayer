//! The BLE controller configuration the product brings the radio up with.
//!
//! Until RAM experiment E13 (`lp2025/2026-10-09-1203-ram-research`) the
//! product passed `Default::default()` to `BleConnector::new`: esp-radio's
//! `Config`, sized after ESP-IDF's `BT_CONTROLLER_INIT_CONFIG_DEFAULT`, for a
//! controller that may hold two connections, scan, sync to periodic
//! advertisers, keep a filter accept list and resolve private addresses.
//! LightPlayer advertises (legacy, connectable, scannable) and holds at most
//! [`RADIO_LINK_SLOTS`] centrals. It never scans (trouble-host is built
//! without `central`), never syncs, never uses the accept list and has no
//! bonding, so none of those lists needs more than its minimum of one entry.
//!
//! What the cut buys, measured on `lp-emu:esp32c6:t1` (the controller's
//! init-time heap, `[ble] up: controller +N B`): 19,436 B at the defaults,
//! 18,372 B here (-1,064 B: the scan duplicate lists and the periodic-sync
//! list -740 B, the accept and resolving lists -324 B). Advertising starts
//! as before.
//!
//! **What is deliberately left alone**, with the measurement behind each:
//!
//! - `max_connections` is now the slot count by name (it was the default, 2,
//!   which is the same number): a third slot must not leave the controller at
//!   two. Each connection the controller can hold costs 1,048 B at boot.
//! - `hci_high_buffer_count` (30) and `hci_low_buffer_count` (8): 72 B each at
//!   boot, 2,744 B together. The controller's event buffers are freed inside
//!   the host callback, so a handful would do, but a short pool that drops a
//!   Number-of-Completed-Packets event stalls the link; that needs a central
//!   on silicon. Not changed.
//! - `acl_buf_count` (24) and `acl_buf_size` (255): no standing memory at all
//!   (changing them moved the boot heap by 0 B). They are the host's transmit
//!   credits and fragment size, and a cap on the controller's on-demand ACL
//!   buffers. `acl_buf_size` must stay at 251 or more for the ATT MTU of 247.
//! - `ext_adv_max_size` (31) must stay: at 0 advertising fails to start
//!   (`[ble] advertising failed; retrying`), and the advertising data buffers
//!   are the 1,384 B allocated the moment advertising starts.
//! - `task_stack_size` (4,096) is honored byte for byte, and the controller
//!   task touched 920 B of it with no central connected. A connection's peak
//!   is unmeasured, so it stays.
//!
//! [`ble_controller_config`] is the one place those numbers live. With the
//! `radio_cfg_probe` feature (research, never shipped) the build reads
//! `LP_BLE_CFG` — `key=value,key=value` over the field names below — on top of
//! what is written here, so one source tree can be built at several
//! configurations and measured on the emulator.

use esp_radio::ble::Config;
use fw_esp32_common::radio_link::RADIO_LINK_SLOTS;

/// What the product runs.
pub fn ble_controller_config() -> Config {
    let config = Config::default()
        .with_max_connections(RADIO_LINK_SLOTS as u16)
        .with_whitelist_size(1)
        .with_ll_resolv_list_size(1)
        .with_ll_sync_list_cnt(1)
        .with_ll_rsp_dup_list_count(1)
        .with_ll_adv_dup_list_count(1);
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
