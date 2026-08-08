//! The caller's own account record.

use alloc::string::String;
use lpc_history::PrefixedUid;
use serde::{Deserialize, Serialize};

/// Answers [`crate::request::GetMe`] and [`crate::request::UpdateMe`] (with
/// the updated record) — the caller's own account, as opposed to
/// [`crate::response::UserInfo`], which answers `WhoAmI` for any actor
/// including anonymous ones.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MeInfo {
    /// The account's uid (`usr…`).
    pub uid: PrefixedUid,
    /// The account's login email.
    pub email: String,
    /// The name shown across the UI: derived from `given_name`/`family_name`
    /// when set, the provider's name otherwise (P2 service policy — this
    /// crate only carries the computed value).
    pub display_name: String,
    /// Given (first) name, editable by the account holder.
    pub given_name: Option<String>,
    /// Family (last) name, editable by the account holder.
    pub family_name: Option<String>,
    /// Profile photo, hotlinked to the provider — never stored as bytes.
    pub picture_url: Option<String>,
    /// Human label of the connection this account signs in through
    /// ("Google", "Dev"). A label, not an enum: providers are config, not
    /// vocabulary (spike §4 ruling).
    pub provider_label: String,
    /// When the account was created.
    pub created_at: f64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;
    use lpc_history::UidPrefix;

    fn sample() -> MeInfo {
        MeInfo {
            uid: PrefixedUid::mint(UidPrefix::User, &[1u8; 16]),
            email: "yona@example.com".to_string(),
            display_name: "Yona Appletree".to_string(),
            given_name: Some("Yona".to_string()),
            family_name: Some("Appletree".to_string()),
            picture_url: Some("https://example.com/photo.jpg".to_string()),
            provider_label: "Google".to_string(),
            created_at: 42.0,
        }
    }

    #[test]
    fn serde_round_trip() {
        let info = sample();
        let json = serde_json::to_string(&info).unwrap();
        let back: MeInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(back, info);
    }

    #[test]
    fn serde_round_trip_no_names() {
        let info = MeInfo {
            given_name: None,
            family_name: None,
            picture_url: None,
            ..sample()
        };
        let json = serde_json::to_string(&info).unwrap();
        let back: MeInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(back, info);
    }

    /// Pinned JSON literal: the deployed format is the contract.
    #[test]
    fn pinned_json_literal() {
        let info = MeInfo {
            uid: PrefixedUid::mint(UidPrefix::User, &[0u8; 16]),
            email: "yona@example.com".to_string(),
            display_name: "Yona".to_string(),
            given_name: None,
            family_name: None,
            picture_url: None,
            provider_label: "Google".to_string(),
            created_at: 0.0,
        };
        assert_eq!(
            serde_json::to_string(&info).unwrap(),
            r#"{"uid":"usr0000000000000000","email":"yona@example.com","displayName":"Yona","givenName":null,"familyName":null,"pictureUrl":null,"providerLabel":"Google","createdAt":0.0}"#
        );
    }
}
