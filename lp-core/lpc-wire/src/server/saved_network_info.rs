//! One saved Wi-Fi network, as a board reports it: without its password.

use alloc::string::String;
use lpc_access::WifiNetwork;
use serde::{Deserialize, Serialize};

use crate::server::last_attempt::LastAttempt;

/// What a client may know about one saved network: its name (broadcast by
/// the access point anyway), whether it has a password, whether it is
/// hidden, and how the station's last attempt at it went. **Never the
/// password.**
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedNetworkInfo {
    /// The network name.
    pub ssid: String,
    /// Whether a password is saved (`false`: an open network).
    pub has_password: bool,
    /// The network does not broadcast its name. Omitted when false.
    #[serde(default, skip_serializing_if = "is_false")]
    pub hidden: bool,
    /// How the station's last attempt at this network went, since the board
    /// started — kept in RAM, never in the network file. Absent when it has
    /// not tried (always, on an image with no station).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last: Option<LastAttempt>,
}

impl SavedNetworkInfo {
    /// `network` as reported, with no attempt yet.
    #[must_use]
    pub fn of(network: &WifiNetwork) -> Self {
        Self {
            ssid: network.ssid.clone(),
            has_password: network.has_password(),
            hidden: network.hidden,
            last: None,
        }
    }
}

fn is_false(value: &bool) -> bool {
    !*value
}
