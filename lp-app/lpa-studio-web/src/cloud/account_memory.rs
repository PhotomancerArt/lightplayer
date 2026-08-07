//! The accounts this browser has seen signed in, for the switch-account rows.
//!
//! Multi-account here is **bones, not machinery** (spike §5 ruling): the
//! server holds one session at a time, so switching is a re-auth. What this
//! module remembers is only what the dropdown needs to *offer* the switch —
//! a name, a face, and which provider to send you back through. No token, no
//! session, nothing that grants anything.
//!
//! Stored as JSON under [`ACCOUNTS_STORAGE_KEY`] in `localStorage`, the same
//! plain-text posture as the settings layer (`crate::settings_io`), and read
//! with the same tolerance: a document this build cannot parse is a document
//! that isn't there. This list is a convenience, and losing it costs a click.
//!
//! The list logic is pure and host-tested; only [`load`] and [`remember`]
//! touch the browser.

use lpc_cloud_api::MeInfo;
use serde::{Deserialize, Serialize};

/// localStorage key holding the remembered-accounts list.
pub const ACCOUNTS_STORAGE_KEY: &str = "lp_accounts";

/// How many accounts the list keeps. Five is the dropdown's appetite, not a
/// storage limit: past that the group stops being a shortcut and becomes a
/// directory.
pub const MAX_REMEMBERED_ACCOUNTS: usize = 5;

/// One account this browser has been signed into.
///
/// Not a session and not a credential — everything here is already public to
/// whoever is sitting at this browser, and none of it is accepted back by the
/// server as proof of anything.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RememberedAccount {
    /// The account's login email — the identity of the row, and what a
    /// re-auth is aimed at.
    pub email: String,
    /// The name to show.
    pub display_name: String,
    /// Provider-hosted photo, hotlinked. Never bytes.
    pub picture_url: Option<String>,
    /// Human label of the connection it signs in through ("Google", "Dev").
    pub provider_label: String,
    /// When it was last seen signed in, epoch milliseconds (`Date.now()`).
    /// Ordering is by list position; this is for display ("last week").
    pub last_seen: f64,
}

impl RememberedAccount {
    /// The row a signed-in account leaves behind.
    pub fn of(me: &MeInfo, last_seen: f64) -> Self {
        Self {
            email: me.email.clone(),
            display_name: me.display_name.clone(),
            picture_url: me.picture_url.clone(),
            provider_label: me.provider_label.clone(),
            last_seen,
        }
    }
}

/// Read a stored list. Anything unreadable — absent, truncated, written by a
/// build with another shape — reads as an empty list, never a panic.
pub fn parse(json: &str) -> Vec<RememberedAccount> {
    serde_json::from_str(json).unwrap_or_default()
}

/// Put `entry` at the front, dropping any earlier row for the same email,
/// and keep at most [`MAX_REMEMBERED_ACCOUNTS`].
///
/// Emails match exactly: the service keys accounts by the string the provider
/// hands it, and inventing a normalization here that the server does not
/// share would let one row stand for two accounts.
pub fn upsert(accounts: &[RememberedAccount], entry: RememberedAccount) -> Vec<RememberedAccount> {
    let superseded = entry.email.clone();
    let mut next = Vec::with_capacity(accounts.len() + 1);
    next.push(entry);
    next.extend(
        accounts
            .iter()
            .filter(|account| account.email != superseded)
            .cloned(),
    );
    next.truncate(MAX_REMEMBERED_ACCOUNTS);
    next
}

/// The remembered list, most-recent first.
pub fn load() -> Vec<RememberedAccount> {
    let Some(storage) = local_storage() else {
        return Vec::new();
    };
    match storage.get_item(ACCOUNTS_STORAGE_KEY) {
        Ok(Some(json)) => parse(&json),
        _ => Vec::new(),
    }
}

/// Record a signed-in account at the front of the list.
///
/// `last_seen` is epoch milliseconds; the caller supplies it so the list
/// logic stays a pure function of its inputs.
pub fn remember(me: &MeInfo, last_seen: f64) {
    let next = upsert(&load(), RememberedAccount::of(me, last_seen));
    let Some(storage) = local_storage() else {
        return;
    };
    match serde_json::to_string(&next) {
        Ok(json) => {
            if let Err(error) = storage.set_item(ACCOUNTS_STORAGE_KEY, &json) {
                log::warn!("account list not saved to localStorage: {error:?}");
            }
        }
        Err(error) => log::warn!("account list not saved (unencodable): {error}"),
    }
}

