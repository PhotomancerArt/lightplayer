//! The Wi-Fi station and the network on `lp-net` (feature `wifi`; Wi-Fi
//! roadmap M6). The chip-free half — the join policy, the scan answer, the
//! seam boundary — is `fw_esp32_common::net`.

pub mod esp_frame_device;
pub mod esp_station;
pub mod lan_endpoint_task;
pub mod mdns_task;
pub mod net_heartbeat;
pub mod net_thread;
#[cfg(feature = "net_thread_stack_diag")]
pub mod net_thread_stack_diag;
pub mod station_probes;
pub mod station_task;

pub use station_probes::uses_wifi;
