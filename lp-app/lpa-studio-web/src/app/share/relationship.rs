//! The user's relationship to the OPEN project — one derivation, every
//! surface renders from it (vision D1). Not persisted; recomputed from
//! session + service signals.
//!
//! [`derive_relationship`] is a pure function over plain values so it is
//! testable without Dioxus in the loop; the components that eventually
//! call it (P3) own gathering those values from `UiStudioView`, the
//! `ProjectShareControl` fetch, and `CloudSession`.
//!
//! This module also carries [`fork_transient_session`] — the dispatch a
//! pristine (unedited) transient session needs to fork (P3's "Save a
//! copy" button). It mirrors [`super::visitor_session::VisitorSession::fork`]'s
//! transient arm, but stands alone: that method only fires from a
//! `VisitorSession` (routed to a `/p/<uid>` share link), while an embedded
//! example's transient session has no such route to key off. Q9 rules
//! that this dispatches `ProjectOp::SaveOverlay` directly rather than
//! picking a `UiPaneAction` out of `header_actions` — `header_actions` is
//! populated only while persisted edits are pending (presence-is-dirty
//! contract), so it is EMPTY exactly when a pristine session needs this
//! button to still work.

use lpa_studio_core::app::studio::studio_view_channel::CommandSender;
use lpa_studio_core::{ControllerId, ProjectController, ProjectOp, StudioCommand, UiAction};
use lpc_cloud_api::Actor;

use crate::base::StudioIconName;

/// The user's relationship to the OPEN project — one derivation, every
/// surface renders from it (vision D1). Not persisted; recomputed from
/// session + service signals.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectRelationship {
    /// Transient view of an embedded example (`open_transient_example`).
    Example,
    /// Transient view of someone else's shared project (View access).
    ViewingSomeoneElses,
    /// In this browser's library; no cloud roster answered (unpublished,
    /// restricted-to-owner, or service silent — the honest merge).
    MineLocal,
    /// In the library AND the service answered the member roster
    /// (owner/editor of a published project).
    MinePublished,
    /// Library tracking copy of someone else's project with Edit access.
    MemberOfSomeoneElses,
}

/// Derive the relationship from the three existing signals — no fetch of
/// its own, so it can run wherever the caller already holds these values.
///
/// - `open_project_transient` / `is_transient_example` — the core view's
///   transient pair (`UiStudioView::open_project_transient` /
///   `open_transient_example`, `ui_studio_view.rs:113-128`): a transient
///   session is either an embedded example or someone else's shared
///   project, and nothing else this function is told matters once that is
///   true.
/// - `in_library` — whether the open project has a library identity
///   (`library_identity.is_some()`, `project_pane.rs:66`): the precondition
///   for every `Mine*` / `MemberOfSomeoneElses` state.
/// - `roster_answered` — the `GetProject` answer's `members.is_some()`
///   (`ShareMode::Member` in `visitor_mode.rs`): the service saying this
///   viewer is on the roster (owner or editor) of a published project.
/// - `owner` / `viewer` — `ProjectMeta.owner` and the session's own actor
///   (`CloudSession::SignedIn { me }` as `Actor::User(me.uid)`, a guest or
///   signed-out session as `Actor::Anonymous`), each `None` when not yet
///   known. Comparing them distinguishes `MemberOfSomeoneElses` from
///   `MinePublished`; when the comparison is not possible — either side
///   unknown, or both `Actor::Anonymous` (indistinguishable identities) —
///   the two compare equal (or absent) and the derivation falls back to
///   `MinePublished` for a roster-answered project. Honest enough for v1:
///   noted here rather than papered over with a guess.
pub fn derive_relationship(
    open_project_transient: bool,
    is_transient_example: bool,
    in_library: bool,
    roster_answered: bool,
    owner: Option<Actor>,
    viewer: Option<Actor>,
) -> ProjectRelationship {
    if open_project_transient {
        return if is_transient_example {
            ProjectRelationship::Example
        } else {
            ProjectRelationship::ViewingSomeoneElses
        };
    }
    if !in_library || !roster_answered {
        return ProjectRelationship::MineLocal;
    }
    match (owner, viewer) {
        (Some(owner), Some(viewer)) if owner != viewer => ProjectRelationship::MemberOfSomeoneElses,
        // Equal actors, or either side unknown, or the ambiguous
        // anonymous-vs-anonymous pair: the honest merge is "mine".
        _ => ProjectRelationship::MinePublished,
    }
}

/// The relationship's **face**: the glyph-plus-word the project segment
/// wears (spike §3, vocabulary V2 — glyph AND word wide, glyph alone at the
/// narrow fold).
///
/// The family is NEUTRAL by ruling (D12, post-#478): a face states who this
/// document is to you, which is identity, not health — so it never borrows
/// a status color. Green would read "good", blue "live", violet is reserved
/// for binding; the face takes the dim/subtle foreground and nothing else.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RelationshipFace {
    /// The one word: Example / Private / Shared / Member / Viewing. The
    /// owner-name future ("eshan's") slots into this position unchanged.
    pub word: &'static str,
    /// The glyph that survives the 900px fold when the word drops.
    pub glyph: StudioIconName,
    /// The segment's tooltip — the sentence the word compresses.
    pub title: &'static str,
}

