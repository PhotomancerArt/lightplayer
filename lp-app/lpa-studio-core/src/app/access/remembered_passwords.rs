//! Passwords this browser remembers for logging in to pieces over Bluetooth.
//!
//! BLE M6 (vision D10–D18, PQ8): Studio remembers every password that
//! logged in with "Remember on this browser" ticked and tries them — most
//! recently successful first — on the next piece that asks. They live in
//! `localStorage` under their own key (`lp.ble.passwords.v1`, the web
//! edge's business), local only, never synced: the threat model accepts it,
//! the way the agent's API keys already sit in `lp.settings.v1`.
//!
//! The list is small and bounded ([`MAX_REMEMBERED_PASSWORDS`]); the order is
//! the whole policy, so it is kept here as data rather than re-derived.

use serde::{Deserialize, Serialize};

/// How many passwords one browser remembers. Automatic unlock tries them
/// only when no key this browser holds is on the device, and at most
/// [`super::AUTO_LOGIN_ATTEMPTS`] of them; the rest are for the NEXT
/// device, which may know a different one.
pub const MAX_REMEMBERED_PASSWORDS: usize = 8;

/// The persisted list, most recently successful first.
#[derive(Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RememberedPasswords {
    /// Format version of this document; always 1.
    pub version: u32,
    /// Most recently successful first.
    pub passwords: Vec<RememberedPassword>,
}

/// One remembered password.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RememberedPassword {
    pub password: String,
    /// When it last logged in, epoch seconds (caller-supplied clock).
    pub last_ok_at: f64,
}

impl RememberedPasswords {
    /// Parse the stored document. Anything unreadable is "nothing
    /// remembered": a damaged list only ever costs a prompt.
    pub fn from_json(json: &str) -> Self {
        serde_json::from_str::<Self>(json).unwrap_or_default()
    }

    pub fn to_json(&self) -> String {
        let doc = Self {
            version: 1,
            passwords: self.passwords.clone(),
        };
        serde_json::to_string(&doc).unwrap_or_default()
    }

    /// Record a password that just logged in: to the front, once.
    pub fn remember(&mut self, password: &str, now_secs: f64) {
        if password.is_empty() {
            return;
        }
        self.passwords.retain(|known| known.password != password);
        self.passwords.insert(
            0,
            RememberedPassword {
                password: password.to_string(),
                last_ok_at: now_secs,
            },
        );
        self.passwords.truncate(MAX_REMEMBERED_PASSWORDS);
    }

    /// Forget every remembered password (Settings' "Forget remembered
    /// passwords").
    pub fn forget_all(&mut self) {
        self.passwords.clear();
    }

    pub fn len(&self) -> usize {
        self.passwords.len()
    }

    pub fn is_empty(&self) -> bool {
        self.passwords.is_empty()
    }

    /// The passwords in the order login tries them, most recent first.
    pub fn in_order(&self) -> impl Iterator<Item = &str> {
        self.passwords.iter().map(|known| known.password.as_str())
    }
}

/// A password never reaches a log line or a panic message.
impl core::fmt::Debug for RememberedPasswords {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RememberedPasswords")
            .field("count", &self.passwords.len())
            .finish()
    }
}

impl core::fmt::Debug for RememberedPassword {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RememberedPassword")
            .field("password", &"<redacted>")
            .field("last_ok_at", &self.last_ok_at)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_most_recent_success_is_tried_first_and_kept_once() {
        let mut list = RememberedPasswords::default();
        list.remember("camp", 1.0);
        list.remember("mine", 2.0);
        list.remember("camp", 3.0);
        assert_eq!(list.in_order().collect::<Vec<_>>(), ["camp", "mine"]);
    }

    #[test]
    fn the_list_is_bounded_and_round_trips() {
        let mut list = RememberedPasswords::default();
        for n in 0..20 {
            list.remember(&format!("pw{n}"), f64::from(n));
        }
        assert_eq!(list.len(), MAX_REMEMBERED_PASSWORDS);
        assert_eq!(list.in_order().next(), Some("pw19"));
        let back = RememberedPasswords::from_json(&list.to_json());
        assert_eq!(back.passwords, list.passwords);
    }

    #[test]
    fn a_damaged_document_remembers_nothing_and_debug_never_prints_one() {
        assert!(RememberedPasswords::from_json("{not json").is_empty());
        let mut list = RememberedPasswords::default();
        list.remember("hunter2", 1.0);
        assert!(!format!("{list:?}").contains("hunter2"));
        assert!(!format!("{:?}", list.passwords[0]).contains("hunter2"));
    }
}
