//! The flagship cloud-sync scenarios, one test per story.
//!
//! Two or three clients and one in-process service, no network and no clock
//! but the service's own. The vocabulary is in [`builders`] — read that
//! module's header once and every test below reads as prose.

mod builders;

use builders::TestWorld;
use lpa_cloud_client::{ClobberSide, SyncError, TransportError};
use lpc_cloud_api::{Access, CloudError};
use lpc_history::SyncRelation;

/// Share by URL, open with no account: the whole product in one test.
#[test]
fn publish_then_anonymous_pull() {
    let td = TestWorld::new();
    let owner = td.user();
    let visitor = td.visitor();

    let dome = owner.project();
    let link = dome.publish(Access::View);
    assert_eq!(link.path(), format!("/p/proj-1-{}", dome.uid()));

    let copy = visitor.open_url(&format!("https://lightplayer.app{}", link.path()));
    assert_eq!(copy.uid(), dome.uid());
    assert_eq!(copy.head(), dome.head());
    assert_eq!(copy.shader(), "first light");
    assert_eq!(copy.bound_to(), Some(dome.uid()));
    assert!(copy.pull().is_up_to_date());

    let sketch = owner.project();
    let unlisted = sketch.publish(Access::None);
    assert!(matches!(
        visitor.open_shared_error(&unlisted),
        SyncError::Cloud(CloudError::NotFound)
    ));
}

/// The publisher keeps working; everybody holding the link catches up.
#[test]
fn publisher_pushes_tracker_fast_forwards() {
    let td = TestWorld::new();
    let owner = td.user();
    let visitor = td.visitor();

    let dome = owner.project();
    let link = dome.publish(Access::View);
    let tracker = visitor.open_shared(&link);

    dome.edit("brighter");
    assert!(dome.push().advanced());

    let pulled = tracker.pull();
    assert!(pulled.can_fast_forward());
    assert_eq!(tracker.shader(), "first light");

    tracker.fast_forward(&pulled);
    assert_eq!(tracker.shader(), "brighter");
    assert_eq!(tracker.head(), dome.head());
    assert!(tracker.pull().is_up_to_date());
}

/// A viewer who starts editing is not stuck: they fork and keep their own
/// line, and the copy they forked from still tracks the original.
#[test]
fn viewer_edits_diverges_then_forks() {
    let td = TestWorld::new();
    let owner = td.user();
    let visitor = td.visitor();

    let dome = owner.project();
    let link = dome.publish(Access::View);
    let copy = visitor.open_shared(&link);

    copy.edit("my own idea");
    dome.edit("the owner's idea");
    dome.push();

    assert!(copy.pull().is_diverged());

    let mine = copy.fork();
    assert_ne!(mine.uid(), copy.uid());
    assert_eq!(mine.shader(), "my own idea");
    assert_eq!(mine.bound_to(), None);

    assert_eq!(copy.shader(), "my own idea");
    assert_eq!(copy.bound_to(), Some(dome.uid()));
}

/// The P6 visitor flow, end to end: a share link opened as a tracking
/// copy, the owner's pushes arriving as fast-forwards, a local edit whose
/// push the service refuses, and the fork that gives that work a home —
/// which, signed in, publishes as the visitor's own project. The refused
/// push must leave the tracking copy's binding untouched (refusal is an
/// answer, not damage), and the fork must not disturb the copy's tracking.
#[test]
fn visitor_tracks_edits_is_refused_forks_and_publishes() {
    let td = TestWorld::new();
    let owner = td.user();
    // Signed in but never invited: the refusal is NotAuthorized (the
    // anonymous twin of this story gets NotAuthenticated — see
    // `anonymous_cannot_push_a_view_link`).
    let visitor = td.user();

    let dome = owner.project();
    let link = dome.publish(Access::View);

    // open_shared: uid preserved, real history, a binding that tracks
    let copy = visitor.open_shared(&link);
    assert_eq!(copy.uid(), dome.uid());
    assert_eq!(copy.bound_to(), Some(dome.uid()));
    assert_eq!(copy.head(), dome.head());

    // the owner keeps working; the visitor's pull is a fast-forward —
    // and a link-holder's pull never carries the roster
    dome.edit("brighter");
    assert!(dome.push().advanced());
    let pulled = copy.pull();
    assert!(pulled.can_fast_forward());
    assert!(
        pulled.members.is_none(),
        "write access is never access to the roster"
    );
    copy.fast_forward(&pulled);
    assert_eq!(copy.shader(), "brighter");
    assert_eq!(copy.head(), dome.head());

    // the visitor edits and saves; the push is refused, terminally —
    // the service is unchanged and so is the copy's binding
    copy.edit("the visitor's take");
    assert!(matches!(
        copy.push_error(),
        SyncError::Cloud(CloudError::NotAuthorized)
    ));
    assert_eq!(copy.bound_to(), Some(dome.uid()));
    assert_eq!(dome.server_heads(), vec![dome.head()]);
    // unpushed local work on a tracking copy reads as ahead, not lost
    assert!(copy.pull().local_ahead());

    // fork: a NEW project at the visitor's version — new uid, no binding,
    // history rooted at the parent's version
    let mine = copy.fork();
    assert_ne!(mine.uid(), copy.uid());
    assert_eq!(mine.shader(), "the visitor's take");
    assert_eq!(mine.bound_to(), None);
    assert_eq!(mine.relation_to(copy.head()), SyncRelation::AtHead);

    // signed in, the fork publishes as the visitor's own project at its
    // own address
    let mine_link = mine.publish(Access::View);
    assert_eq!(mine_link.uid, mine.uid());
    assert_eq!(mine.bound_to(), Some(mine.uid()));
    assert_eq!(mine.server_heads(), vec![mine.head()]);

    // and the tracking copy still tracks the ORIGINAL, edits intact
    assert_eq!(copy.bound_to(), Some(dome.uid()));
    assert_eq!(copy.shader(), "the visitor's take");
}

