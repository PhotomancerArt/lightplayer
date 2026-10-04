//! A board's network settings and station state: the answer to every
//! network request.

use lpc_access::NetworkFile;
use serde::{Deserialize, Serialize};

use crate::server::station_state::StationState;
use crate::server::wifi_info::WifiInfo;

/// The body of [`crate::server::ServerMsgBody::NetworkStatus`] — the answer
/// to [`crate::ClientRequest::NetworkStatus`], `NetworkSet` and
/// `NetworkForget`. It carries no password: the network file's password is
/// write-only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkStatus {
    /// The saved network, without its password; absent when none is saved.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wifi: Option<WifiInfo>,
    /// Never dial the relay.
    pub lan_only: bool,
    /// What the station is doing.
    pub station: StationState,
}

impl NetworkStatus {
    /// The status of `file`, with the station reporting `station`.
    #[must_use]
    pub fn of(file: &NetworkFile, station: StationState) -> Self {
        Self {
            wifi: file.wifi.as_ref().map(WifiInfo::from),
            lan_only: file.lan_only,
            station,
        }
    }
}
