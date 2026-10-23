//! Where this image's relay is and what the board tells it: the relay
//! client's [`RelayClientConfig`] for `fw_esp32_radio::net::relay_task`.
//!
//! These are this crate's build facts, so they stay here: the relay's
//! address is `lightplayer.app:80` on the product image, and a desk image
//! built with `LP_RELAY_HOST=<host>[:port]` dials that instead and says so
//! at boot (RD14; `build.rs` `emit_relay_host`). The hello's firmware
//! version is the manifest's version slot (`crate::manifest_version`).

use alloc::string::String;

use lpc_relay::RelayClientConfig;

/// The relay's product address, or the desk image's (`LP_RELAY_HOST`).
pub const RELAY_HOST: &str = env!("LP_RELAY_HOST_NAME");

/// The relay's port (80: the device leg is plain HTTP).
pub fn relay_port() -> u16 {
    env!("LP_RELAY_HOST_PORT").parse().unwrap_or(80)
}

/// Whether this image was built to dial a desk relay (`LP_RELAY_HOST`).
pub fn relay_host_overridden() -> bool {
    env!("LP_RELAY_HOST_OVERRIDDEN") == "true"
}

/// The relay's `config` for this board: its MAC, its name (the stamped
/// `/.lp/device.json` name, else its `lp-xxxx` LAN label), this build's wire
/// version and app version (the protocol 2 hello's firmware), and the
/// relay's address (`lightplayer.app:80`, or the desk image's
/// `LP_RELAY_HOST`). One session at a time, as on the C6.
pub fn relay_config(board_mac: [u8; 6], name: Option<String>) -> RelayClientConfig {
    RelayClientConfig {
        host: String::from(RELAY_HOST),
        port: relay_port(),
        board_mac,
        label: name.unwrap_or_else(|| fw_esp32_common::net::mdns::mdns_label(board_mac)),
        wire_proto: lpc_wire::WIRE_PROTO_VERSION,
        max_routes: 1,
        firmware: crate::manifest_version(),
    }
}
