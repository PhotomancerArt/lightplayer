//! Stories for the `/account` page.
//!
//! The live page reads the `CloudSession` context and fetches its own
//! session list; stories provide neither, so they mount the presentational
//! halves with fixtures — every value, every clock reading and every
//! gesture is a prop.
//!
//! `now_secs` is pinned per story so "signed in 3d ago" is the same string
//! on every capture, and photos use an inline `data:` image so no capture
//! depends on a provider CDN.

use dioxus::prelude::*;
use lpa_studio_web_story_macros::story;
use lpc_cloud_api::request::UpdateMe;
use lpc_cloud_api::{
    DevChoice, DevPickerOptions, LoginOptionsInfo, MeInfo, OidcOption, SessionInfo,
};
use lpc_history::{PrefixedUid, UidPrefix};

use crate::app::account::account_page::{
    AccountPageBody, AccountSignInCard, SaveStatus, SessionsPane,
};
use crate::cloud::sync::sync_status::{SyncOutcomeKind, SyncStatusBoard, SyncStatusSnapshot};

#[story(
    description = "The converged page (spike §3: B's structure, A's content) — Identity, Account, Sessions as settings rows on a 640px measure. Three browsers signed in; the calling one wears the `current` badge and cannot be signed out from its own row (that is what the footer's danger link is for)."
)]
pub(crate) fn page_signed_in() -> Element {
    rsx! {
        AccountPageBody {
            me: yona(),
            given: "Yona".to_string(),
            family: "Appletree".to_string(),
            sessions: SessionsPane::Ready(three_sessions()),
            now_secs: NOW,
            on_given: EventHandler::new(|_: String| {}),
            on_family: EventHandler::new(|_: String| {}),
            on_save: EventHandler::new(|_: UpdateMe| {}),
            on_revoke: EventHandler::new(|_: String| {}),
            on_sign_out_everywhere: EventHandler::new(|()| {}),
        }
    }
}

#[story(
    label = "One session",
    description = "The common case for a new account: signed in on one machine, no provider photo (initials on the account's own hue), and a Sessions group that is a single row plus its footer. The lifetime line is read off the current session's own expiry, not assumed from our config."
)]
pub(crate) fn page_single_session() -> Element {
    rsx! {
        AccountPageBody {
            me: crew(),
            given: "Zook".to_string(),
            family: "Dome-Crew".to_string(),
            sessions: SessionsPane::Ready(vec![current_session()]),
            now_secs: NOW,
            on_given: EventHandler::new(|_: String| {}),
            on_family: EventHandler::new(|_: String| {}),
            on_save: EventHandler::new(|_: UpdateMe| {}),
            on_revoke: EventHandler::new(|_: String| {}),
            on_sign_out_everywhere: EventHandler::new(|()| {}),
        }
    }
}

#[story(
    label = "Save on dirty",
    description = "Top: a family name edited but not yet saved — Save APPEARS on dirty rather than sitting greyed out, and the note is the guidance line. Bottom: the same rows a moment after saving — no button, and the note reads \"Saved.\" until the next keystroke."
)]
pub(crate) fn page_dirty_then_saved() -> Element {
    rsx! {
        div { class: STACK,
            AccountPageBody {
                me: yona(),
                given: "Yona".to_string(),
                family: "Appletree-Kane".to_string(),
                sessions: SessionsPane::Ready(vec![current_session()]),
                now_secs: NOW,
                on_given: EventHandler::new(|_: String| {}),
                on_family: EventHandler::new(|_: String| {}),
                on_save: EventHandler::new(|_: UpdateMe| {}),
                on_revoke: EventHandler::new(|_: String| {}),
                on_sign_out_everywhere: EventHandler::new(|()| {}),
            }
            AccountPageBody {
                me: yona(),
                given: "Yona".to_string(),
                family: "Appletree".to_string(),
                save_status: SaveStatus::Saved,
                sessions: SessionsPane::Ready(vec![current_session()]),
                now_secs: NOW,
                on_given: EventHandler::new(|_: String| {}),
                on_family: EventHandler::new(|_: String| {}),
                on_save: EventHandler::new(|_: UpdateMe| {}),
                on_revoke: EventHandler::new(|_: String| {}),
                on_sign_out_everywhere: EventHandler::new(|()| {}),
            }
        }
    }
}

