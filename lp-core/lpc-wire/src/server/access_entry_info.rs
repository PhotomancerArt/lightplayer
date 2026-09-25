//! One row of "Who has access", as the board reports it.

use alloc::string::String;
use lpc_access::{SALT_BYTES, SecretEntry, SecretKind, Tier};
use serde::{Deserialize, Serialize};

/// What a client may know about one installed secret: everything a person
/// needs to recognise and remove it, and **nothing that logs in**.
///
/// Never `k` (login-equivalent) and never `iterations` (not a person's
/// concern). `salt` is the entry's identity: a holder uses one salt on
/// every device, so a client recognises its own rows by it, and
/// [`crate::ClientRequest::AccessRemove`] names a row by it. The salt is
/// already public — every login challenge offers it — so listing it tells
/// an edit-tier client nothing a stranger in range could not learn.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccessEntryInfo {
    /// The name people see ("Yona's MacBook", "friends").
    pub label: String,
    /// A browser, an account, or a password.
    pub kind: SecretKind,
    /// What a login with it grants.
    pub tier: Tier,
    /// The entry's salt, base64 — its identity.
    #[serde(with = "lpc_access::base64_bytes")]
    pub salt: [u8; SALT_BYTES],
    /// When it was added (epoch seconds), when the adding client said.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub added_at: Option<u64>,
}

impl From<&SecretEntry> for AccessEntryInfo {
    fn from(entry: &SecretEntry) -> Self {
        Self {
            label: entry.label.clone(),
            kind: entry.kind,
            tier: entry.tier,
            salt: entry.salt,
            added_at: entry.added_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn carries_the_salt_and_never_the_key() {
        let entry = SecretEntry::from_password("friends", Tier::Play, b"pw", [1u8; 16], 60_000)
            .with_added_at(1_790_000_000);
        let json = crate::json::to_string(&AccessEntryInfo::from(&entry)).unwrap();
        assert_eq!(
            json,
            r#"{"label":"friends","kind":"password","tier":"play","salt":"AQEBAQEBAQEBAQEBAQEBAQ==","addedAt":1790000000}"#
        );
    }
}
