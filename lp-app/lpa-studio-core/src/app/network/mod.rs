//! Wi‑Fi settings, Studio's side: reading a board's network status, the
//! Wi‑Fi verbs at `devices/<board>/wifi/…`, and what the card's Wi‑Fi row
//! and popover read.
//!
//! The board keeps the settings in `/.lp/network.json`, write-only on every
//! link; it answers a status that never carries the password, and every M5
//! image says its station is `unsupported` (the station is M6). Studio
//! never stores the password: it lives in the form until the press, in the
//! op until the request leaves, and nowhere after.
//!
//! | concept | file |
//! |---|---|
//! | what a change does to the password, never printed | [`wifi_password_change`] |
//! | one change, as the offers dispatch it | [`network_op`] |
//! | the three requests over a device's client | [`device_network_ops`] |
//! | the controller's inputs on the actor's queue | [`network_command`] |
//! | read once per connection, run the changes, hold the status | [`network_controller`] |
//! | the status sentences | [`wifi_status_sentence`] |
//! | what the card's row and popover read | [`ui_device_wifi`] |
//! | the Wi‑Fi verbs as offers | [`wifi_offers`] |
//!
//! Decision record: `docs/adr/2026-10-04-device-wifi-settings.md`.

pub mod device_network_ops;
pub mod network_command;
pub mod network_controller;
pub mod network_op;
pub mod ui_device_wifi;
pub mod wifi_offers;
pub mod wifi_password_change;
pub mod wifi_status_sentence;

pub use device_network_ops::{NetworkRefusal, NetworkStep};
pub use network_command::{NetworkCommand, NetworkStepKind};
pub use network_controller::{NetworkController, WifiReach, wifi_reach_for};
pub use network_op::{NetworkChange, NetworkOp};
pub use ui_device_wifi::{NEEDS_AUTHOR, READING, UiDeviceWifi, WIFI_ABOUT};
pub use wifi_offers::{
    WIFI_BUSY, WIFI_CLOUD_RELAY_SUMMARY, WIFI_ENABLED_PARAM, WIFI_NETWORK_PARAM,
    WIFI_PASSWORD_PARAM, WIFI_SEGMENT, wifi_offers,
};
pub use wifi_password_change::PasswordChange;
pub use wifi_status_sentence::{
    NOT_SET, RELAY_OFF, RELAY_ON, SAVED_UNSUPPORTED, cloud_relay_sentence, wifi_status_sentence,
};
