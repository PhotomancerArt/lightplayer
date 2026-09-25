//! An account's device key and optional account passwords.

use alloc::string::String;
use alloc::vec::Vec;
use lpc_cloud_api::AccountAccessInfo;
use lpc_history::PrefixedUid;

/// How many retired account-key salts [`AccountAccess::rotate_key`] keeps.
/// Enough for a client that has not reached a device in a while to still
/// recognize and remove the entries a few resets left behind; bounded so
/// the record cannot grow without limit.
pub const MAX_PREVIOUS_KEY_SALTS: usize = 4;

/// What lets a phone signed in to an account unlock every device that
/// account was installed on, without having been plugged in itself.
///
/// One generated **account key** (a 32-byte secret and a 16-byte salt, used
/// on every device), and two **optional account passwords**, play and
/// edit, none by default. Each password has a salt of its own because a
/// board keys its access entries by salt: the salt is what names a
/// password's entry on a device. Salts are fixed for the record's life, so
/// changing a password replaces its device entry rather than adding one.
///
/// The passwords are stored readable, so Settings can show them. They are
/// shareable device passwords, like a Wi-Fi password — not account
/// credentials — and encrypting them at rest is out of scope.
///
/// Every random byte here comes from the
/// [`IdMint`](crate::ports::id_mint::IdMint) port; the record is minted
/// lazily, on the account's first ask.
#[derive(Debug, Clone, PartialEq)]
pub struct AccountAccess {
    /// The account this record belongs to (`usr…`).
    pub user: PrefixedUid,
    /// The account key's secret.
    pub key_secret: [u8; 32],
    /// The account key's salt.
    pub key_salt: [u8; 16],
    /// The salt the play password is derived under.
    pub play_password_salt: [u8; 16],
    /// The salt the edit password is derived under.
    pub edit_password_salt: [u8; 16],
    /// The play password, if one is set.
    pub play_password: Option<String>,
    /// The edit password, if one is set.
    pub edit_password: Option<String>,
    /// Salts of account keys replaced by a reset, oldest first, at most
    /// [`MAX_PREVIOUS_KEY_SALTS`] — so a client can remove the retired
    /// entries from the devices it reaches.
    pub previous_key_salts: Vec<[u8; 16]>,
    /// When the record last changed, f64 epoch seconds from the clock port.
    pub updated_at: f64,
}

impl AccountAccess {
    /// Replace the account key, keeping the old salt on
    /// `previous_key_salts` (newest last, oldest dropped past
    /// [`MAX_PREVIOUS_KEY_SALTS`]).
    pub fn rotate_key(&mut self, key_secret: [u8; 32], key_salt: [u8; 16], now: f64) {
        self.previous_key_salts.push(self.key_salt);
        let excess = self
            .previous_key_salts
            .len()
            .saturating_sub(MAX_PREVIOUS_KEY_SALTS);
        self.previous_key_salts.drain(..excess);
        self.key_secret = key_secret;
        self.key_salt = key_salt;
        self.updated_at = now;
    }

    /// The wire form of this record.
    pub fn info(&self) -> AccountAccessInfo {
        AccountAccessInfo {
            key_secret: self.key_secret,
            key_salt: self.key_salt,
            play_password_salt: self.play_password_salt,
            edit_password_salt: self.edit_password_salt,
            play_password: self.play_password.clone(),
            edit_password: self.edit_password.clone(),
            previous_key_salts: self.previous_key_salts.clone(),
            updated_at: self.updated_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;
    use lpc_history::UidPrefix;

    fn record() -> AccountAccess {
        AccountAccess {
            user: PrefixedUid::mint(UidPrefix::User, &[1u8; 16]),
            key_secret: [0u8; 32],
            key_salt: [0u8; 16],
            play_password_salt: [0xa0; 16],
            edit_password_salt: [0xe0; 16],
            play_password: None,
            edit_password: None,
            previous_key_salts: vec![],
            updated_at: 1.0,
        }
    }

    #[test]
    fn rotating_records_the_old_salt_and_keeps_the_last_four() {
        let mut access = record();
        for n in 1..=6u8 {
            access.rotate_key([n; 32], [n; 16], f64::from(n));
        }
        assert_eq!(access.key_salt, [6u8; 16]);
        assert_eq!(access.key_secret, [6u8; 32]);
        assert_eq!(
            access.previous_key_salts,
            vec![[2u8; 16], [3u8; 16], [4u8; 16], [5u8; 16]],
            "oldest first, capped at four"
        );
        assert_eq!(access.updated_at, 6.0);
    }

    #[test]
    fn rotating_leaves_the_password_salts_alone() {
        let mut access = record();
        access.rotate_key([9u8; 32], [9u8; 16], 2.0);
        assert_eq!(access.play_password_salt, [0xa0; 16]);
        assert_eq!(access.edit_password_salt, [0xe0; 16]);
    }
}
