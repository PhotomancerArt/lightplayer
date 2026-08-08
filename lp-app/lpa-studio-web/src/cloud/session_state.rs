//! Who the cloud service says we are, as one signal in context.
//!
//! The chrome, the account dropdown and the `/account` page all render from
//! this and nothing else, so there is exactly one `whoami` per page load and
//! no component owns a fetch.
//!
//! # The four states, and why `Unreachable` is silent
//!
//! [`CloudSession::Pending`] exists so the chrome can shimmer rather than
//! render "Sign in" and then pop it into an avatar a moment later.
//! [`CloudSession::Unreachable`] exists so a viewer following a share link
//! from a machine that cannot reach the service — or a tab left open across a
//! redeploy that no longer speaks this vocabulary — sees *nothing*
//! account-shaped, never an error badge. Signing in is not what they came
//! for, and a failure to answer a question nobody asked is not news.
//!
//! # House rule
//!
//! Consumers `try_consume_context` and render inert without it (`web_app.rs`
//! provider notes): stories provide no context, and none of this may make a
//! component undrawable.

use dioxus::prelude::*;
use lpa_cloud_client::cloud_port::CloudPort;
use lpc_cloud_api::request::{GetMe, LoginOptions, WhoAmI};
use lpc_cloud_api::{Actor, LoginOptionsInfo, MeInfo};

use crate::cloud::FetchCloudPort;
use crate::cloud::account_memory;

/// What the service answered about this browser.
#[derive(Debug, Clone, PartialEq)]
pub enum CloudSession {
    /// `whoami` is in flight — the boot state, and what a refresh returns to.
    Pending,
    /// Reached, nobody signed in. `options` is how to sign in, as the server
    /// has it configured; `None` when that second call did not land (a
    /// sign-in affordance with nowhere to go is worse than none).
    Anonymous { options: Option<LoginOptionsInfo> },
    /// Reached, signed in. `options` rides along because the identity
    /// dropdown's switch and add-account rows are sign-in links too (spike
    /// §5: a switch IS a re-auth), and the client never hard-codes a
    /// provider path; `None` when that call did not land, and those rows
    /// are then omitted rather than pointed nowhere.
    SignedIn {
        me: MeInfo,
        options: Option<LoginOptionsInfo>,
    },
    /// Not reached, or reached and unintelligible. The chrome stays quiet.
    Unreachable,
}

impl CloudSession {
    /// The signed-in account, if there is one.
    pub fn me(&self) -> Option<&MeInfo> {
        match self {
            CloudSession::SignedIn { me, .. } => Some(me),
            _ => None,
        }
    }

    /// The configured ways to sign in, when the service told us — signed
    /// out (to sign in) or signed in (to switch or add an account).
    pub fn login_options(&self) -> Option<&LoginOptionsInfo> {
        match self {
            CloudSession::Anonymous { options } | CloudSession::SignedIn { options, .. } => {
                options.as_ref()
            }
            _ => None,
        }
    }

    /// Whether the answer is still outstanding (the shimmer condition).
    pub fn is_pending(&self) -> bool {
        matches!(self, CloudSession::Pending)
    }
}

/// Ask the service who we are.
///
/// One call when unreachable, two when anonymous, three when signed in: the
/// extra `LoginOptions` round trip happens on every path that is about to
/// render a sign-in link — the chrome's word when signed out, the identity
/// dropdown's switch/add rows when signed in (both are the same flow, spike
/// §5) — and only its own failure is tolerated.
pub async fn load_session<P: CloudPort + ?Sized>(port: &P) -> CloudSession {
    let Ok(info) = lpa_cloud_client::call(port, WhoAmI).await else {
        return CloudSession::Unreachable;
    };
    match info.actor {
        Actor::Anonymous => CloudSession::Anonymous {
            options: lpa_cloud_client::call(port, LoginOptions).await.ok(),
        },
        // A session the service acknowledged but will not describe is a
        // disagreement, not a signed-out state: staying quiet beats rendering
        // a nameless avatar.
        Actor::User(_) => match lpa_cloud_client::call(port, GetMe).await {
            Ok(me) => CloudSession::SignedIn {
                me,
                options: lpa_cloud_client::call(port, LoginOptions).await.ok(),
            },
            Err(_) => CloudSession::Unreachable,
        },
    }
}

