//! The signed-in account's device keys (plan D7, D8).
//!
//! The cloud keeps one generated account key (a secret and a salt, used on
//! every device) and two optional account passwords, play and edit, each
//! with a salt of its own (the board names entries by salt). Studio core
//! does not know the cloud: the web edge fetches them (`GetAccountAccess`)
//! and hands them in with [`super::AccessCommand::AccountKeys`], and `None`
//! on sign-out.
//!
//! They are cached in `localStorage` (`lp.access.account.v1`), so a phone
//! with no internet at camp still unlocks every device the account was
//! installed on. The cache is cleared on sign-out.

use lpc_access::{KEY_BYTES, SALT_BYTES, SecretKind, Tier};
use serde::{Deserialize, Serialize};

use super::key_holder::{HeldKey, InstallableKey, KeyHolder};
use super::login_key_cache::DEFAULT_KDF_ITERATIONS;

/// See the module doc.
#[derive(Clone, PartialEq, Eq)]
pub struct AccountKeys {
    pub key_secret: [u8; KEY_BYTES],
    pub key_salt: [u8; SALT_BYTES],
    pub play_password_salt: [u8; SALT_BYTES],
    pub edit_password_salt: [u8; SALT_BYTES],
    pub play_password: Option<String>,
    pub edit_password: Option<String>,
    /// Salts of account keys the account has since reset: their entries are
    /// removed from each device Studio reaches by USB.
    pub previous_key_salts: Vec<[u8; SALT_BYTES]>,
    /// The name the entries are labelled with (the account's given name).
    pub account_name: String,
}

impl AccountKeys {
    /// Every key the account holds: its key, then whichever passwords are
    /// set.
    pub fn held_keys(&self) -> Vec<HeldKey> {
        let mut keys = vec![HeldKey {
            holder: KeyHolder::Account,
            key: InstallableKey {
                label: format!("{}'s account", self.account_name),
                kind: SecretKind::Account,
                tier: Tier::Edit,
                salt: self.key_salt,
                iterations: 1,
                material: self.key_secret.to_vec(),
            },
        }];
        for (tier, password, salt) in [
            (Tier::Play, &self.play_password, self.play_password_salt),
            (Tier::Edit, &self.edit_password, self.edit_password_salt),
        ] {
            let Some(password) = password.as_ref().filter(|p| !p.is_empty()) else {
                continue;
            };
            keys.push(HeldKey {
                holder: KeyHolder::AccountPassword(tier),
                key: InstallableKey {
                    label: format!(
                        "{}'s {} password",
                        self.account_name,
                        super::tier_word(tier)
                    ),
                    kind: SecretKind::Password,
                    tier,
                    salt,
                    iterations: DEFAULT_KDF_ITERATIONS,
                    material: password.as_bytes().to_vec(),
                },
            });
        }
        keys
    }

    /// Whether `salt` names one of the account's entries (current or
    /// retired key, or either password).
    pub fn owns_salt(&self, salt: &[u8; SALT_BYTES]) -> bool {
        *salt == self.key_salt
            || *salt == self.play_password_salt
            || *salt == self.edit_password_salt
            || self.previous_key_salts.contains(salt)
    }

    /// Parse the cached document; anything unreadable is "not cached".
    pub fn from_json(json: &str) -> Option<Self> {
        let doc = serde_json::from_str::<AccountKeysDoc>(json).ok()?;
        (doc.version == 1).then(|| doc.into())
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(&AccountKeysDoc::from(self)).unwrap_or_default()
    }
}

/// Passwords and secrets never reach a log line.
impl core::fmt::Debug for AccountKeys {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("AccountKeys")
            .field("account_name", &self.account_name)
            .field(
                "play_password",
                &self.play_password.as_ref().map(|_| "<set>"),
            )
            .field(
                "edit_password",
                &self.edit_password.as_ref().map(|_| "<set>"),
            )
            .field("previous_key_salts", &self.previous_key_salts.len())
            .finish_non_exhaustive()
    }
}

/// One salt, as base64 in the cached document.
#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(transparent)]
struct Salt(#[serde(with = "lpc_access::base64_bytes")] [u8; SALT_BYTES]);

/// The cached document (`lp.access.account.v1`).
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AccountKeysDoc {
    version: u32,
    #[serde(with = "lpc_access::base64_bytes")]
    key_secret: [u8; KEY_BYTES],
    key_salt: Salt,
    play_password_salt: Salt,
    edit_password_salt: Salt,
    #[serde(default)]
    play_password: Option<String>,
    #[serde(default)]
    edit_password: Option<String>,
    #[serde(default)]
    previous_key_salts: Vec<Salt>,
    account_name: String,
}

impl From<&AccountKeys> for AccountKeysDoc {
    fn from(keys: &AccountKeys) -> Self {
        Self {
            version: 1,
            key_secret: keys.key_secret,
            key_salt: Salt(keys.key_salt),
            play_password_salt: Salt(keys.play_password_salt),
            edit_password_salt: Salt(keys.edit_password_salt),
            play_password: keys.play_password.clone(),
            edit_password: keys.edit_password.clone(),
            previous_key_salts: keys.previous_key_salts.iter().copied().map(Salt).collect(),
            account_name: keys.account_name.clone(),
        }
    }
}

impl From<AccountKeysDoc> for AccountKeys {
    fn from(doc: AccountKeysDoc) -> Self {
        Self {
            key_secret: doc.key_secret,
            key_salt: doc.key_salt.0,
            play_password_salt: doc.play_password_salt.0,
            edit_password_salt: doc.edit_password_salt.0,
            play_password: doc.play_password,
            edit_password: doc.edit_password,
            previous_key_salts: doc.previous_key_salts.into_iter().map(|s| s.0).collect(),
            account_name: doc.account_name,
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn account(play: Option<&str>) -> AccountKeys {
        AccountKeys {
            key_secret: [40; 32],
            key_salt: [41; 16],
            play_password_salt: [42; 16],
            edit_password_salt: [43; 16],
            play_password: play.map(str::to_string),
            edit_password: None,
            previous_key_salts: vec![[44; 16]],
            account_name: "Yona".to_string(),
        }
    }

    #[test]
    fn the_account_holds_its_key_and_only_the_passwords_that_are_set() {
        let keys = account(Some("glitter")).held_keys();
        let labels: Vec<&str> = keys.iter().map(|k| k.key.label.as_str()).collect();
        assert_eq!(labels, ["Yona's account", "Yona's play password"]);
        assert_eq!(keys[0].key.iterations, 1);
        assert_eq!(keys[1].key.iterations, DEFAULT_KDF_ITERATIONS);
        assert_eq!(keys[1].key.salt, [42; 16]);
        assert_eq!(account(None).held_keys().len(), 1);
        assert!(account(None).owns_salt(&[44; 16]));
    }

    #[test]
    fn the_cache_round_trips_and_debug_prints_no_password() {
        let keys = account(Some("glitter"));
        assert_eq!(AccountKeys::from_json(&keys.to_json()), Some(keys.clone()));
        assert!(AccountKeys::from_json("nope").is_none());
        assert!(!format!("{keys:?}").contains("glitter"));
    }
}
