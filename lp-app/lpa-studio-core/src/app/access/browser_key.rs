//! This browser's own device key.
//!
//! Minted once, from the caller's randomness, the first time Studio starts
//! in this browser: a 32-byte secret and a 16-byte salt, used on every
//! device (plan D1). Plugging a device in by USB installs it there (the
//! physical link is access), and from then on this browser unlocks that
//! device over Bluetooth with no screen at all.
//!
//! Its **name** is the label people see in "Who has access" ("Yona's
//! MacBook"). The web edge supplies a default (`<given name>'s <platform>`
//! when signed in, else `<Browser> on <platform>`) until the user renames
//! it; a rename re-labels the entry on the next USB connect, because adding
//! an entry with the same salt replaces it.
//!
//! Persisted by the web edge under `lp.access.browser.v1`. Like the
//! remembered passwords beside it, the secret is login-equivalent and local
//! only (PQ8).

use lpc_access::{KEY_BYTES, SALT_BYTES, SecretKind, Tier};
use serde::{Deserialize, Serialize};

use super::key_holder::InstallableKey;

/// The name a key gets before the web edge has said anything better.
pub const FALLBACK_BROWSER_NAME: &str = "A browser";

/// See the module doc.
#[derive(Clone, PartialEq, Eq)]
pub struct BrowserKey {
    pub secret: [u8; KEY_BYTES],
    pub salt: [u8; SALT_BYTES],
    pub name: String,
    /// The user chose `name`: a new default from the web edge no longer
    /// replaces it.
    pub named_by_user: bool,
}

/// The persisted document (`lp.access.browser.v1`).
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BrowserKeyDoc {
    version: u32,
    #[serde(with = "lpc_access::base64_bytes")]
    secret: [u8; KEY_BYTES],
    #[serde(with = "lpc_access::base64_bytes")]
    salt: [u8; SALT_BYTES],
    name: String,
    #[serde(default)]
    named_by_user: bool,
}

impl BrowserKey {
    /// A new key from `random` (three draws: 32 secret bytes, 16 salt).
    pub fn mint(random: &dyn Fn() -> [u8; SALT_BYTES], name: impl Into<String>) -> Self {
        let mut secret = [0u8; KEY_BYTES];
        secret[..SALT_BYTES].copy_from_slice(&random());
        secret[SALT_BYTES..].copy_from_slice(&random());
        Self {
            secret,
            salt: random(),
            name: name.into(),
            named_by_user: false,
        }
    }

    /// Parse the stored document; anything unreadable is "no key yet" (a
    /// new one is minted, and the old one's entries stay on devices until
    /// someone removes them).
    pub fn from_json(json: &str) -> Option<Self> {
        let doc = serde_json::from_str::<BrowserKeyDoc>(json).ok()?;
        (doc.version == 1).then_some(Self {
            secret: doc.secret,
            salt: doc.salt,
            name: doc.name,
            named_by_user: doc.named_by_user,
        })
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(&BrowserKeyDoc {
            version: 1,
            secret: self.secret,
            salt: self.salt,
            name: self.name.clone(),
            named_by_user: self.named_by_user,
        })
        .unwrap_or_default()
    }

    /// Take the web edge's default name, unless the user named it. Returns
    /// whether the name changed.
    pub fn offer_default_name(&mut self, name: &str) -> bool {
        let name = name.trim();
        if self.named_by_user || name.is_empty() || self.name == name {
            return false;
        }
        self.name = name.to_string();
        true
    }

    /// Take `name` only while the key still wears the placeholder it was
    /// minted with ([`FALLBACK_BROWSER_NAME`]) — the edge's name for when
    /// it does not know yet who is signed in (offline), which must never
    /// overwrite a signed-in default like "Yona's Mac". Returns whether the
    /// name changed.
    pub fn offer_placeholder_name(&mut self, name: &str) -> bool {
        if self.name != FALLBACK_BROWSER_NAME {
            return false;
        }
        self.offer_default_name(name)
    }

    /// The user renamed it. A blank name is refused (the name is how people
    /// recognise it on a device). Returns whether the name changed.
    pub fn rename(&mut self, name: &str) -> bool {
        let name = name.trim();
        if name.is_empty() {
            return false;
        }
        let changed = self.name != name || !self.named_by_user;
        self.name = name.to_string();
        self.named_by_user = true;
        changed
    }

    /// The device entry this key installs: kind browser, tier edit, one
    /// iteration.
    pub fn installable(&self) -> InstallableKey {
        InstallableKey {
            label: self.name.clone(),
            kind: SecretKind::Browser,
            tier: Tier::Edit,
            salt: self.salt,
            iterations: 1,
            material: self.secret.to_vec(),
        }
    }
}

impl core::fmt::Debug for BrowserKey {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("BrowserKey")
            .field("name", &self.name)
            .field("named_by_user", &self.named_by_user)
            .field("secret", &"<redacted>")
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    fn counter() -> impl Fn() -> [u8; SALT_BYTES] {
        let next = Cell::new(0u8);
        move || {
            next.set(next.get() + 1);
            [next.get(); SALT_BYTES]
        }
    }

    #[test]
    fn a_minted_key_round_trips_and_never_prints_its_secret() {
        let key = BrowserKey::mint(&counter(), "Chrome on Mac");
        assert_eq!(key.secret[..16], [1; 16]);
        assert_eq!(key.secret[16..], [2; 16]);
        assert_eq!(key.salt, [3; 16]);
        assert_eq!(BrowserKey::from_json(&key.to_json()), Some(key.clone()));
        assert!(BrowserKey::from_json("{damaged").is_none());
        assert!(!format!("{key:?}").contains("[1"));
    }

    #[test]
    fn a_default_name_applies_until_the_user_renames() {
        let mut key = BrowserKey::mint(&counter(), FALLBACK_BROWSER_NAME);
        assert!(key.offer_default_name("Yona's MacBook"));
        assert!(!key.offer_default_name("Yona's MacBook"));
        assert!(!key.rename("  "));
        assert!(key.rename("Studio laptop"));
        assert!(!key.offer_default_name("Chrome on Mac"));
        assert_eq!(key.installable().label, "Studio laptop");
    }

    /// Offline at boot, the edge offers "<Browser> on <platform>": it names
    /// a fresh key, and never overwrites a signed-in default.
    #[test]
    fn a_placeholder_name_only_replaces_the_placeholder() {
        let mut key = BrowserKey::mint(&counter(), FALLBACK_BROWSER_NAME);
        assert!(key.offer_placeholder_name("Chrome on Mac"));
        assert_eq!(key.installable().label, "Chrome on Mac");
        let mut named = BrowserKey::mint(&counter(), FALLBACK_BROWSER_NAME);
        assert!(named.offer_default_name("Yona's Mac"));
        assert!(!named.offer_placeholder_name("Chrome on Mac"));
        assert_eq!(named.installable().label, "Yona's Mac");
    }
}
