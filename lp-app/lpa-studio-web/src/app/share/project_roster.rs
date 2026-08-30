//! The **live half** of sharing: one `GetProject` about the project in the
//! address bar, the roster it answers with, and the writes the access
//! controls make back against it.
//!
//! This used to be private to [`super::project_share_control`]. The
//! relationship control needs the same answer in a second place — the
//! project segment's popover owns Access now (vision D9) — and the
//! relationship derivation itself needs the roster half
//! ([`super::relationship::derive_relationship`]'s `roster_answered` /
//! `owner`), so the machinery moved here and both surfaces call
//! [`use_project_roster`]. One implementation, not two.
//!
//! # Why one `GetProject` decides everything
//!
//! The question is "published, and I can write it". Both halves are the
//! same question to the service:
//!
//! - a project it has never seen answers `NotFound`, and so does one whose
//!   link grants this caller nothing;
//! - [`ProjectInfo::members`] is `Some` **only** for a caller who is on the
//!   roster (P2: the member list is a list of people's email addresses, so
//!   it is answered to the members and to nobody else). A link-holder — even
//!   an `Access::Edit` one — gets `None`.
//!
//! So `members.is_some()` is exactly "owner or editor", and the same reply
//! carries the access level, the owner, and the roster the panel renders.
//! Reading the local `CloudBinding` instead would need an OPFS mount to
//! answer a question the service answers in one round trip — and would
//! answer it from this device's bookkeeping rather than from the truth.
//!
//! # Optimistic, with a real undo
//!
//! `SetAccess` flips the segment immediately and calls afterwards: the
//! control is a statement of intent, and a segment that waits for a round
//! trip before moving feels broken on a slow link. A refusal puts the old
//! level back and says so — no surface ever shows a level the service does
//! not hold.
//!
//! # Two mounts, two trips (this phase)
//!
//! The standalone Share pill and the project popover both call the hook
//! while the pill is still mounted (it retires in P5), so a project route
//! asks the service twice. Deliberate and temporary: sharing one fetch
//! across two independently-mounted chrome slots would mean a context
//! provider and a lifetime for it, for one phase of overlap. The answer is
//! idempotent and cached by the browser's own connection reuse; the cost is
//! one extra GET.

use dioxus::prelude::*;
use lpc_cloud_api::request::{AddMember, GetProject, RemoveMember, SetAccess};
use lpc_cloud_api::response::ProjectInfo;
use lpc_cloud_api::{Access, Actor, MemberInfo};
use lpc_history::PrefixedUid;

use crate::base::Toasts;
use crate::cloud::{CloudSession, FetchCloudPort};

/// What the service has told us about this project, so far.
#[derive(Clone, Debug, PartialEq)]
pub enum RosterState {
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
        /// Who the service says owns it — the half
        /// [`super::relationship::derive_relationship`] compares against
        /// the viewer to tell `MinePublished` from `MemberOfSomeoneElses`.
        owner: Actor,
        members: Vec<MemberInfo>,
    },
}

impl RosterState {
    /// A reply as state. `members: None` is the service saying "you are a
    /// link-holder, not a member" — no administration door for you (the
    /// visitor surface is `visitor_popover`'s). An archived project keeps
    /// resolving for its members but has no sharing to do until it is
    /// restored.
    pub fn of_info(info: &ProjectInfo) -> Self {
        match &info.members {
            Some(members) if !info.meta.archived => RosterState::Ready {
                name: display_name(info),
                slug: info.meta.slug.clone(),
                access: info.meta.access,
                owner: info.meta.owner,
                members: members.clone(),
            },
            _ => RosterState::Absent,
        }
    }

    /// The roster answer itself: whether the service put this viewer on the
    /// member list. Exactly `derive_relationship`'s `roster_answered`.
    pub fn answered(&self) -> bool {
        matches!(self, RosterState::Ready { .. })
    }

    pub fn access(&self) -> Option<Access> {
        match self {
            RosterState::Ready { access, .. } => Some(*access),
            _ => None,
        }
    }

    /// The project's owner per the service, once it has answered.
    pub fn owner(&self) -> Option<Actor> {
        match self {
            RosterState::Ready { owner, .. } => Some(*owner),
            _ => None,
        }
    }

    /// The optimistic write (and the revert that undoes it).
    fn set_access(&mut self, level: Access) {
        if let RosterState::Ready { access, .. } = self {
            *access = level;
        }
    }
}

/// The service's answer plus the three writes the access controls make —
/// one value both sharing surfaces render from.
///
/// `EventHandler` is `Copy`, so this whole value is cheap to clone into a
/// props struct and down through a popover.
#[derive(Clone, Copy, PartialEq)]
pub struct ProjectRoster {
    /// The answer, as state. Read `answered()` / `owner()` for the
    /// relationship derivation and match on `Ready` for the panel.
    pub state: Signal<RosterState>,
    /// A `SetAccess` is in flight; the segment stays interactive (the
    /// update is optimistic) but says so.
    pub busy: Signal<bool>,
    /// The signed-in account's address, for the "(you)" row.
    pub me_email: Memo<Option<String>>,
    pub on_access: EventHandler<Access>,
    pub on_add: EventHandler<String>,
    pub on_remove: EventHandler<String>,
}