/// The face for one relationship. Total over the enum, so a new state
/// cannot ship without its vocabulary.
pub fn relationship_face(relationship: ProjectRelationship) -> RelationshipFace {
    match relationship {
        ProjectRelationship::Example => RelationshipFace {
            word: "Example",
            glyph: StudioIconName::Test,
            title: "Built-in example \u{2014} save a copy to make it yours",
        },
        ProjectRelationship::MineLocal => RelationshipFace {
            word: "Private",
            glyph: StudioIconName::RelationshipPrivate,
            title: "Yours \u{2014} not shared",
        },
        ProjectRelationship::MinePublished => RelationshipFace {
            word: "Shared",
            glyph: StudioIconName::RelationshipShared,
            title: "Yours \u{2014} anyone with the link can view",
        },
        ProjectRelationship::MemberOfSomeoneElses => RelationshipFace {
            word: "Member",
            glyph: StudioIconName::RelationshipMember,
            title: "Someone else\u{2019}s project \u{2014} you can edit",
        },
        ProjectRelationship::ViewingSomeoneElses => RelationshipFace {
            word: "Viewing",
            glyph: StudioIconName::RelationshipViewing,
            title: "Someone else\u{2019}s project \u{2014} save a copy to keep changes",
        },
    }
}

/// Fork the active transient session: dispatch the explicit save that
/// `fork_transient_at_save` (`project_controller.rs:5968`) promotes into
/// the library. Reaches the controller even for a CLEAN overlay — a
/// pristine save commits nothing (`written == 0`) but still runs the
/// fork, because `pulled` only turns false on a failed commit — so this
/// is exactly the verb a "Save a copy" button on a pristine example
/// needs, without waiting for an edit first.
///
/// Mirrors `VisitorSession::fork`'s transient arm
/// (`visitor_session.rs:189-195`) but does not require a `VisitorSession`:
/// an embedded example's transient session has no `/p/<uid>` route to key
/// a `VisitorSession` off, so this dispatches straight to the controller
/// instead.
pub fn fork_transient_session(tx: &CommandSender) {
    tx.send(StudioCommand::Action(UiAction::from_op(
        ControllerId::new(ProjectController::NODE_ID),
        ProjectOp::SaveOverlay,
    )));
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpc_history::{PrefixedUid, UidPrefix};

    fn user(seed: u8) -> Actor {
        Actor::User(PrefixedUid::mint(UidPrefix::User, &[seed; 16]))
    }

    #[test]
    fn a_transient_example_is_example() {
        assert_eq!(
            derive_relationship(true, true, false, false, None, None),
            ProjectRelationship::Example
        );
    }

    #[test]
    fn a_transient_non_example_is_viewing_someone_elses() {
        assert_eq!(
            derive_relationship(true, false, false, false, None, None),
            ProjectRelationship::ViewingSomeoneElses
        );
        // Transient always short-circuits, whatever else is passed.
        assert_eq!(
            derive_relationship(true, false, true, true, Some(user(1)), Some(user(2))),
            ProjectRelationship::ViewingSomeoneElses
        );
    }

    #[test]
    fn a_library_project_with_no_roster_answer_is_mine_local() {
        assert_eq!(
            derive_relationship(false, false, true, false, None, None),
            ProjectRelationship::MineLocal
        );
    }

    #[test]
    fn not_in_the_library_is_also_mine_local() {
        // The honest merge: unpublished, restricted, or the service simply
        // has not answered yet all fall back the same way.
        assert_eq!(
            derive_relationship(false, false, false, true, Some(user(1)), Some(user(1))),
            ProjectRelationship::MineLocal
        );
    }

    #[test]
    fn a_roster_answered_project_owned_by_the_viewer_is_mine_published() {
        assert_eq!(
            derive_relationship(false, false, true, true, Some(user(1)), Some(user(1))),
            ProjectRelationship::MinePublished
        );
    }

    #[test]
    fn a_roster_answered_project_owned_by_someone_else_is_member_of_someone_elses() {
        assert_eq!(
            derive_relationship(false, false, true, true, Some(user(1)), Some(user(2))),
            ProjectRelationship::MemberOfSomeoneElses
        );
    }

    /// The ambiguous-owner fallback: both sides answer `Actor::Anonymous`
    /// (ex: a guest-owned project viewed by another guest before either
    /// carries a comparable uid) — indistinguishable identities, so the
    /// roster answer alone decides and the derivation calls it "mine"
    /// rather than guessing membership.
    #[test]
    fn ambiguous_anonymous_owner_falls_back_to_mine_published() {
        assert_eq!(
            derive_relationship(
                false,
                false,
                true,
                true,
                Some(Actor::Anonymous),
                Some(Actor::Anonymous)
            ),
            ProjectRelationship::MinePublished
        );
    }

    /// An unknown owner or viewer (fetch still in flight) is the same
    /// honest fallback, not a guess in either direction.
    #[test]
    fn an_unknown_owner_or_viewer_falls_back_to_mine_published() {
        assert_eq!(
            derive_relationship(false, false, true, true, None, Some(user(1))),
            ProjectRelationship::MinePublished
        );
        assert_eq!(
            derive_relationship(false, false, true, true, Some(user(1)), None),
            ProjectRelationship::MinePublished
        );
    }
}
