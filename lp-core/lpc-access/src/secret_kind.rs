//! What holds a secret: a browser, an account, or a person who knows a
//! password.

use serde::{Deserialize, Serialize};

/// Who a [`crate::SecretEntry`] belongs to — what a person sees in "Who has
/// access", and nothing the board decides anything by.
///
/// The board treats every kind alike: the login verifies `k` and grants the
/// entry's tier whatever its kind. The kind is for people, and for a client
/// that wants to recognise its own entries.
///
/// - `browser` — a key a browser generated for itself (32 random bytes,
///   one salt the browser uses on every device, `iterations: 1`).
/// - `account` — the signed-in account's key, generated the same way and
///   shared by every browser signed in to that account.
/// - `password` — a password a person typed or was given. Every v1 entry
///   reads as this kind: before v2, a password was the only kind there was.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub enum SecretKind {
    /// A key one browser generated for itself.
    Browser,
    /// The signed-in account's key.
    Account,
    /// A typed or shared password.
    Password,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_ride_as_camel_case_strings() {
        assert_eq!(
            serde_json::to_string(&SecretKind::Browser).unwrap(),
            "\"browser\""
        );
        assert_eq!(
            serde_json::to_string(&SecretKind::Account).unwrap(),
            "\"account\""
        );
        assert_eq!(
            serde_json::from_str::<SecretKind>("\"password\"").unwrap(),
            SecretKind::Password
        );
        assert!(serde_json::from_str::<SecretKind>("\"Password\"").is_err());
    }
}
