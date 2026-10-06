//! Wi‑Fi settings, Studio's side: reading a board's network status, the
//! Wi‑Fi verbs at `devices/<board>/wifi/…`, and what the card's Wi‑Fi row
//! and its three-page popover read.
//!
//! The board keeps up to eight networks in `/.lp/network.json`, write-only
//! on every link; it answers a status that never carries a password, and
//! every M5 image says its station is `unsupported` (the station is M6).
//! Studio never stores a password: it lives in the form until the press,
//! in the op until the request leaves, and nowhere after.
//!
//! | concept | file |
//! |---|---|
//! | the password an add carries, never printed | [`wifi_password_change`] |
//! | one change, as the offers dispatch it | [`network_op`] |
//! | the five requests over a device's client | [`device_network_ops`] |
//! | the controller's inputs on the actor's queue | [`network_command`] |
//! | read once per connection, scan, run the changes, hold the status | [`network_controller`] |
//! | the words | [`wifi_words`] |
//! | what the card's row and popover read | [`ui_device_wifi`] |
//! | the test in a just-added network's row | [`ui_wifi_test`] |
//! | each saved network's id in an offer path | [`wifi_network_slug`] |
//! | the Wi‑Fi verbs as offers | [`wifi_offers`] |
//!
//! Decision record: `docs/adr/2026-10-04-device-wifi-settings.md`.

pub mod device_network_ops;
pub mod network_command;
pub mod network_controller;
pub mod network_op;
pub mod ui_device_wifi;
pub mod ui_wifi_test;
pub mod wifi_network_slug;
pub mod wifi_offers;
pub mod wifi_password_change;
pub mod wifi_words;

pub use device_network_ops::{NetworkRefusal, NetworkStep};
pub use network_command::{NetworkCommand, NetworkStepKind};
pub use network_controller::{NetworkController, WifiReach, wifi_reach_for};
pub use network_op::{NetworkChange, NetworkOp};
pub use ui_device_wifi::{NEEDS_AUTHOR, READING, UiDeviceWifi, UiWifiNetworkRow};
pub use ui_wifi_test::{
    UiWifiTest, UiWifiTestResult, UiWifiTestStepLine, WifiStepState, WifiTestNext, WifiTestOutcome,
    WifiTestProgress, WifiTestStep,
};
pub use wifi_offers::{
    PASSWORD_NEEDED, WIFI_BUSY, WIFI_CLOUD_RELAY_SUMMARY, WIFI_ENABLED_PARAM, WIFI_FORGET_SEGMENT,
    WIFI_HIDDEN_PARAM, WIFI_NETWORK_PARAM, WIFI_PASSWORD_PARAM, WIFI_SEGMENT, wifi_offers,
};
pub use wifi_password_change::PasswordChange;
pub use wifi_words::{WifiTone, signal_bars, signal_word};
