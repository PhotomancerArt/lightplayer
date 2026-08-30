//! The chrome's standalone Share pill: the thin mount over
//! [`use_project_roster`], plus the archive verb the ⋯ menu's row dispatches.
//!
//! [`SharePillPopover`] is pure and [`super::project_roster`] is the live
//! half — one `GetProject`, the optimistic `SetAccess`, the roster writes.
//! This component is only the *gate*: it renders nothing at all unless the
//! answer says this viewer administers the project in the address bar.
//!
//! The pill is on its way out (relationship-control P5): the project
//! segment's popover owns Access now and mounts the same roster from the
//! same hook. Until it goes, both surfaces stand — see the two-mounts note
//! in [`super::project_roster`].

use dioxus::prelude::*;
use lpc_cloud_api::request::ArchiveProject;
use lpc_history::PrefixedUid;

use crate::app::share::project_roster::{RosterState, use_project_roster};
use crate::app::share::share_panel::SharePillPopover;
use crate::app::share::share_person::people_of;
use crate::app::share::share_url::{ShareUrl, current_origin};
use crate::base::Toasts;
use crate::cloud::FetchCloudPort;

/// The chrome's Share slot for the project route it is showing.
///
/// Inert (renders nothing) without the `CloudSession` context, signed out,
/// or for a project this account does not administer — the visitor's own
/// variant of this door is `visitor_popover`'s, not a dimmed version of
/// this one.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn ProjectShareControl(
    /// The project in the address bar. The whole of the identity.
    uid: PrefixedUid,
    /// Stories and the ⋯ menu's "Sharing & access…" row: mount open.
    #[props(default = false)]
    initially_open: bool,
) -> Element {
    let toasts = try_consume_context::<Toasts>();
    let roster = use_project_roster(Some(uid));

    let RosterState::Ready {
        name,
        slug,
        access,
        members,
        ..
    } = (roster.state)()
    else {
        return rsx! {};
    };

    let url = ShareUrl {
        origin: current_origin(),
        slug: slug.clone(),
        uid: Some(uid),
    };
    let copy_url = url.absolute();
    let on_copy = EventHandler::new(move |()| {
        crate::clipboard::write_text(&copy_url);
        if let Some(mut toasts) = toasts {
            toasts.say("Link copied — opens running for anyone (no account).");
        }
    });

    rsx! {
        SharePillPopover {
            name,
            url,
            access,
            people: people_of(&members, (roster.me_email)().as_deref()),
            busy: (roster.busy)(),
            on_access: roster.on_access,
            on_copy,
            on_add: roster.on_add,
            on_remove: roster.on_remove,
            initially_open,
        }
    }
}

/// Archive the project at `uid`, then run `on_archived` if the service
/// agreed.
///
/// Lives here rather than in the chrome because the ⋯ row is only the
/// door: what "archive" means to the service — owner-only, reversible,
/// nothing deleted — is this module's concern. A refusal is reported and
/// nothing moves; the caller's `on_archived` is what navigates away.
pub fn archive_project(
    uid: PrefixedUid,
    toasts: Option<Toasts>,
    on_archived: impl FnOnce() + 'static,
) {
    spawn(async move {
        match lpa_cloud_client::call(&FetchCloudPort::new(), ArchiveProject { uid }).await {
            Ok(_) => {
                on_archived();
                if let Some(mut toasts) = toasts {
                    toasts.say("Archived — Restore from the Projects page.");
                }
            }
            Err(error) => {
                log::warn!("share: could not archive {uid}: {error}");
                if let Some(mut toasts) = toasts {
                    toasts.warn("Could not archive this project — it is still where it was.");
                }
            }
        }
    });
}
