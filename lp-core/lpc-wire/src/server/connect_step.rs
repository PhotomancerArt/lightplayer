//! Where the station is in one attempt at a network.

use serde::{Deserialize, Serialize};

/// The step of [`crate::server::StationState::Connecting`]: how far one
/// attempt has got, so a client can show it advance live (Studio's in-row
/// test: Looking for → Checking the password → Getting an address). A code,
/// not words: Studio words it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ConnectStep {
    /// Listening for the network.
    Looking,
    /// Heard it; associating and checking the password.
    CheckingPassword,
    /// Joined; waiting for an address.
    GettingAddress,
}

impl ConnectStep {
    /// The step's wire name, for summaries and logs.
    #[must_use]
    pub fn code(self) -> &'static str {
        match self {
            Self::Looking => "looking",
            Self::CheckingPassword => "checkingPassword",
            Self::GettingAddress => "gettingAddress",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_step_spelling() {
        for step in [
            ConnectStep::Looking,
            ConnectStep::CheckingPassword,
            ConnectStep::GettingAddress,
        ] {
            let json = crate::json::to_string(&step).unwrap();
            assert_eq!(json, alloc::format!("\"{}\"", step.code()));
            assert_eq!(crate::json::from_str::<ConnectStep>(&json).unwrap(), step);
        }
    }
}