#[story(
    label = "Awkward identity",
    description = "The three names that argue for two neutral boxes over one \"full name\" field: a family-first CJK name (山田 太郎 — given box first, family box second, exactly as the provider sent them), a mononym whose family box is legitimately empty, and a very long name on a very long domain, which must ellipsize inside the 640px measure rather than widen it."
)]
pub(crate) fn page_awkward_identity() -> Element {
    rsx! {
        div { class: STACK,
            AccountPageBody {
                me: yamada(),
                given: "山田".to_string(),
                family: "太郎".to_string(),
                sessions: SessionsPane::Ready(vec![current_session()]),
                now_secs: NOW,
                on_given: EventHandler::new(|_: String| {}),
                on_family: EventHandler::new(|_: String| {}),
                on_save: EventHandler::new(|_: UpdateMe| {}),
                on_revoke: EventHandler::new(|_: String| {}),
                on_sign_out_everywhere: EventHandler::new(|()| {}),
            }
            AccountPageBody {
                me: cher(),
                given: "Cher".to_string(),
                family: String::new(),
                sessions: SessionsPane::Ready(vec![current_session()]),
                now_secs: NOW,
                on_given: EventHandler::new(|_: String| {}),
                on_family: EventHandler::new(|_: String| {}),
                on_save: EventHandler::new(|_: UpdateMe| {}),
                on_revoke: EventHandler::new(|_: String| {}),
                on_sign_out_everywhere: EventHandler::new(|()| {}),
            }
            AccountPageBody {
                me: maria(),
                given: "María-Guadalupe Fernanda".to_string(),
                family: "de los Ángeles Rodríguez-Vásquez".to_string(),
                sessions: SessionsPane::Ready(vec![current_session()]),
                now_secs: NOW,
                on_given: EventHandler::new(|_: String| {}),
                on_family: EventHandler::new(|_: String| {}),
                on_save: EventHandler::new(|_: UpdateMe| {}),
                on_revoke: EventHandler::new(|_: String| {}),
                on_sign_out_everywhere: EventHandler::new(|()| {}),
            }
        }
    }
}

#[story(
    label = "Sessions still loading",
    description = "The two states the list itself can be in before it is a list: in flight (a shimmer, not an empty group that would read as \"you are signed in nowhere\"), and unreachable — where the footer's danger link stays, because signing out everywhere is how you recover from a list you cannot read."
)]
pub(crate) fn page_sessions_pending() -> Element {
    rsx! {
        div { class: STACK,
            AccountPageBody {
                me: yona(),
                given: "Yona".to_string(),
                family: "Appletree".to_string(),
                sessions: SessionsPane::Loading,
                now_secs: NOW,
                on_given: EventHandler::new(|_: String| {}),
                on_family: EventHandler::new(|_: String| {}),
                on_save: EventHandler::new(|_: UpdateMe| {}),
                on_sign_out_everywhere: EventHandler::new(|()| {}),
            }
            AccountPageBody {
                me: yona(),
                given: "Yona".to_string(),
                family: "Appletree".to_string(),
                sessions: SessionsPane::Unavailable,
                now_secs: NOW,
                on_given: EventHandler::new(|_: String| {}),
                on_family: EventHandler::new(|_: String| {}),
                on_save: EventHandler::new(|_: UpdateMe| {}),
                on_sign_out_everywhere: EventHandler::new(|()| {}),
            }
        }
    }
}

