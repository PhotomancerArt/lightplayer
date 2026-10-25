//! The Wi-Fi station and the network on `lp-net` (feature `wifi`; S3 radio
//! roadmap M4). The network itself — the thread, the station, the IP stack,
//! the LAN endpoint, mDNS, the relay's device leg — is
//! `fw_esp32_radio::net`, the C6's (re-exported here by module, as the C6's
//! `net` does); the chip-free half under it is `fw_esp32_common::net`. What
//! is this chip's: the thread's facts and hooks ([`net_chip`]) and the
//! relay's build-time address ([`relay_config`]).

pub mod net_chip;
pub mod relay_config;

pub use fw_esp32_radio::net::{net_heartbeat, net_thread, relay_probes, station_probes};
