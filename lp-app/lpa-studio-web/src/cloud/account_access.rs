//! The account's device key and optional device passwords, from the cloud
//! into Studio core (plan D7, D8, D13).
//!
//! Studio core does not know the cloud. This bridge watches the session
//! and, on sign-in, fetches `GetAccountAccess` and hands the keys to core
//! (`AccessCommand::AccountKeys`), which caches them for offline unlock and
//! installs them on every device plugged in by USB. On sign-out it hands in
//! `None`. It also sends this browser's default key name
//! (`AccessCommand::BrowserNameDefault`): "Yona's Mac" signed in, "Chrome on
//! Mac" not — and, at boot, `BrowserNamePlaceholder` for a key that has
//! no name yet, so an offline first visit is not left "A browser".
//!
//! A guest account (`me.anonymous`) counts as signed out (D16): a
//! throwaway identity's key on a device would be a stray row. An
//! unreachable service changes nothing — the cached keys keep unlocking.
//!
//! Settings reads [`AccountAccessState`] and writes through
//! [`AccountAccessUi`]: setting a password, clearing one, resetting the key.
//! Each answer is the updated record, sent straight on to core.

use dioxus::prelude::*;
use lpa_studio_core::{AccessCommand, AccountKeys};
use lpc_cloud_api::request::{GetAccountAccess, ResetAccountKey, SetAccountPassword};
use lpc_cloud_api::{AccountAccessInfo, AccountPasswordTier, MeInfo};

use crate::app::home::browser_identity::{default_browser_name, detect_browser, detect_platform};
use crate::cloud::{CloudSession, FetchCloudPort};

/// What Settings knows about the account's device access.
#[derive(Clone, Debug, PartialEq)]
pub enum AccountAccessState {
    /// Nobody signed in (or a guest).
    SignedOut,
    /// Signed in; the record is on its way.
    Loading { name: String },
    Ready {
        /// The account's given name ("Yona").
        name: String,
        info: AccountAccessInfo,
    },
    /// Signed in, but the record did not come (offline): the cached keys
    /// still unlock, Settings just cannot show or change them now.
    Unavailable { name: String },
}

/// Settings' handle on the account's device access.
#[derive(Clone, Copy)]
pub struct AccountAccessUi {
    pub state: Signal<AccountAccessState>,
    /// Set (`Some`) or clear (`None`) the play or edit password.
    pub set_password: Callback<(AccountPasswordTier, Option<String>)>,
    /// "Reset account key…".
    pub reset_key: Callback<()>,
}

/// Provide the bridge. Call once, in the app, with core's access command
/// sink.
pub fn use_account_access_provider(on_access: Callback<AccessCommand>) -> AccountAccessUi {
    let session = use_context::<Signal<CloudSession>>();
    let mut state = use_signal(|| AccountAccessState::SignedOut);
    // A fresh key gets a real name at once ("Chrome on Mac"), even if the
    // service never answers; a key that already has one keeps it.
    use_hook(move || {
        on_access.call(AccessCommand::BrowserNamePlaceholder(default_browser_name(
            None,
            detect_browser(),
            detect_platform(),
        )));
    });
    // The session drives it: every settled answer re-sends the name, and a
    // change of who is signed in re-fetches (or clears) the keys.
    use_effect(move || {
        let current = session();
        let Some(signed_in) = signed_in_state(&current) else {
            return;
        };
        on_access.call(AccessCommand::BrowserNameDefault(default_browser_name(
            signed_in.as_deref(),
            detect_browser(),
            detect_platform(),
        )));
        match signed_in {
            None => {
                state.set(AccountAccessState::SignedOut);
                on_access.call(AccessCommand::AccountKeys(None));
            }
            Some(name) => {
                state.set(AccountAccessState::Loading { name: name.clone() });
                spawn(async move {
                    let answer =
                        lpa_cloud_client::call(&FetchCloudPort::new(), GetAccountAccess).await;
                    settle(answer, name, state, on_access);
                });
            }
        }
    });
    let set_password = Callback::new(
        move |(tier, password): (AccountPasswordTier, Option<String>)| {
            let Some(name) = state_name(&state.peek()) else {
                return;
            };
            spawn(async move {
                let answer = lpa_cloud_client::call(
                    &FetchCloudPort::new(),
                    SetAccountPassword { tier, password },
                )
                .await;
                settle(answer, name, state, on_access);
            });
        },
    );
    let reset_key = Callback::new(move |()| {
        let Some(name) = state_name(&state.peek()) else {
            return;
        };
        spawn(async move {
            let answer = lpa_cloud_client::call(&FetchCloudPort::new(), ResetAccountKey).await;
            settle(answer, name, state, on_access);
        });
    });
    use_context_provider(|| AccountAccessUi {
        state,
        set_password,
        reset_key,
    })
}