#[story(
    label = "Cloud sync ledger",
    description = "The Cloud sync group with every outcome the driver records: the sweep summary line, then per-project rows — published/pushed (good), no-save-yet and skipped (informational), retrying (warning), refused and denied (error). The detail sentence is the diagnosis and survives truncation as the tooltip; nothing here is a control."
)]
pub(crate) fn page_sync_ledger() -> Element {
    rsx! {
        AccountPageBody {
            me: yona(),
            given: "Yona".to_string(),
            family: "Appletree".to_string(),
            sessions: SessionsPane::Ready(vec![current_session()]),
            sync: sync_ledger(),
            now_secs: NOW,
            on_given: EventHandler::new(|_: String| {}),
            on_family: EventHandler::new(|_: String| {}),
            on_save: EventHandler::new(|_: UpdateMe| {}),
            on_revoke: EventHandler::new(|_: String| {}),
            on_sign_out_everywhere: EventHandler::new(|()| {}),
        }
    }
}

#[story(
    label = "Signed out",
    description = "/account is a real address someone can bookmark, so signed out it is an invitation rather than a 404: the ask, then the chrome's own §4 chooser body (minus its title — the heading already made the ask). Right, the boot state: the card holds its shape while whoami is in flight instead of offering to sign in a beat before showing a profile."
)]
pub(crate) fn signed_out_and_pending() -> Element {
    rsx! {
        div { class: "tw:grid tw:grid-cols-2 tw:items-start tw:gap-8",
            AccountSignInCard { options: Some(dev_options()), next: "/account".to_string() }
            AccountSignInCard { pending: true, next: "/account".to_string() }
        }
    }
}

/// The stories' fixed "now": 2026-08-07T00:00:00Z, epoch seconds. Pinned so
/// "signed in 3d ago" is a constant, not a function of capture time.
const NOW: f64 = 1_786_060_800.0;
const DAY: f64 = 86_400.0;
const SESSION_TTL: f64 = 30.0 * DAY;

/// A staged sync ledger holding one row of every outcome kind, minutes old
/// against the pinned clock so the trailing "· Nm ago" stays constant.
fn sync_ledger() -> SyncStatusSnapshot {
    let mut board = SyncStatusBoard::default();
    board.record_signed_in(true);
    board.record_sweep(7, false, (NOW - 300.0) * 1000.0);
    let rows: [(&str, &str, SyncOutcomeKind, &str); 7] = [
        (
            "prjaaaaaaaaaaaaaaaa",
            "Zook Dome",
            SyncOutcomeKind::Published,
            "record and content are up",
        ),
        (
            "prjbbbbbbbbbbbbbbbb",
            "Logo Sign",
            SyncOutcomeKind::Pushed,
            "content is up",
        ),
        (
            "prjcccccccccccccccc",
            "Fresh Sketch",
            SyncOutcomeKind::NothingSaved,
            "no saved version yet — nothing to publish",
        ),
        (
            "prjdddddddddddddddd",
            "Workbench Wall",
            SyncOutcomeKind::Skipped,
            "open in another tab; that tab syncs it",
        ),
        (
            "prjeeeeeeeeeeeeeeee",
            "Ember Dusk",
            SyncOutcomeKind::Retrying,
            "transport: the service was unreachable",
        ),
        (
            "prjffffffffffffffff",
            "Ancient Import",
            SyncOutcomeKind::Refused,
            "local history is unreadable — the project has no event log",
        ),
        (
            "prjgggggggggggggggg",
            "Borrowed Tracking Copy",
            SyncOutcomeKind::Denied,
            "the service refused the push: not yours to write",
        ),
    ];
    for (i, (uid, name, kind, detail)) in rows.into_iter().enumerate() {
        board.record_project(uid, name, kind, detail, (NOW - 60.0 * (i as f64 + 1.0)) * 1000.0);
    }
    SyncStatusSnapshot {
        engine: board.engine.clone(),
        rows: board.rows(),
    }
}

