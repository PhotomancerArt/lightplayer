//! The Wi-Fi station and the network on `lp-net` (feature `wifi`; Wi-Fi
//! roadmap M6). The network itself — the station, the IP stack, the LAN
//! endpoint, mDNS, the relay's device leg, the thread they run on — is
//! `fw_esp32_radio::net` (re-exported here by module, so the boot path reads
//! as before); the chip-free half under it is `fw_esp32_common::net`. What
//! stays here is this chip's: the thread's facts and hooks ([`net_chip`]),
//! the relay's build-time address ([`relay_config`]), and the stack
//! diagnostic.

pub mod net_chip;
#[cfg(feature = "net_thread_stack_diag")]
pub mod net_thread_stack_diag;
pub mod relay_config;

pub use fw_esp32_radio::net::{net_heartbeat, net_thread, relay_probes, station_probes};