/// Settings' handle, when there is an app (not in a story).
pub fn use_account_access_ui() -> Option<AccountAccessUi> {
    try_consume_context::<AccountAccessUi>()
}

/// A record arrived (or did not): show it, and hand its keys to core.
fn settle<E: core::fmt::Display>(
    answer: Result<AccountAccessInfo, E>,
    name: String,
    mut state: Signal<AccountAccessState>,
    on_access: Callback<AccessCommand>,
) {
    match answer {
        Ok(info) => {
            on_access.call(AccessCommand::AccountKeys(Some(account_keys(&info, &name))));
            state.set(AccountAccessState::Ready { name, info });
        }
        Err(error) => {
            log::warn!("account device access unavailable: {error}");
            state.set(AccountAccessState::Unavailable { name });
        }
    }
}

/// `None` while the session is unsettled (pending, unreachable: change
/// nothing); `Some(None)` signed out; `Some(Some(name))` signed in.
fn signed_in_state(session: &CloudSession) -> Option<Option<String>> {
    match session {
        CloudSession::Pending | CloudSession::Unreachable => None,
        CloudSession::Anonymous { .. } => Some(None),
        CloudSession::SignedIn { me, .. } if me.anonymous => Some(None),
        CloudSession::SignedIn { me, .. } => Some(Some(account_name(me))),
    }
}

fn state_name(state: &AccountAccessState) -> Option<String> {
    match state {
        AccountAccessState::SignedOut => None,
        AccountAccessState::Loading { name }
        | AccountAccessState::Ready { name, .. }
        | AccountAccessState::Unavailable { name } => Some(name.clone()),
    }
}

/// The name the account's entries carry on a device: the given name, else
/// the first word of the display name.
pub fn account_name(me: &MeInfo) -> String {
    me.given_name
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .or_else(|| me.display_name.split_whitespace().next())
        .unwrap_or("Someone")
        .to_string()
}

/// The cloud's record as core's keys.
pub fn account_keys(info: &AccountAccessInfo, name: &str) -> AccountKeys {
    AccountKeys {
        key_secret: info.key_secret,
        key_salt: info.key_salt,
        play_password_salt: info.play_password_salt,
        edit_password_salt: info.edit_password_salt,
        play_password: info.play_password.clone(),
        edit_password: info.edit_password.clone(),
        previous_key_salts: info.previous_key_salts.clone(),
        account_name: name.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_account_is_named_by_its_given_name() {
        let mut me = me();
        assert_eq!(account_name(&me), "Yona");
        me.given_name = None;
        assert_eq!(account_name(&me), "Yona");
        me.display_name = String::new();
        assert_eq!(account_name(&me), "Someone");
    }

    #[test]
    fn a_guest_is_signed_out_and_an_unsettled_session_changes_nothing() {
        let mut guest = me();
        guest.anonymous = true;
        assert_eq!(
            signed_in_state(&CloudSession::SignedIn {
                me: guest,
                options: None
            }),
            Some(None)
        );
        assert_eq!(signed_in_state(&CloudSession::Pending), None);
        assert_eq!(signed_in_state(&CloudSession::Unreachable), None);
        assert_eq!(
            signed_in_state(&CloudSession::Anonymous { options: None }),
            Some(None)
        );
        let signed_in = signed_in_state(&CloudSession::SignedIn {
            me: me(),
            options: None,
        });
        assert_eq!(signed_in.flatten().as_deref(), Some("Yona"));
    }

    #[test]
    fn the_record_becomes_core_keys_under_the_account_name() {
        let info = AccountAccessInfo {
            key_secret: [1; 32],
            key_salt: [2; 16],
            play_password_salt: [3; 16],
            edit_password_salt: [4; 16],
            play_password: Some("camp".to_string()),
            edit_password: None,
            previous_key_salts: vec![[5; 16]],
            updated_at: 1.0,
        };
        let keys = account_keys(&info, "Yona");
        assert_eq!(keys.account_name, "Yona");
        assert_eq!(keys.key_salt, [2; 16]);
        assert_eq!(keys.play_password.as_deref(), Some("camp"));
        assert!(keys.owns_salt(&[5; 16]));
    }

    fn me() -> MeInfo {
        MeInfo {
            uid: lpc_history::PrefixedUid::mint(lpc_history::UidPrefix::User, &[1u8; 16]),
            email: "yona@example.com".to_string(),
            display_name: "Yona Appletree".to_string(),
            given_name: Some("Yona".to_string()),
            family_name: None,
            picture_url: None,
            provider_label: "Dev".to_string(),
            anonymous: false,
            created_at: 0.0,
        }
    }
}