/// The handle that re-asks: after a logout, an account edit, or a switch.
///
/// A counter rather than a callback because the fetch lives in one effect —
/// bumping it re-runs that effect, and there is only ever the one place the
/// question gets asked.
#[derive(Clone, Copy)]
pub struct CloudSessionRefresh(Signal<u32>);

impl CloudSessionRefresh {
    /// Re-ask the service. The signal goes back through
    /// [`CloudSession::Pending`] first, so the chrome shimmers rather than
    /// showing a stale identity while the answer is in flight.
    pub fn refresh(&mut self) {
        self.0 += 1;
    }
}

/// Provide `Signal<CloudSession>` + [`CloudSessionRefresh`], and keep them fed.
///
/// Call once, from `App`, after the standalone-page early returns.
pub fn use_cloud_session_provider() {
    let mut session = use_context_provider(|| Signal::new(CloudSession::Pending));
    let refresh = use_context_provider(|| CloudSessionRefresh(Signal::new(0u32)));
    use_effect(move || {
        // The one tracked read: bumping the counter re-runs this.
        let _ = refresh.0.read();
        session.set(CloudSession::Pending);
        spawn(async move {
            let next = load_session(&FetchCloudPort::new()).await;
            if let CloudSession::SignedIn { me, .. } = &next {
                account_memory::remember(me, js_sys::Date::now());
            }
            // The auto-publish driver's one input: whether there is an
            // account to converge on. The false→true edge sweeps the library
            // (D7); the other direction forgets the queue.
            #[cfg(target_arch = "wasm32")]
            crate::cloud::sync::sync_engine::set_signed_in(crate::cloud::sync::syncs(&next));
            session.set(next);
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpa_cloud_client::cloud_port::TransportError;
    use lpc_cloud_api::{
        CLOUD_API_VERSION, CloudCall, CloudError, CloudReply, CloudResponse, DevChoice,
        DevPickerOptions, OidcOption,
    };
    use lpc_history::{ContentHash, PrefixedUid, TreeManifest, UidPrefix};

    /// A port that answers each control-plane call from a script, in order.
    struct ScriptedPort {
        replies: core::cell::RefCell<Vec<Result<CloudReply, TransportError>>>,
    }

    impl ScriptedPort {
        fn new(replies: Vec<Result<CloudReply, TransportError>>) -> Self {
            Self {
                replies: core::cell::RefCell::new(replies),
            }
        }
    }

    impl CloudPort for ScriptedPort {
        async fn call(&self, _call: CloudCall) -> Result<CloudReply, TransportError> {
            self.replies.borrow_mut().remove(0)
        }
        async fn get_blob(&self, hash: ContentHash) -> Result<Vec<u8>, TransportError> {
            Err(TransportError::MissingBlob(hash))
        }
        async fn put_blob(&self, bytes: &[u8]) -> Result<ContentHash, TransportError> {
            Ok(ContentHash::of(bytes))
        }
        async fn get_tree(&self, hash: ContentHash) -> Result<TreeManifest, TransportError> {
            Err(TransportError::MissingBlob(hash))
        }
        async fn put_tree(&self, manifest: &TreeManifest) -> Result<ContentHash, TransportError> {
            Ok(manifest.package_hash())
        }
    }

    fn ok(response: CloudResponse) -> Result<CloudReply, TransportError> {
        Ok(CloudReply {
            version: CLOUD_API_VERSION,
            result: Ok(response),
        })
    }

    fn refused(error: CloudError) -> Result<CloudReply, TransportError> {
        Ok(CloudReply {
            version: CLOUD_API_VERSION,
            result: Err(error),
        })
    }

    fn who(actor: Actor) -> CloudResponse {
        CloudResponse::UserInfo(lpc_cloud_api::response::UserInfo { actor })
    }

    fn me() -> MeInfo {
        MeInfo {
            uid: PrefixedUid::mint(UidPrefix::User, &[1u8; 16]),
            email: "yona@example.com".to_string(),
            display_name: "Yona Appletree".to_string(),
            given_name: Some("Yona".to_string()),
            family_name: Some("Appletree".to_string()),
            picture_url: None,
            provider_label: "Google".to_string(),
            created_at: 1.0,
        }
    }

    fn options() -> LoginOptionsInfo {
        LoginOptionsInfo {
            oidc: vec![OidcOption {
                id: "google".to_string(),
                label: "Google".to_string(),
                start_path: "/auth/google".to_string(),
            }],
            dev_picker: Some(DevPickerOptions {
                start_path: "/auth/dev".to_string(),
                choices: vec![DevChoice {
                    email: "dev@example.com".to_string(),
                    display_name: "Dev".to_string(),
                }],
            }),
        }
    }

    fn run(replies: Vec<Result<CloudReply, TransportError>>) -> CloudSession {
        lpa_cloud_client::block_on(load_session(&ScriptedPort::new(replies)))
    }

    #[test]
    fn a_signed_in_caller_lands_on_its_account() {
        let session = run(vec![
            ok(who(Actor::User(me().uid))),
            ok(CloudResponse::MeInfo(me())),
            ok(CloudResponse::LoginOptionsInfo(options())),
        ]);
        assert_eq!(
            session,
            CloudSession::SignedIn {
                me: me(),
                options: Some(options()),
            }
        );
        assert_eq!(
            session.me().map(|m| m.email.as_str()),
            Some("yona@example.com")
        );
        // The dropdown's switch/add rows are sign-in links too, so a
        // signed-in session carries the options as well.
        assert!(session.login_options().is_some());
        assert!(!session.is_pending());
    }

    /// Options that do not land leave the account intact — the identity
    /// dropdown simply omits the rows that would have nowhere to go.
    #[test]
    fn a_signed_in_caller_survives_unanswered_login_options() {
        let session = run(vec![
            ok(who(Actor::User(me().uid))),
            ok(CloudResponse::MeInfo(me())),
            Err(TransportError::Offline),
        ]);
        assert_eq!(
            session,
            CloudSession::SignedIn {
                me: me(),
                options: None,
            }
        );
        assert!(session.login_options().is_none());
    }

    /// Anonymous costs the one extra call, and only then.
    #[test]
    fn an_anonymous_caller_also_learns_how_to_sign_in() {
        let session = run(vec![
            ok(who(Actor::Anonymous)),
            ok(CloudResponse::LoginOptionsInfo(options())),
        ]);
        assert_eq!(
            session,
            CloudSession::Anonymous {
                options: Some(options())
            }
        );
        assert!(session.login_options().is_some());
    }

    /// Options that do not land leave an anonymous session, not a broken one
    /// — P5 renders no affordance rather than one pointing nowhere.
    #[test]
    fn unanswered_login_options_still_leave_an_anonymous_session() {
        let session = run(vec![
            ok(who(Actor::Anonymous)),
            Err(TransportError::Offline),
        ]);
        assert_eq!(session, CloudSession::Anonymous { options: None });
        assert!(session.login_options().is_none());
    }

    #[test]
    fn an_unreachable_service_is_quiet() {
        assert_eq!(
            run(vec![Err(TransportError::Offline)]),
            CloudSession::Unreachable
        );
    }

    /// A tab open across a redeploy: the reply's version is refused before
    /// its body is read, and that is a quiet state too, not an error badge.
    #[test]
    fn a_version_mismatch_is_quiet() {
        let stale = Ok(CloudReply {
            version: CLOUD_API_VERSION + 1,
            result: Ok(who(Actor::Anonymous)),
        });
        assert_eq!(run(vec![stale]), CloudSession::Unreachable);
    }

    /// A session the service acknowledges but will not describe.
    #[test]
    fn an_undescribable_session_is_quiet() {
        let session = run(vec![
            ok(who(Actor::User(me().uid))),
            refused(CloudError::NotFound),
        ]);
        assert_eq!(session, CloudSession::Unreachable);
        assert!(session.me().is_none());
    }
}
