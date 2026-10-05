//! How the station's last attempt at one saved network went.

use serde::{Deserialize, Serialize};

/// The outcome of the station's last attempt at a saved network, in
/// [`crate::server::SavedNetworkInfo::last`]. A code, not words: Studio
/// words it. Kept in RAM by the station (M6); never persisted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LastAttempt {
    /// It joined and got an address.
    Connected,
    /// The network refused the password.
    WrongPassword,
    /// The station did not hear the network.
    NotFound,
    /// It joined but got no address.
    NoAddress,
}
