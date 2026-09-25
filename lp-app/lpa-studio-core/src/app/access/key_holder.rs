//! The keys this browser holds, and the entries they install on a device.
//!
//! Every key holder — this browser, the signed-in account, each account
//! password — uses ONE salt on every device (plan D1). So a board's login
//! challenge, which offers each installed entry's salt, tells Studio which of
//! its own keys the board knows before any answer is sent: the automatic
//! unlock matches by salt and never guesses. The same salt names the entry in
//! the board's "Who has access" list, so Studio recognises its own rows and
//! adds only what is missing.
//!
//! A generated secret (the browser's, the account's) is 32 random bytes and
//! is installed at one PBKDF2 iteration: there is nothing to stretch. A
//! human password (an account password, a password typed for a device) is
//! installed at [`super::DEFAULT_KDF_ITERATIONS`].

use lpc_access::{SALT_BYTES, SecretEntry, SecretKind, Tier};

use super::account_keys::AccountKeys;
use super::browser_key::BrowserKey;

/// Who a held key belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyHolder {
    /// This browser's own key.
    Browser,
    /// The signed-in account's key.
    Account,
    /// One of the account's optional passwords, by the tier it grants.
    AccountPassword(Tier),
}

/// An entry to install on a device, before its key is derived: the secret
/// material and the cost it is derived at. Deriving is the expensive part
/// (PBKDF2 for a password), so it happens only for an entry the device is
/// actually missing, inside the conversation that installs it.
#[derive(Clone, PartialEq, Eq)]
pub struct InstallableKey {
    pub label: String,
    pub kind: SecretKind,
    pub tier: Tier,
    pub salt: [u8; SALT_BYTES],
    pub iterations: u32,
    /// The secret bytes, or a password's UTF-8 bytes.
    pub material: Vec<u8>,
}

impl InstallableKey {
    /// The device-store entry, `K` derived now.
    pub fn entry(&self, added_at: u64) -> SecretEntry {
        SecretEntry::from_password(
            self.label.clone(),
            self.tier,
            &self.material,
            self.salt,
            self.iterations,
        )
        .with_kind(self.kind)
        .with_added_at(added_at)
    }
}

/// The material is login-equivalent: never in a log line.
impl core::fmt::Debug for InstallableKey {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("InstallableKey")
            .field("label", &self.label)
            .field("kind", &self.kind)
            .field("tier", &self.tier)
            .field("iterations", &self.iterations)
            .field("material", &"<redacted>")
            .finish()
    }
}

/// One key this browser holds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HeldKey {
    pub holder: KeyHolder,
    pub key: InstallableKey,
}

impl HeldKey {
    pub fn salt(&self) -> [u8; SALT_BYTES] {
        self.key.salt
    }
}

/// Every key this browser holds, in the order a device lists them: the
/// browser, then the account key, then its play and edit passwords.
pub fn held_keys(browser: Option<&BrowserKey>, account: Option<&AccountKeys>) -> Vec<HeldKey> {
    let mut keys = Vec::new();
    if let Some(browser) = browser {
        keys.push(HeldKey {
            holder: KeyHolder::Browser,
            key: browser.installable(),
        });
    }
    if let Some(account) = account {
        keys.extend(account.held_keys());
    }
    keys
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_held_key_installs_an_entry_the_board_verifies_and_debug_hides_it() {
        let key = InstallableKey {
            label: "Yona's MacBook".to_string(),
            kind: SecretKind::Browser,
            tier: Tier::Edit,
            salt: [9; 16],
            iterations: 1,
            material: vec![7; 32],
        };
        let entry = key.entry(1_790_000_000);
        assert_eq!(entry.kind, SecretKind::Browser);
        assert_eq!(entry.added_at, Some(1_790_000_000));
        assert_eq!(entry.k, lpc_access::derive_login_key(&[7; 32], &[9; 16], 1));
        assert!(!format!("{key:?}").contains("[7"));
    }
}