/// `localStorage`, or `None` where it is blocked (private mode, a host that
/// refuses storage). Same tolerance as `settings_io`.
fn local_storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok()?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn account(email: &str, last_seen: f64) -> RememberedAccount {
        RememberedAccount {
            email: email.to_string(),
            display_name: "Yona Appletree".to_string(),
            picture_url: Some("https://example.com/photo.jpg".to_string()),
            provider_label: "Google".to_string(),
            last_seen,
        }
    }

    fn emails(accounts: &[RememberedAccount]) -> Vec<&str> {
        accounts.iter().map(|a| a.email.as_str()).collect()
    }

    #[test]
    fn an_unseen_account_goes_to_the_front() {
        let list = upsert(&[account("a@x.com", 1.0)], account("b@x.com", 2.0));
        assert_eq!(emails(&list), ["b@x.com", "a@x.com"]);
    }

    /// Signing back into an account already in the list moves it to the
    /// front rather than duplicating it — and its details refresh (the
    /// provider photo changes every login).
    #[test]
    fn a_returning_account_moves_up_and_refreshes() {
        let stored = vec![account("a@x.com", 1.0), account("b@x.com", 2.0)];
        let mut fresh = account("b@x.com", 9.0);
        fresh.display_name = "Yona A.".to_string();
        let list = upsert(&stored, fresh);
        assert_eq!(emails(&list), ["b@x.com", "a@x.com"]);
        assert_eq!(list[0].display_name, "Yona A.");
        assert_eq!(list[0].last_seen, 9.0);
    }

    #[test]
    fn the_list_is_capped_and_drops_the_oldest() {
        let mut list = Vec::new();
        for index in 0..8 {
            list = upsert(&list, account(&format!("{index}@x.com"), index as f64));
        }
        assert_eq!(list.len(), MAX_REMEMBERED_ACCOUNTS);
        assert_eq!(
            emails(&list),
            ["7@x.com", "6@x.com", "5@x.com", "4@x.com", "3@x.com"]
        );
    }

    #[test]
    fn a_corrupt_document_reads_as_an_empty_list() {
        assert!(parse("").is_empty());
        assert!(parse("{").is_empty());
        assert!(parse("null").is_empty());
        assert!(parse(r#"{"email":"a@x.com"}"#).is_empty());
        assert!(parse(r#"[{"email":"a@x.com"}]"#).is_empty());
    }

    /// The stored shape is a format this build will have to read back after
    /// a deploy, so it is pinned like any other persisted document.
    #[test]
    fn pinned_json_literal() {
        let list = vec![RememberedAccount {
            email: "yona@example.com".to_string(),
            display_name: "Yona".to_string(),
            picture_url: None,
            provider_label: "Google".to_string(),
            last_seen: 0.0,
        }];
        let json = serde_json::to_string(&list).unwrap();
        assert_eq!(
            json,
            r#"[{"email":"yona@example.com","displayName":"Yona","pictureUrl":null,"providerLabel":"Google","lastSeen":0.0}]"#
        );
        assert_eq!(parse(&json), list);
    }

    #[test]
    fn a_signed_in_account_becomes_a_row() {
        let me = MeInfo {
            uid: lpc_history::PrefixedUid::mint(lpc_history::UidPrefix::User, &[1u8; 16]),
            email: "yona@example.com".to_string(),
            display_name: "Yona Appletree".to_string(),
            given_name: Some("Yona".to_string()),
            family_name: Some("Appletree".to_string()),
            picture_url: Some("https://example.com/photo.jpg".to_string()),
            provider_label: "Google".to_string(),
            created_at: 1.0,
        };
        let row = RememberedAccount::of(&me, 42.0);
        assert_eq!(row.email, me.email);
        assert_eq!(row.display_name, me.display_name);
        assert_eq!(row.picture_url, me.picture_url);
        assert_eq!(row.provider_label, me.provider_label);
        assert_eq!(row.last_seen, 42.0);
    }
}
