//! The live half of the Share pill: who may see it, what it shows, and
//! what its controls do to the service.
//!
//! [`SharePillPopover`] is pure. This is the component the chrome actually
//! mounts: it reads the `CloudSession`, asks the service about the project
//! in the address bar once, and renders nothing at all unless the answer
//! says this viewer administers it.
//!
//! # Why one `GetProject` decides everything
//!
//! The pill's condition is "published, and I can write it". Both halves are
//! the same question to the service:
//!
//! - a project it has never seen answers `NotFound`, and so does one whose
//!   link grants this caller nothing;
//! - [`ProjectInfo::members`] is `Some` **only** for a caller who is on the
//!   roster (P2: the member list is a list of people's email addresses, so
//!   it is answered to the members and to nobody else). A link-holder — even
//!   an `Access::Edit` one — gets `None`.
//!
//! So `members.is_some()` is exactly "owner or editor", and the same reply
//! carries the access level and the roster the panel renders. Reading the
//! local `CloudBinding` instead would need an OPFS mount to answer a
//! question the service answers in one round trip — and would answer it
//! from this device's bookkeeping rather than from the truth.
//!
//! # Optimistic, with a real undo
//!
//! `SetAccess` flips the segment immediately and calls afterwards: the
//! control is a statement of intent, and a segment that waits for a round
//! trip before moving feels broken on a slow link. A refusal puts the old
//! level back and says so — the panel never shows a level the service does
//! not hold.

use dioxus::prelude::*;
use lpc_cloud_api::request::{AddMember, ArchiveProject, GetProject, RemoveMember, SetAccess};
use lpc_cloud_api::response::ProjectInfo;
use lpc_cloud_api::{Access, MemberInfo};
use lpc_history::PrefixedUid;

use crate::app::share::share_panel::SharePillPopover;
use crate::app::share::share_person::people_of;
use crate::app::share::share_url::{ShareUrl, current_origin};
use crate::base::Toasts;
use crate::cloud::{CloudSession, FetchCloudPort};

