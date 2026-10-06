//! Why the station's attempt at a network failed.

use serde::{Deserialize, Serialize};

/// Why [`crate::server::StationState::Failed`] failed. A code, not words:
/// Studio words it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StationFailure {
    /// The network refused the password.
    WrongPassword,
    /// The station did not hear the network.
    NotFound,
    /// It joined but got no address.
    NoAddress,
}

impl StationFailure {
    /// The code's wire name, for summaries and logs.
    #[must_use]
    pub fn code(self) -> &'static str {
        match self {
            Self::WrongPassword => "wrongPassword",
            Self::NotFound => "notFound",
            Self::NoAddress => "noAddress",
        }
    }
}
