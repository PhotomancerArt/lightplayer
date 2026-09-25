//! The caller's account device key and optional account passwords.

use alloc::string::String;
use alloc::vec::Vec;
use serde::{Deserialize, Serialize};

/// Answers [`crate::request::GetAccountAccess`],
/// [`crate::request::SetAccountPassword`] and
/// [`crate::request::ResetAccountKey`]: everything a signed-in client needs
/// to install the account on a device and to unlock one.
///
/// # What it carries
///
/// The **account key** is one generated secret and one salt, used on every
/// device the account is installed on. A board keys its access entries by
/// salt, so a client matches a board's offer to this key by `key_salt`.
/// Each optional **account password** (play, edit) has a salt of its own
/// for the same reason: a password is installed as its own entry, and the
/// salt is what names that entry on the board. A password salt is fixed for
/// the life of the record, so changing a password replaces the board entry
/// instead of adding a second one.
///
/// The passwords come back readable: they are shareable device passwords,
/// like a Wi-Fi password, and Settings shows them. This record answers only
/// to the account itself.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountAccessInfo {
    /// The account key's 32-byte secret, base64.
    #[serde(with = "crate::base64_bytes")]
    pub key_secret: [u8; 32],
    /// The account key's 16-byte salt, base64.
    #[serde(with = "crate::base64_bytes")]
    pub key_salt: [u8; 16],
    /// The salt the play password is derived under, base64. Present even
    /// with no play password set, so it never changes underneath a board.
    #[serde(with = "crate::base64_bytes")]
    pub play_password_salt: [u8; 16],
    /// The salt the edit password is derived under, base64.
    #[serde(with = "crate::base64_bytes")]
    pub edit_password_salt: [u8; 16],
    /// The account's play password, if one is set. None by default.
    pub play_password: Option<String>,
    /// The account's edit password, if one is set. None by default.
    pub edit_password: Option<String>,
    /// Salts of account keys replaced by
    /// [`crate::request::ResetAccountKey`], oldest first, so a client can
    /// remove the retired entries from the devices it reaches.
    #[serde(with = "crate::base64_bytes::list")]
    pub previous_key_salts: Vec<[u8; 16]>,
    /// When the record last changed, f64 epoch seconds.
    pub updated_at: f64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;
    use alloc::vec;

    fn sample() -> AccountAccessInfo {
        AccountAccessInfo {
            key_secret: [1u8; 32],
            key_salt: [2u8; 16],
            play_password_salt: [3u8; 16],
            edit_password_salt: [4u8; 16],
            play_password: Some("friends".to_string()),
            edit_password: None,
            previous_key_salts: vec![[5u8; 16]],
            updated_at: 42.0,
        }
    }

    #[test]
    fn serde_round_trip() {
        let info = sample();
        let json = serde_json::to_string(&info).unwrap();
        let back: AccountAccessInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(back, info);
    }

    /// Pinned JSON literal: the deployed format is the contract.
    #[test]
    fn pinned_json_literal() {
        let info = AccountAccessInfo {
            key_secret: [0u8; 32],
            key_salt: [0u8; 16],
            play_password_salt: [0u8; 16],
            edit_password_salt: [0u8; 16],
            play_password: None,
            edit_password: Some("edit me".to_string()),
            previous_key_salts: vec![],
            updated_at: 0.0,
        };
        assert_eq!(
            serde_json::to_string(&info).unwrap(),
            r#"{"keySecret":"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=","keySalt":"AAAAAAAAAAAAAAAAAAAAAA==","playPasswordSalt":"AAAAAAAAAAAAAAAAAAAAAA==","editPasswordSalt":"AAAAAAAAAAAAAAAAAAAAAA==","playPassword":null,"editPassword":"edit me","previousKeySalts":[],"updatedAt":0.0}"#
        );
    }
}