/// Two people on one project: an invitation that predates the account, a
/// fast-forward each way, a collision, and the loser still reachable.
#[test]
fn two_members_edit_push_pull() {
    let td = TestWorld::new();
    let owner = td.user();
    let dome = owner.project();
    let link = dome.publish(Access::None);

    let invited = td.invitee();
    dome.add_member(invited.email());
    let member = invited.sign_in();
    let theirs = member.open_shared(&link);

    theirs.edit("the member's pass");
    assert!(theirs.push().advanced());

    let pulled = dome.pull();
    assert!(pulled.can_fast_forward());
    dome.fast_forward(&pulled);
    assert_eq!(dome.shader(), "the member's pass");

    dome.edit("the owner's pass");
    theirs.edit("the member's second pass");
    assert!(theirs.push().advanced());
    assert!(dome.push().created_new_head());
    assert_eq!(dome.server_heads().len(), 2);

    let divergence = dome.pull();
    assert!(divergence.is_diverged());
    let resolved = dome.resolve(&divergence, ClobberSide::Ours);
    assert_eq!(dome.shader(), "the owner's pass");
    assert_eq!(dome.server_heads(), vec![dome.head()]);
    assert_eq!(
        dome.shader_at(resolved.set_aside),
        "the member's second pass"
    );

    let after_the_join = theirs.pull();
    assert!(after_the_join.can_fast_forward());
    theirs.fast_forward(&after_the_join);
    assert_eq!(theirs.shader(), "the owner's pass");
    assert_eq!(theirs.head(), dome.head());
    assert_eq!(theirs.relation_to(resolved.set_aside), SyncRelation::Behind);
    assert_eq!(
        theirs.shader_at(resolved.set_aside),
        "the member's second pass"
    );
}

/// Offline is a failed attempt, not a mode: the work continues, and the
/// retry lands everything.
#[test]
fn offline_queue_then_push() {
    let td = TestWorld::new();
    let owner = td.user();
    let dome = owner.project();
    let link = dome.publish(Access::View);

    owner.go_offline();
    dome.edit("sketching on a plane");
    assert!(matches!(
        dome.push_error(),
        SyncError::Transport(TransportError::Offline)
    ));
    dome.edit("still sketching");
    assert!(matches!(
        dome.push_error(),
        SyncError::Transport(TransportError::Offline)
    ));
    assert_eq!(dome.shader(), "still sketching");

    owner.go_online();
    assert!(dome.push().advanced());
    assert_eq!(dome.server_heads(), vec![dome.head()]);

    let onlooker = td.visitor();
    assert_eq!(onlooker.open_shared(&link).shader(), "still sketching");
}