/// The chrome's Share slot for the project route it is showing.
///
/// Inert (renders nothing) without the `CloudSession` context, signed out,
/// or for a project this account does not administer — the visitor's own
/// variant of this door is P6's, not a dimmed version of this one.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn ProjectShareControl(
    /// The project in the address bar. The whole of the identity.
    uid: PrefixedUid,
    /// Stories and the ⋯ menu's "Sharing & access…" row: mount open.
    #[props(default = false)]
    initially_open: bool,
) -> Element {
    let Some(session) = try_consume_context::<Signal<CloudSession>>() else {
        return rsx! {};
    };
    let toasts = try_consume_context::<Toasts>();
    // Keyed on the ACCOUNT, not the whole session: a name edit writes a new
    // `MeInfo` through the context, and re-asking the service about this
    // project because somebody fixed a typo would be a wasted round trip.
    let me_email = use_memo(move || session().me().map(|me| me.email.clone()));
    let mut state = use_signal(|| ShareState::Loading);
    let mut busy = use_signal(|| false);
    use_effect(move || {
        if me_email().is_none() {
            state.set(ShareState::Absent);
            return;
        }
        spawn(async move {
            match lpa_cloud_client::call(&FetchCloudPort::new(), GetProject { uid }).await {
                Ok(info) => state.set(ShareState::of_info(&info)),
                Err(error) => {
                    // Silence, not a badge: an unpublished project, a
                    // project somebody else owns, and an unreachable
                    // service all mean "no sharing door here".
                    log::debug!("share: no administrable project at {uid}: {error}");
                    state.set(ShareState::Absent);
                }
            }
        });
    });

    let ShareState::Ready {
        name,
        slug,
        access,
        members,
    } = state()
    else {
        return rsx! {};
    };

    let url = ShareUrl {
        origin: current_origin(),
        slug: slug.clone(),
        uid,
    };
    let copy_url = url.absolute();

    let on_copy = EventHandler::new(move |()| {
        crate::clipboard::write_text(&copy_url);
        if let Some(mut toasts) = toasts {
            toasts.say("Link copied — opens running for anyone (no account).");
        }
    });
    let on_access = EventHandler::new(move |level: Access| {
        let Some(previous) = state.peek().access() else {
            return;
        };
        state.write().set_access(level);
        busy.set(true);
        spawn(async move {
            let result =
                lpa_cloud_client::call(&FetchCloudPort::new(), SetAccess { uid, access: level })
                    .await;
            busy.set(false);
            match result {
                Ok(info) => state.set(ShareState::of_info(&info)),
                Err(error) => {
                    log::warn!("share: could not set access on {uid}: {error}");
                    state.write().set_access(previous);
                    if let Some(mut toasts) = toasts {
                        toasts.warn("Could not change who the link lets in — nothing was saved.");
                    }
                }
            }
        });
    });
    let on_add = EventHandler::new(move |email: String| {
        spawn(async move {
            match lpa_cloud_client::call(&FetchCloudPort::new(), AddMember { uid, email }).await {
                Ok(info) => state.set(ShareState::of_info(&info)),
                Err(error) => {
                    log::warn!("share: could not add a member to {uid}: {error}");
                    if let Some(mut toasts) = toasts {
                        toasts.warn("Could not add that address — nobody was invited.");
                    }
                }
            }
        });
    });
    let on_remove = EventHandler::new(move |email: String| {
        spawn(async move {
            match lpa_cloud_client::call(&FetchCloudPort::new(), RemoveMember { uid, email }).await
            {
                Ok(info) => state.set(ShareState::of_info(&info)),
                Err(error) => {
                    log::warn!("share: could not remove a member from {uid}: {error}");
                    if let Some(mut toasts) = toasts {
                        toasts.warn("Could not remove them — their access is unchanged.");
                    }
                }
            }
        });
    });

    rsx! {
        SharePillPopover {
            name,
            url,
            access,
            people: people_of(&members, me_email().as_deref()),
            busy: busy(),
            on_access,
            on_copy,
            on_add,
            on_remove,
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

/// What the service has told us about this project, so far.
#[derive(Clone, Debug, PartialEq)]
enum ShareState {
    /// The first `GetProject` is in flight. Renders nothing — a pill that
    /// appears a beat after the page would read as a glitch, and the
    /// answer is usually "no door here" anyway.
    Loading,
    /// No door: unpublished, not ours, archived, or unreachable.
    Absent,
    /// Ours to administer.
    Ready {
        /// The display name the last commit carried (`SidecarMeta.name`) —
        /// what the panel's title calls this project. Falls back to the
        /// slug, and then to the uid, so the title is never blank.
        name: String,
        slug: String,
        access: Access,
        members: Vec<MemberInfo>,
    },
}

impl ShareState {
    /// A reply as state. `members: None` is the service saying "you are a
    /// link-holder, not a member" — no administration door for you (P6
    /// owns the visitor's variant). An archived project keeps resolving
    /// for its members but has no sharing to do until it is restored.
    fn of_info(info: &ProjectInfo) -> Self {
        match &info.members {
            Some(members) if !info.meta.archived => ShareState::Ready {
                name: display_name(info),
                slug: info.meta.slug.clone(),
                access: info.meta.access,
                members: members.clone(),
            },
            _ => ShareState::Absent,
        }
    }

    fn access(&self) -> Option<Access> {
        match self {
            ShareState::Ready { access, .. } => Some(*access),
            _ => None,
        }
    }

    /// The optimistic write (and the revert that undoes it).
    fn set_access(&mut self, level: Access) {
        if let ShareState::Ready { access, .. } = self {
            *access = level;
        }
    }
}

/// What to call this project: the commit's own display name, the slug when
/// the sidecar carries none, and the uid as the last resort — a blank title
/// reads as a rendering fault.
fn display_name(info: &ProjectInfo) -> String {
    for candidate in [info.sidecar.name.trim(), info.meta.slug.trim()] {
        if !candidate.is_empty() {
            return candidate.to_string();
        }
    }
    info.meta.uid.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpc_cloud_api::{Actor, HeadInfo, MemberRole, ProjectMeta, SidecarMeta};
    use lpc_history::UidPrefix;

    #[test]
    fn a_member_gets_the_door() {
        let state = ShareState::of_info(&info(Some(vec![owner()]), false));
        assert_eq!(
            state,
            ShareState::Ready {
                name: "Zook Dome".to_string(),
                slug: "zook-dome".to_string(),
                access: Access::View,
                members: vec![owner()],
            }
        );
        assert_eq!(state.access(), Some(Access::View));
    }

    /// A project whose sidecar never carried a name still has a title.
    #[test]
    fn the_title_falls_back_rather_than_blanking() {
        let mut nameless = info(Some(vec![owner()]), false);
        nameless.sidecar.name = "  ".to_string();
        assert_eq!(display_name(&nameless), "zook-dome");
        nameless.meta.slug = String::new();
        assert_eq!(display_name(&nameless), nameless.meta.uid.to_string());
    }

    /// An `Access::Edit` link-holder can write the project and still gets
    /// `members: None` — write access is never access to the roster.
    #[test]
    fn a_link_holder_gets_no_door() {
        assert_eq!(ShareState::of_info(&info(None, false)), ShareState::Absent);
    }

    /// Archived is reversible, not shareable: the ⋯ menu's Restore is the
    /// only verb it has left.
    #[test]
    fn an_archived_project_has_nothing_to_share() {
        assert_eq!(
            ShareState::of_info(&info(Some(vec![owner()]), true)),
            ShareState::Absent
        );
    }

    /// The optimistic flip and its revert are the same one-line write, and
    /// neither may invent a Ready state out of nothing.
    #[test]
    fn setting_access_moves_only_a_ready_state() {
        let mut state = ShareState::of_info(&info(Some(vec![owner()]), false));
        state.set_access(Access::Edit);
        assert_eq!(state.access(), Some(Access::Edit));
        state.set_access(Access::View);
        assert_eq!(state.access(), Some(Access::View));

        let mut absent = ShareState::Absent;
        absent.set_access(Access::Edit);
        assert_eq!(absent, ShareState::Absent);
        assert_eq!(ShareState::Loading.access(), None);
    }

    fn owner() -> MemberInfo {
        MemberInfo {
            email: "yona@example.com".to_string(),
            role: MemberRole::Owner,
            pending: false,
            user: None,
        }
    }

    fn info(members: Option<Vec<MemberInfo>>, archived: bool) -> ProjectInfo {
        ProjectInfo {
            meta: ProjectMeta {
                uid: PrefixedUid::mint(UidPrefix::Project, &[4u8; 16]),
                slug: "zook-dome".to_string(),
                access: Access::View,
                owner: Actor::Anonymous,
                archived,
            },
            heads: Vec::<HeadInfo>::new(),
            sidecar: SidecarMeta {
                name: "Zook Dome".to_string(),
                format_version: 4,
                preview_png: None,
            },
            members,
        }
    }
}