/// Ask the service about `uid` and keep the answer, re-asking whenever the
/// project or the signed-in account changes.
///
/// `None` (no project route, or a transient session whose uid must never
/// reach the service) settles straight to [`RosterState::Absent`] without a
/// round trip. Signed out does the same: the roster is answered to members,
/// and an anonymous caller is never one.
pub fn use_project_roster(uid: Option<PrefixedUid>) -> ProjectRoster {
    let session = try_consume_context::<Signal<CloudSession>>();
    let toasts = try_consume_context::<Toasts>();
    // Keyed on the ACCOUNT, not the whole session: a name edit writes a new
    // `MeInfo` through the context, and re-asking the service about this
    // project because somebody fixed a typo would be a wasted round trip.
    let me_email =
        use_memo(move || session.and_then(|session| session().me().map(|me| me.email.clone())));
    let mut state = use_signal(|| RosterState::Loading);
    let mut busy = use_signal(|| false);

    // `use_reactive` because `uid` is a plain value, not a signal: without
    // it the effect would capture the uid it first saw and keep answering
    // about the project the tab used to have open.
    use_effect(use_reactive!(|(uid,)| {
        let signed_in = me_email().is_some();
        let Some(uid) = uid.filter(|_| signed_in) else {
            state.set(RosterState::Absent);
            return;
        };
        state.set(RosterState::Loading);
        spawn(async move {
            match lpa_cloud_client::call(&FetchCloudPort::new(), GetProject { uid }).await {
                Ok(info) => state.set(RosterState::of_info(&info)),
                Err(error) => {
                    // Silence, not a badge: an unpublished project, a
                    // project somebody else owns, and an unreachable
                    // service all mean "no sharing door here".
                    log::debug!("share: no administrable project at {uid}: {error}");
                    state.set(RosterState::Absent);
                }
            }
        });
    }));

    let on_access = EventHandler::new(move |level: Access| {
        let (Some(uid), Some(previous)) = (uid, state.peek().access()) else {
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
                Ok(info) => state.set(RosterState::of_info(&info)),
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
        let Some(uid) = uid else {
            return;
        };
        spawn(async move {
            match lpa_cloud_client::call(&FetchCloudPort::new(), AddMember { uid, email }).await {
                Ok(info) => state.set(RosterState::of_info(&info)),
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
        let Some(uid) = uid else {
            return;
        };
        spawn(async move {
            match lpa_cloud_client::call(&FetchCloudPort::new(), RemoveMember { uid, email }).await
            {
                Ok(info) => state.set(RosterState::of_info(&info)),
                Err(error) => {
                    log::warn!("share: could not remove a member from {uid}: {error}");
                    if let Some(mut toasts) = toasts {
                        toasts.warn("Could not remove them — their access is unchanged.");
                    }
                }
            }
        });
    });

    ProjectRoster {
        state,
        busy,
        me_email,
        on_access,
        on_add,
        on_remove,
    }
}

/// The signed-in viewer as an [`Actor`] — the other half of the owner
/// comparison. A guest or signed-out session is `Actor::Anonymous`, and a
/// session whose `whoami` has not landed is `None` rather than a guess.
pub fn viewer_actor(session: &CloudSession) -> Option<Actor> {
    match session {
        CloudSession::SignedIn { me, .. } => Some(Actor::User(me.uid)),
        CloudSession::Anonymous { .. } => Some(Actor::Anonymous),
        CloudSession::Pending | CloudSession::Unreachable => None,
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
    use lpc_cloud_api::{HeadInfo, MemberRole, ProjectMeta, SidecarMeta};
    use lpc_history::UidPrefix;

    #[test]
    fn a_member_gets_the_door() {
        let state = RosterState::of_info(&info(Some(vec![owner()]), false));
        assert_eq!(
            state,
            RosterState::Ready {
                name: "Zook Dome".to_string(),
                slug: "zook-dome".to_string(),
                access: Access::View,
                owner: Actor::Anonymous,
                members: vec![owner()],
            }
        );
        assert_eq!(state.access(), Some(Access::View));
        assert!(state.answered());
        assert_eq!(state.owner(), Some(Actor::Anonymous));
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
        let state = RosterState::of_info(&info(None, false));
        assert_eq!(state, RosterState::Absent);
        assert!(!state.answered());
        assert_eq!(state.owner(), None);
    }

    /// Archived is reversible, not shareable: the ⋯ menu's Restore is the
    /// only verb it has left.
    #[test]
    fn an_archived_project_has_nothing_to_share() {
        assert_eq!(
            RosterState::of_info(&info(Some(vec![owner()]), true)),
            RosterState::Absent
        );
    }

    /// The optimistic flip and its revert are the same one-line write, and
    /// neither may invent a Ready state out of nothing.
    #[test]
    fn setting_access_moves_only_a_ready_state() {
        let mut state = RosterState::of_info(&info(Some(vec![owner()]), false));
        state.set_access(Access::Edit);
        assert_eq!(state.access(), Some(Access::Edit));
        state.set_access(Access::View);
        assert_eq!(state.access(), Some(Access::View));

        let mut absent = RosterState::Absent;
        absent.set_access(Access::Edit);
        assert_eq!(absent, RosterState::Absent);
        assert_eq!(RosterState::Loading.access(), None);
    }

    /// The viewer half of the owner comparison: a real account is its uid, a
    /// guest is anonymous, and an unanswered session is unknown — never a
    /// guess in either direction.
    #[test]
    fn the_viewer_actor_says_what_the_session_knows() {
        assert_eq!(viewer_actor(&CloudSession::Pending), None);
        assert_eq!(viewer_actor(&CloudSession::Unreachable), None);
        assert_eq!(
            viewer_actor(&CloudSession::Anonymous { options: None }),
            Some(Actor::Anonymous)
        );
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