/// Whichever side wins, the other is set aside rather than lost, and the
/// service is left holding one head.
#[test]
fn clobber_both_directions() {
    let td = TestWorld::new();
    let owner = td.user();

    let keeping_mine = owner.project();
    let mine_link = keeping_mine.publish(Access::View);
    let mine_peer = keeping_mine.collaborator().open_shared(&mine_link);
    mine_peer.edit("the peer's take");
    mine_peer.push();
    keeping_mine.edit("the owner's take");
    keeping_mine.push();

    let mine_divergence = keeping_mine.pull();
    let kept_ours = keeping_mine.resolve(&mine_divergence, ClobberSide::Ours);
    assert_eq!(keeping_mine.shader(), "the owner's take");
    assert_eq!(
        keeping_mine.shader_at(kept_ours.set_aside),
        "the peer's take"
    );
    assert_eq!(
        keeping_mine.relation_to(kept_ours.set_aside),
        SyncRelation::Behind
    );
    assert_eq!(keeping_mine.server_heads(), vec![keeping_mine.head()]);

    let taking_theirs = owner.project();
    let theirs_link = taking_theirs.publish(Access::View);
    let theirs_peer = taking_theirs.collaborator().open_shared(&theirs_link);
    theirs_peer.edit("the peer's take");
    theirs_peer.push();
    taking_theirs.edit("the owner's take");
    taking_theirs.push();

    let theirs_divergence = taking_theirs.pull();
    let kept_theirs = taking_theirs.resolve(&theirs_divergence, ClobberSide::Theirs);
    assert_eq!(taking_theirs.shader(), "the peer's take");
    assert_eq!(
        taking_theirs.shader_at(kept_theirs.set_aside),
        "the owner's take"
    );
    assert_eq!(
        taking_theirs.relation_to(kept_theirs.set_aside),
        SyncRelation::Behind
    );
    assert_eq!(taking_theirs.server_heads(), vec![taking_theirs.head()]);
}

/// A `View` link is permission to read, never to write.
#[test]
fn anonymous_cannot_push_a_view_link() {
    let td = TestWorld::new();
    let owner = td.user();
    let visitor = td.visitor();

    let dome = owner.project();
    let link = dome.publish(Access::View);
    let copy = visitor.open_shared(&link);
    copy.edit("a stranger's idea");

    assert!(matches!(
        copy.push_error(),
        SyncError::Cloud(CloudError::NotAuthenticated)
    ));
    assert_eq!(copy.shader(), "a stranger's idea");
    assert_eq!(dome.server_heads(), vec![dome.head()]);
}

/// The whole access model in one story: a link that starts as a view, a
/// visitor who opens it with no account at all, and one flip of a switch by
/// the owner after which that same visitor's work lands on the project.
#[test]
fn opening_the_link_to_edit_lets_a_visitor_push() {
    let td = TestWorld::new();
    let owner = td.user();
    let visitor = td.visitor();

    let dome = owner.project();
    let link = dome.publish(Access::View);

    let copy = visitor.open_shared(&link);
    assert_eq!(copy.shader(), "first light");
    copy.edit("the visitor's idea");
    assert!(matches!(
        copy.push_error(),
        SyncError::Cloud(CloudError::NotAuthenticated)
    ));

    dome.set_access(Access::Edit);

    assert!(copy.push().advanced());
    assert_eq!(dome.server_heads(), vec![copy.head()]);

    // And the owner sees it as an ordinary fast-forward from a collaborator.
    let pulled = dome.pull();
    assert!(pulled.can_fast_forward());
    dome.fast_forward(&pulled);
    assert_eq!(dome.shader(), "the visitor's idea");
}

/// Archiving takes the project off the link without touching anybody's
/// library, and restoring puts it back exactly as it was.
#[test]
fn archiving_takes_the_link_down_and_restoring_brings_it_back() {
    let td = TestWorld::new();
    let owner = td.user();
    let visitor = td.visitor();

    let dome = owner.project();
    let link = dome.publish(Access::View);
    let copy = visitor.open_shared(&link);

    dome.archive();

    assert!(matches!(
        copy.pull_error(),
        SyncError::Cloud(CloudError::NotFound)
    ));
    assert_eq!(copy.shader(), "first light", "their copy is untouched");
    // Its owner cannot push to it either — an archive is a freeze, not a
    // one-way door.
    dome.edit("one more thought");
    assert!(matches!(
        dome.push_error(),
        SyncError::Cloud(CloudError::InvalidRequest { .. })
    ));

    dome.restore();

    assert!(dome.push().advanced());
    let caught_up = copy.pull();
    assert!(caught_up.can_fast_forward());
    copy.fast_forward(&caught_up);
    assert_eq!(copy.shader(), "one more thought");
}

/// Unsharing stops the conversation. It does not reach into anybody's
/// library.
#[test]
fn closing_the_link_revokes_the_view() {
    let td = TestWorld::new();
    let owner = td.user();
    let visitor = td.visitor();

    let dome = owner.project();
    let link = dome.publish(Access::View);
    let copy = visitor.open_shared(&link);

    dome.set_access(Access::None);

    assert!(matches!(
        copy.pull_error(),
        SyncError::Cloud(CloudError::NotFound)
    ));
    assert_eq!(copy.shader(), "first light");
    assert_eq!(copy.head(), dome.head());

    copy.edit("still mine to change");
    assert_eq!(copy.shader(), "still mine to change");
}
