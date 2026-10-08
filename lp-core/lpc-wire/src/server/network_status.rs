//! A board's network settings and station state: the answer to every
//! network request that changes or reads them.

use alloc::vec::Vec;
use lpc_access::NetworkFile;
use serde::{Deserialize, Serialize};

use crate::server::relay_state::RelayState;
use crate::server::saved_network_info::SavedNetworkInfo;
use crate::server::station_state::StationState;

/// The body of [`crate::server::ServerMsgBody::NetworkStatus`] — the answer
/// to [`crate::ClientRequest::NetworkStatus`], `NetworkAdd`,
/// `NetworkForget` and `NetworkSet`. It carries no password: the network
/// file's passwords are write-only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkStatus {
    /// The board's Wi-Fi switch (on by default).
    pub wifi: bool,
    /// Lets lightplayer.app reach this board through the cloud relay (on
    /// by default).
    pub cloud_relay: bool,
    /// The saved networks, in the order they were added, without their
    /// passwords.
    pub networks: Vec<SavedNetworkInfo>,
    /// What the station is doing.
    pub station: StationState,
    /// Whether the board reached lightplayer.app through the cloud relay
    /// (`off` on a board with Cloud relay off, or with no relay).
    pub relay: RelayState,
}

impl NetworkStatus {
    /// The status of `file`, with the station reporting `station`, the
    /// relay `relay`, and no attempt recorded against any network.
    #[must_use]
    pub fn of(file: &NetworkFile, station: StationState, relay: RelayState) -> Self {
        Self {
            wifi: file.wifi,
            cloud_relay: file.cloud_relay,
            networks: file.networks.iter().map(SavedNetworkInfo::of).collect(),
            station,
            relay,
        }
    }

    /// The saved network named `ssid`, if there is one.
    #[must_use]
    pub fn network(&self, ssid: &str) -> Option<&SavedNetworkInfo> {
        self.networks.iter().find(|network| network.ssid == ssid)
    }
}
