//! The saved Wi-Fi network, as a board reports it: without its password.

use alloc::string::String;
use lpc_access::WifiNetwork;
use serde::{Deserialize, Serialize};

/// What a client may know about the saved network: its name (broadcast by
/// the access point anyway), whether it has a password, and whether it is
/// switched on. **Never the password.**
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WifiInfo {
    /// The network name.
    pub ssid: String,
    /// Whether a password is saved (`false`: an open network).
    pub has_password: bool,
    /// Join when on.
    pub enabled: bool,
}

impl From<&WifiNetwork> for WifiInfo {
    fn from(network: &WifiNetwork) -> Self {
        Self {
            ssid: network.ssid.clone(),
            has_password: network.has_password(),
            enabled: network.enabled,
        }
    }
}
