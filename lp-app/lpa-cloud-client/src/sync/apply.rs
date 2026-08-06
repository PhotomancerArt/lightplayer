//! Adopting a pulled fast-forward into the local copy.

use lpc_history::{ContentHash, ProjectHistory};

use crate::local_project::LocalProject;
use crate::sync::event_diff::events_missing_from;
use crate::sync::pull::PullReport;
use crate::sync_error::SyncError;

/// What adopting the service's history did locally.
#[derive(Debug, Clone, PartialEq)]
pub struct ApplyReport {
    /// The version the working copy now holds.
    pub head: ContentHash,
    /// Events appended to the local log.
    pub applied_events: usize,
    /// Working-copy content that was not in the history, banked before it
    /// was overwritten. `Some` means the caller applied over uncommitted
    /// edits — allowed, never silent, and recoverable by this hash.
    pub banked_uncommitted: Option<ContentHash>,
}

/// Adopt the service's history and content into a local copy.
///
/// Only ever called on a fast-forward: the service's history has to already
/// contain the local head, or there is a choice to make and
/// [`resolve_clobber`](crate::sync::resolve_clobber) is the operation that
/// makes it.
///
/// Order matters, and it is: bank, validate, log, checkout.
///
/// 1. **Bank** the working copy. Idempotent and cheap; afterwards nothing in
///    the working directory can be lost, whatever happens next.
/// 2. **Validate** by replaying local events plus the service's new ones
///    through `ProjectHistory`, and check the result really does end at the
///    service's head. An event batch that does not replay is refused before
///    a byte is written.
/// 3. **Append** the new events to the local log.
/// 4. **Checkout** the new head into the working copy.
///
/// The caller decides *whether* to call this (see [`pull`](crate::sync::pull)
/// for the clean-copy rule). If it is called over uncommitted edits, those
/// edits are banked and reported, not silently dropped.
pub fn apply_fast_forward(
    project: &LocalProject<'_>,
    report: &PullReport,
) -> Result<ApplyReport, SyncError> {
    let head = report.remote_head.ok_or(SyncError::NoRemoteHead)?;
    if !report.can_fast_forward() && !report.is_up_to_date() {
        return Err(SyncError::NotAFastForward);
    }

    let history = project.history()?;
    let incoming = events_missing_from(&report.remote_events, history.events());

    if incoming.is_empty() && history.head() == Some(head) {
        // Already there. Deliberately does not touch the working copy: a
        // no-op that overwrote files would clobber an open editing session
        // for nothing.
        return Ok(ApplyReport {
            head,
            applied_events: 0,
            banked_uncommitted: None,
        });
    }

    let banked = project.bank_working_copy()?;
    let banked_uncommitted = (Some(banked) != history.head()).then_some(banked);

    let mut candidate_events = history.events().to_vec();
    candidate_events.extend(incoming.iter().cloned());
    let candidate = ProjectHistory::from_events(candidate_events)?;
    if candidate.head() != Some(head) {
        return Err(SyncError::NotAFastForward);
    }
    if !project.snapshots().has(&head)? {
        return Err(SyncError::UnbankedVersion(head));
    }

    let log = project.event_log();
    for event in &incoming {
        log.append(event)?;
    }
    project.checkout(head)?;

    Ok(ApplyReport {
        head,
        applied_events: incoming.len(),
        banked_uncommitted,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block_on::block_on;
    use crate::sync::publish::publish;
    use crate::sync::pull::pull;
    use crate::sync::push::push;
    use crate::test_support::{TestWorld, sidecar};
    use lpc_cloud_api::Visibility;
    use lpc_history::SyncRelation;

    #[test]
    fn a_tracking_copy_fast_forwards_to_the_new_head() {
        let world = TestWorld::new();
        let yona = world.user("yona");
        let dome = world.project(1);
        let local = dome.init();
        dome.write("/project.json", b"{}");
        local.save(2.0).unwrap();
        block_on(publish(
            &yona,
            &local,
            Visibility::Link,
            "dome",
            &sidecar("Dome"),
        ))
        .unwrap();

        let viewer = world.anonymous();
        let copy = world.tracking_copy(&viewer, 2, dome.uid());
        let tracking = copy.local_as(dome.uid());

        dome.write("/project.json", b"{\"n\":2}");
        let v2 = local.save(3.0).unwrap();
        block_on(push(&yona, &local, &sidecar("Dome"))).unwrap();

        let report = block_on(pull(&viewer, &tracking)).unwrap();
        let applied = apply_fast_forward(&tracking, &report).unwrap();

        assert_eq!(applied.head, v2);
        assert_eq!(applied.applied_events, 1);
        assert_eq!(applied.banked_uncommitted, None);
        assert_eq!(tracking.head().unwrap(), Some(v2));
        assert_eq!(copy.read("/project.json"), Some(b"{\"n\":2}".to_vec()));

        // and a second pull says so
        let again = block_on(pull(&viewer, &tracking)).unwrap();
        assert!(again.is_up_to_date());
    }

    #[test]
    fn applying_when_already_current_changes_nothing() {
        let world = TestWorld::new();
        let yona = world.user("yona");
        let dome = world.project(1);
        let local = dome.init();
        dome.write("/project.json", b"{}");
        local.save(2.0).unwrap();
        block_on(publish(
            &yona,
            &local,
            Visibility::Link,
            "dome",
            &sidecar("Dome"),
        ))
        .unwrap();

        let report = block_on(pull(&yona, &local)).unwrap();
        let applied = apply_fast_forward(&local, &report).unwrap();
        assert_eq!(applied.applied_events, 0);
        assert_eq!(applied.banked_uncommitted, None);
    }

    /// The D18 hazard, made non-destructive: applying over uncommitted edits
    /// is possible, reported, and recoverable — the caller's policy decides
    /// whether to do it at all.
    #[test]
    fn applying_over_uncommitted_edits_banks_them_first() {
        let world = TestWorld::new();
        let yona = world.user("yona");
        let dome = world.project(1);
        let local = dome.init();
        dome.write("/project.json", b"{}");
        local.save(2.0).unwrap();
        block_on(publish(
            &yona,
            &local,
            Visibility::Link,
            "dome",
            &sidecar("Dome"),
        ))
        .unwrap();

        let viewer = world.anonymous();
        let copy = world.tracking_copy(&viewer, 2, dome.uid());
        let tracking = copy.local_as(dome.uid());

        dome.write("/project.json", b"{\"n\":2}");
        let v2 = local.save(3.0).unwrap();
        block_on(push(&yona, &local, &sidecar("Dome"))).unwrap();

        copy.write("/project.json", b"{\"mine\":true}");
        let report = block_on(pull(&viewer, &tracking)).unwrap();
        let applied = apply_fast_forward(&tracking, &report).unwrap();

        assert_eq!(applied.head, v2);
        let rescued = applied.banked_uncommitted.expect("edits were banked");
        tracking.checkout(rescued).unwrap();
        assert_eq!(
            copy.read("/project.json"),
            Some(b"{\"mine\":true}".to_vec())
        );
    }

    #[test]
    fn a_divergence_is_not_applied() {
        let world = TestWorld::new();
        let yona = world.user("yona");
        let dome = world.project(1);
        let local = dome.init();
        dome.write("/project.json", b"{}");
        local.save(2.0).unwrap();
        block_on(publish(
            &yona,
            &local,
            Visibility::Link,
            "dome",
            &sidecar("Dome"),
        ))
        .unwrap();

        let oliver = world.member(&yona, dome.uid(), "oliver");
        let copy = world.tracking_copy(&oliver, 2, dome.uid());
        let theirs = copy.local_as(dome.uid());
        copy.write("/project.json", b"{\"them\":true}");
        theirs.save(3.0).unwrap();
        block_on(push(&oliver, &theirs, &sidecar("Dome"))).unwrap();

        dome.write("/project.json", b"{\"us\":true}");
        let ours = local.save(4.0).unwrap();

        let report = block_on(pull(&yona, &local)).unwrap();
        assert!(matches!(
            apply_fast_forward(&local, &report),
            Err(SyncError::NotAFastForward)
        ));
        assert_eq!(local.head().unwrap(), Some(ours));
        assert_eq!(
            local.history().unwrap().classify(ours),
            SyncRelation::AtHead
        );
    }
}
