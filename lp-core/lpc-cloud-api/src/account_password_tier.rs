//! Which of the account's two optional device passwords a call means.

use serde::{Deserialize, Serialize};

/// One of the account's two optional device passwords
/// ([`crate::request::SetAccountPassword`]). They mirror the board's two
/// access tiers: a **play** password unlocks the panel, an **edit**
/// password unlocks authoring.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AccountPasswordTier {
    /// Unlocks play (the panel).
    Play,
    /// Unlocks edit (authoring).
    Edit,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Pinned JSON literals: the deployed spelling is the contract.
    #[test]
    fn pinned_json_literals() {
        assert_eq!(
            serde_json::to_string(&AccountPasswordTier::Play).unwrap(),
            "\"play\""
        );
        assert_eq!(
            serde_json::to_string(&AccountPasswordTier::Edit).unwrap(),
            "\"edit\""
        );
    }
}
