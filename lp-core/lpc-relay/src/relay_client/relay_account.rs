//! One account key the board holds, as the relay client needs it.

use lpc_access::{KEY_BYTES, SALT_BYTES, SecretEntry, SecretKind};

/// An account-key entry: its salt (the key id the hub looks the account up
/// by) and its `K` (what the proof is made with). Passed in by the caller
/// from the board's access store; the client never reads a file.
#[derive(Clone, PartialEq, Eq)]
pub struct RelayAccount {
    pub salt: [u8; SALT_BYTES],
    pub k: [u8; KEY_BYTES],
}

impl RelayAccount {
    /// The board's account entries, in store order: every
    /// [`SecretKind::Account`] entry, nothing else (a browser key or a
    /// password names no account the cloud knows).
    pub fn from_entries<'a>(
        entries: impl IntoIterator<Item = &'a SecretEntry>,
    ) -> alloc::vec::Vec<Self> {
        entries
            .into_iter()
            .filter(|entry| entry.kind == SecretKind::Account)
            .map(|entry| Self {
                salt: entry.salt,
                k: entry.k,
            })
            .collect()
    }
}

/// The key is login-equivalent: never in a log line.
impl core::fmt::Debug for RelayAccount {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RelayAccount").finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpc_access::Tier;

    #[test]
    fn only_account_entries_become_relay_accounts() {
        let entry = |kind, salt| SecretEntry {
            label: alloc::string::String::from("x"),
            kind,
            tier: Tier::Edit,
            salt: [salt; 16],
            iterations: 1,
            k: [salt; 32],
            added_at: None,
        };
        let entries = [
            entry(SecretKind::Browser, 1),
            entry(SecretKind::Account, 2),
            entry(SecretKind::Password, 3),
            entry(SecretKind::Account, 4),
        ];
        let accounts = RelayAccount::from_entries(&entries);
        let salts: alloc::vec::Vec<u8> = accounts.iter().map(|a| a.salt[0]).collect();
        assert_eq!(salts, [2, 4]);
        assert!(!alloc::format!("{:?}", accounts[0]).contains('2'));
    }
}
