//! Resolving a divergence by choosing one side wholesale.

use lpc_cloud_api::SidecarMeta;
use lpc_history::ContentHash;

use crate::cloud_port::CloudPort;
use crate::local_project::LocalProject;
use crate::sync::push::{PushReport, push};
use crate::sync_error::SyncError;

/// Which side of a divergence wins.
///
/// Nothing loses permanently: the other side is set aside, stays in the
/// snapshot store, and classifies as [`SyncRelation::Behind`](lpc_history::SyncRelation)
/// forever after — which is what lets a peer still carrying it fast-forward
/// instead of re-diverging, and what lets the history UI restore it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClobberSide {
    /// Keep the local version.
    Ours,
    /// Adopt the service's version.
    Theirs,
}

/// What resolving produced.
#[derive(Debug, Clone, PartialEq)]
pub struct ClobberReport {
    /// The version the line continues at.
    pub kept: ContentHash,
    /// The version set aside — reachable, not destroyed.
    pub set_aside: ContentHash,
    /// The push of the join, which collapses the service's frontier back to
    /// one head.
    pub push: PushReport,
}

/// Resolve a divergence against `theirs`, then push the join.
///
/// A join is the DAG's only non-linear node: two parents, content equal to
/// the chosen one. It is recorded locally and then pushed naming **both**
/// sides as parents, which is what collapses a two-head frontier back to one
/// (`ProjectRefs::apply_push` consumes every head a push names).
///
/// # What is banked, and when
///
/// Before anything is recorded:
///
/// - The **working copy** is snapshotted. If it holds content the history has
///   not recorded, this refuses with [`SyncError::UncommittedLocalEdits`] —
///   *after* banking it, so the hash in that error is a real, restorable
///   version. Resolve a divergence from a saved state; the alternative is a
///   join whose losing side was never a version anybody can get back to.
/// - `theirs` must already be in the local snapshot store
///   ([`pull`](crate::sync::pull) puts it there, for every head). Adopting a
///   version whose content the client does not hold would produce a head it
///   cannot check out.
///
/// Only then is the join recorded, and only then — for
/// [`ClobberSide::Theirs`] — is the working copy replaced.
pub async fn resolve_clobber<P: CloudPort + ?Sized>(
    port: &P,
    project: &LocalProject<'_>,
    theirs: ContentHash,
    keep: ClobberSide,
    at: f64,
    sidecar: &SidecarMeta,
) -> Result<ClobberReport, SyncError> {
    let mut history = project.history()?;
    let ours = history.head().ok_or(SyncError::NoLocalHead)?;

    // Bank first: after this line nothing in the working directory can be
    // lost, whichever way the rest goes.
    let banked = project.bank_working_copy()?;
    if banked != ours {
        return Err(SyncError::UncommittedLocalEdits { banked, head: ours });
    }
    if !project.snapshots().has(&theirs)? {
        return Err(SyncError::UnbankedVersion(theirs));
    }

    let (kept, set_aside) = match keep {
        ClobberSide::Ours => (ours, theirs),
        ClobberSide::Theirs => (theirs, ours),
    };

    let event = history.record_join(kept, set_aside, at)?;
    project.event_log().append(&event)?;
    if kept != ours {
        project.checkout(kept)?;
    }

    let push = push(port, project, sidecar).await?;
    Ok(ClobberReport {
        kept,
        set_aside,
        push,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block_on::block_on;
    use crate::sync::apply::apply_fast_forward;
    use crate::sync::publish::publish;
    use crate::sync::pull::pull;
    use crate::test_support::{TestWorld, sidecar};
    use lpc_cloud_api::Visibility;
    use lpc_history::SyncRelation;

    /// Keep mine: the service ends up with one head, and the loser is still
    /// reachable here.
    #[test]
    fn keeping_ours_collapses_the_frontier_and_keeps_the_loser() {
        let world = TestWorld::new();
        let scene = diverged(&world);

        let report = block_on(resolve_clobber(
            &scene.owner,
            &scene.ours,
            scene.theirs_version,
            ClobberSide::Ours,
            5.0,
            &sidecar("Dome"),
        ))
        .unwrap();

        assert_eq!(report.kept, scene.our_version);
        assert_eq!(report.set_aside, scene.theirs_version);
        assert!(report.push.advanced());
        assert_eq!(report.push.heads().len(), 1);
        assert_eq!(report.push.heads()[0].tree, scene.our_version);

        let history = scene.ours.history().unwrap();
        assert_eq!(history.head(), Some(scene.our_version));
        assert!(history.superseded(scene.theirs_version));
        assert_eq!(history.classify(scene.theirs_version), SyncRelation::Behind);
        // and the losing content is still checkout-able
        scene.ours.checkout(scene.theirs_version).unwrap();
    }

    /// Use theirs: the working copy changes, and *our* version becomes the
    /// reachable past.
    #[test]
    fn keeping_theirs_adopts_their_content_and_keeps_ours() {
        let world = TestWorld::new();
        let scene = diverged(&world);

        let report = block_on(resolve_clobber(
            &scene.owner,
            &scene.ours,
            scene.theirs_version,
            ClobberSide::Theirs,
            5.0,
            &sidecar("Dome"),
        ))
        .unwrap();

        assert_eq!(report.kept, scene.theirs_version);
        assert_eq!(report.set_aside, scene.our_version);
        assert!(report.push.advanced());
        assert_eq!(report.push.heads().len(), 1);
        assert_eq!(
            world.project(1).read("/project.json"),
            Some(b"{\"them\":true}".to_vec())
        );

        let history = scene.ours.history().unwrap();
        assert_eq!(history.head(), Some(scene.theirs_version));
        assert_eq!(history.classify(scene.our_version), SyncRelation::Behind);
        scene.ours.checkout(scene.our_version).unwrap();
    }

    /// The other side of a resolved clobber: the peer still carrying the
    /// losing version is *Behind*, not diverged, and fast-forwards.
    #[test]
    fn the_loser_fast_forwards_on_its_next_pull() {
        let world = TestWorld::new();
        let scene = diverged(&world);
        block_on(resolve_clobber(
            &scene.owner,
            &scene.ours,
            scene.theirs_version,
            ClobberSide::Ours,
            5.0,
            &sidecar("Dome"),
        ))
        .unwrap();

        let report = block_on(pull(&scene.member, &scene.theirs)).unwrap();
        assert!(report.can_fast_forward());
        let applied = apply_fast_forward(&scene.theirs, &report).unwrap();
        assert_eq!(applied.head, scene.our_version);
        assert_eq!(scene.theirs.head().unwrap(), Some(scene.our_version));
    }

    #[test]
    fn resolving_over_uncommitted_edits_is_refused_after_banking_them() {
        let world = TestWorld::new();
        let scene = diverged(&world);
        world.project(1).write("/project.json", b"{\"dirty\":true}");

        let error = block_on(resolve_clobber(
            &scene.owner,
            &scene.ours,
            scene.theirs_version,
            ClobberSide::Theirs,
            5.0,
            &sidecar("Dome"),
        ))
        .unwrap_err();

        let SyncError::UncommittedLocalEdits { banked, head } = error else {
            panic!("expected an uncommitted-edits refusal, got {error:?}");
        };
        assert_eq!(head, scene.our_version);
        // nothing was recorded, and the edits survive by hash
        assert_eq!(scene.ours.head().unwrap(), Some(scene.our_version));
        scene.ours.checkout(banked).unwrap();
        assert_eq!(
            world.project(1).read("/project.json"),
            Some(b"{\"dirty\":true}".to_vec())
        );
    }

    #[test]
    fn resolving_toward_content_we_do_not_hold_is_refused() {
        let world = TestWorld::new();
        let scene = diverged(&world);
        let stranger = ContentHash::of(b"never pulled");

        assert!(matches!(
            block_on(resolve_clobber(
                &scene.owner,
                &scene.ours,
                stranger,
                ClobberSide::Theirs,
                5.0,
                &sidecar("Dome"),
            )),
            Err(SyncError::UnbankedVersion(_))
        ));
        assert_eq!(scene.ours.head().unwrap(), Some(scene.our_version));
    }

    struct Diverged<'a> {
        owner: crate::InProcessCloud,
        member: crate::InProcessCloud,
        ours: LocalProject<'a>,
        theirs: LocalProject<'a>,
        our_version: ContentHash,
        theirs_version: ContentHash,
    }

    /// Two members, one base, one edit each, both pushed: the service is
    /// holding two heads and the owner has pulled the other side.
    fn diverged(world: &TestWorld) -> Diverged<'_> {
        let owner = world.user("yona");
        let dome = world.project(1);
        let ours = dome.init();
        dome.write("/project.json", b"{}");
        ours.save(2.0).unwrap();
        block_on(publish(
            &owner,
            &ours,
            Visibility::Link,
            "dome",
            &sidecar("Dome"),
        ))
        .unwrap();

        let member = world.member(&owner, dome.uid(), "oliver");
        let copy = world.tracking_copy(&member, 2, dome.uid());
        let theirs = copy.local_as(dome.uid());
        copy.write("/project.json", b"{\"them\":true}");
        let theirs_version = theirs.save(3.0).unwrap();
        block_on(crate::sync::push::push(&member, &theirs, &sidecar("Dome"))).unwrap();

        dome.write("/project.json", b"{\"us\":true}");
        let our_version = ours.save(4.0).unwrap();
        block_on(crate::sync::push::push(&owner, &ours, &sidecar("Dome"))).unwrap();

        // the owner pulls, which banks their version and reports divergence
        let report = block_on(pull(&owner, &ours)).unwrap();
        assert!(report.is_diverged());

        Diverged {
            owner,
            member,
            ours,
            theirs,
            our_version,
            theirs_version,
        }
    }
}
