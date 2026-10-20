//! The home page's sign-in line: "Sign in to unlock your boards from any
//! browser." and the existing sign-in word (PD10).
//!
//! It shows for someone who has something to keep and no account to keep it
//! in: a signed-out visitor, or a guest, who is not a newcomer (a first
//! visit has nothing yet, so the line would be noise). The rule is one pure
//! function, [`sign_in_prompt_shown`]. The component reads the session the
//! chrome already holds (`Signal<CloudSession>`) with `try_consume_context`
//! and renders nothing without it, the house rule: stories and host mounts
//! provide none.

use dioxus::prelude::*;

use crate::app::layout::cloud_account::{
    SignInAffordance, SignInLink, SignInMenu, sign_in_affordance,
};
use crate::cloud::session_state::CloudSession;

/// The line's words.
pub(crate) const SIGN_IN_WORDS: &str = "Sign in to unlock your boards from any browser.";

/// Where signing in sends the person back to: the home page.
const NEXT: &str = "/";

/// The sign-in line, when it is wanted.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn SignInPrompt(
    /// A first visit: no board and no project yet.
    newcomer: bool,
) -> Element {
    let Some(session) = try_consume_context::<Signal<CloudSession>>() else {
        return rsx! {};
    };
    let session = session();
    if !sign_in_prompt_shown(&session, newcomer) {
        return rsx! {};
    }
    let Some(options) = session.login_options().cloned() else {
        return rsx! {};
    };
    // A deployment with no connection configured has nowhere to send the
    // word: better no line than one that leads nowhere.
    let word = match sign_in_affordance(&options, NEXT) {
        SignInAffordance::Direct(href) => rsx! {
            SignInLink { href }
        },
        SignInAffordance::Chooser => rsx! {
            SignInMenu { options, next: NEXT.to_string() }
        },
        SignInAffordance::Nothing => return rsx! {},
    };
    rsx! {
        div { class: "tw:flex tw:flex-wrap tw:items-center tw:gap-2.5 tw:rounded-md tw:border tw:border-border tw:bg-card tw:px-3 tw:py-1.5 tw:text-[12.5px] tw:text-muted-foreground",
            span { "{SIGN_IN_WORDS}" }
            {word}
        }
    }
}

/// Whether the line shows: the session is signed out, or a guest's, and
/// the service told us how to sign in, and this is not a first visit.
///
/// Not while the answer is outstanding (`Pending`), not when the service
/// could not be reached (`Unreachable`, silent by design), not for a real
/// account, and not when the sign-in options never landed (a word with
/// nowhere to go).
pub(crate) fn sign_in_prompt_shown(session: &CloudSession, newcomer: bool) -> bool {
    if newcomer {
        return false;
    }
    match session {
        CloudSession::Anonymous { options } => options.is_some(),
        CloudSession::SignedIn { me, options } => me.anonymous && options.is_some(),
        CloudSession::Pending | CloudSession::Unreachable => false,
    }
}

#[cfg(test)]
mod tests {
    use lpc_cloud_api::{LoginOptionsInfo, MeInfo, OidcOption};
    use lpc_history::{PrefixedUid, UidPrefix};

    use super::*;

    #[test]
    fn it_shows_for_a_signed_out_visitor_and_a_guest_who_are_not_new() {
        assert!(sign_in_prompt_shown(&signed_out(), false));
        assert!(sign_in_prompt_shown(&guest(), false));
    }

    #[test]
    fn a_newcomer_never_sees_it() {
        for session in every_session() {
            assert!(!sign_in_prompt_shown(&session, true));
        }
    }

    #[test]
    fn a_real_account_a_pending_answer_and_an_unreachable_service_see_none() {
        assert!(!sign_in_prompt_shown(&account(), false));
        assert!(!sign_in_prompt_shown(&CloudSession::Pending, false));
        assert!(!sign_in_prompt_shown(&CloudSession::Unreachable, false));
    }

    #[test]
    fn options_that_never_landed_leave_a_word_with_nowhere_to_go() {
        assert!(!sign_in_prompt_shown(
            &CloudSession::Anonymous { options: None },
            false
        ));
        assert!(!sign_in_prompt_shown(
            &CloudSession::SignedIn {
                me: me(true),
                options: None
            },
            false
        ));
    }

    #[test]
    fn the_words_are_the_ones_the_page_uses() {
        assert_eq!(
            SIGN_IN_WORDS,
            "Sign in to unlock your boards from any browser."
        );
    }

    fn every_session() -> Vec<CloudSession> {
        vec![
            CloudSession::Pending,
            CloudSession::Unreachable,
            CloudSession::Anonymous { options: None },
            signed_out(),
            guest(),
            account(),
        ]
    }

    fn signed_out() -> CloudSession {
        CloudSession::Anonymous {
            options: Some(options()),
        }
    }

    fn guest() -> CloudSession {
        CloudSession::SignedIn {
            me: me(true),
            options: Some(options()),
        }
    }

    fn account() -> CloudSession {
        CloudSession::SignedIn {
            me: me(false),
            options: Some(options()),
        }
    }

    fn options() -> LoginOptionsInfo {
        LoginOptionsInfo {
            oidc: vec![OidcOption {
                id: "google".to_string(),
                label: "Google".to_string(),
                start_path: "/auth/google".to_string(),
            }],
            dev_picker: None,
        }
    }

    fn me(anonymous: bool) -> MeInfo {
        MeInfo {
            uid: PrefixedUid::mint(UidPrefix::User, &[7; 16]),
            email: "someone@example.com".to_string(),
            display_name: "Someone".to_string(),
            given_name: None,
            family_name: None,
            picture_url: None,
            provider_label: "Google".to_string(),
            created_at: 1_752_000_000_000.0,
            anonymous,
        }
    }
}