/// The row stack for stories that show several pages at once.
const STACK: &str = "tw:grid tw:gap-8";

fn current_session() -> SessionInfo {
    SessionInfo {
        id: "a1b2c3d4".to_string(),
        created_at: NOW - 1.5 * DAY,
        expires_at: NOW - 1.5 * DAY + SESSION_TTL,
        user_agent: Some(
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 \
             (KHTML, like Gecko) Version/17.4 Safari/605.1.15"
                .to_string(),
        ),
        current: true,
    }
}

fn three_sessions() -> Vec<SessionInfo> {
    vec![
        current_session(),
        SessionInfo {
            id: "e5f6a7b8".to_string(),
            created_at: NOW - 5.0 * DAY,
            expires_at: NOW - 5.0 * DAY + SESSION_TTL,
            user_agent: Some(
                "Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 \
                 (KHTML, like Gecko) Chrome/124.0.0.0 Mobile Safari/537.36"
                    .to_string(),
            ),
            current: false,
        },
        // No user agent: an edge that captured none, or a row from before
        // the column existed. It still renders a row you can end.
        SessionInfo {
            id: "c9d0e1f2".to_string(),
            created_at: NOW - 23.0 * DAY,
            expires_at: NOW - 23.0 * DAY + SESSION_TTL,
            user_agent: None,
            current: false,
        },
    ]
}

fn google() -> OidcOption {
    OidcOption {
        id: "google".to_string(),
        label: "Google".to_string(),
        start_path: "/auth/google".to_string(),
    }
}

fn dev_options() -> LoginOptionsInfo {
    LoginOptionsInfo {
        oidc: vec![google()],
        dev_picker: Some(DevPickerOptions {
            start_path: "/auth/dev".to_string(),
            choices: vec![DevChoice {
                email: "lightatplay@gmail.com".to_string(),
                display_name: "Yona Appletree".to_string(),
            }],
        }),
    }
}

/// A deterministic stand-in for a provider photo — no CDN in a capture.
const PHOTO: &str = "data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 64 64'%3E%3Crect width='64' height='64' fill='rgb(20,50,63)'/%3E%3Ccircle cx='32' cy='25' r='11' fill='rgb(123,224,178)'/%3E%3Cellipse cx='32' cy='58' rx='20' ry='16' fill='rgb(29,92,80)'/%3E%3C/svg%3E";

fn person(given: &str, family: &str, email: &str, photo: bool, provider: &str, seed: u8) -> MeInfo {
    let display_name = [given, family]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    MeInfo {
        uid: PrefixedUid::mint(UidPrefix::User, &[seed; 16]),
        email: email.to_string(),
        display_name,
        given_name: (!given.is_empty()).then(|| given.to_string()),
        family_name: (!family.is_empty()).then(|| family.to_string()),
        picture_url: photo.then(|| PHOTO.to_string()),
        provider_label: provider.to_string(),
        // 2026-06-14T00:00:00Z — an account with some history behind it.
        created_at: 1_781_395_200.0,
    }
}

fn yona() -> MeInfo {
    person(
        "Yona",
        "Appletree",
        "lightatplay@gmail.com",
        true,
        "Google",
        1,
    )
}

fn crew() -> MeInfo {
    person("Zook", "Dome-Crew", "crew@zookdome.org", false, "Google", 3)
}

fn maria() -> MeInfo {
    person(
        "María-Guadalupe Fernanda",
        "de los Ángeles Rodríguez-Vásquez",
        "maria.guadalupe.fernanda.rodriguez.vasquez@extremely-long-domain.example.org",
        true,
        "Google",
        4,
    )
}

fn yamada() -> MeInfo {
    person("山田", "太郎", "yamada.taro@gmail.com", false, "Dev", 5)
}

fn cher() -> MeInfo {
    person("Cher", "", "cher@gmail.com", false, "Google", 6)
}
