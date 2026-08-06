//! Sending local work to the service. One attempt, typed outcome.

use alloc::vec::Vec;

use lpc_cloud_api::{HeadInfo, PushOutcome, SidecarMeta};
use lpc_history::ContentHash;

use crate::cloud_binding::CloudBinding;
use crate::cloud_port::CloudPort;
use crate::local_project::LocalProject;
use crate::sync::commit_parents::parents_of_head;
use crate::sync::content_transfer::upload_version;
use crate::sync::event_diff::events_missing_from;
use crate::sync::service_calls::{get_events, get_heads, push_commit};
use crate::sync_error::SyncError;

/// What one push attempt did.
#[derive(Debug, Clone, PartialEq)]
pub enum PushReport {
    /// The service already had this head and every local event. Nothing was
    /// sent.
    UpToDate {
        /// The service's frontier.
        heads: Vec<HeadInfo>,
    },
    /// A commit was accepted. Push is never refused for divergence (D5), so
    /// the only question the outcome answers is whether the line advanced or
    /// gained a sibling.
    Pushed {
        /// Whether the commit continued the line or created a second head.
        outcome: PushOutcome,
        /// The service's frontier after the push.
        heads: Vec<HeadInfo>,
        /// Blobs and trees uploaded to satisfy the commit.
        uploaded_objects: usize,
        /// History events the commit carried.
        sent_events: usize,
    },
}

impl PushReport {
    /// The service's frontier as of this attempt.
    pub fn heads(&self) -> &[HeadInfo] {
        match self {
            PushReport::UpToDate { heads } => heads,
            PushReport::Pushed { heads, .. } => heads,
        }
    }

    /// What the push did to the frontier, if anything was pushed.
    pub fn outcome(&self) -> Option<PushOutcome> {
        match self {
            PushReport::UpToDate { .. } => None,
            PushReport::Pushed { outcome, .. } => Some(*outcome),
        }
    }

    /// Whether the push continued the service's line.
    pub fn advanced(&self) -> bool {
        self.outcome() == Some(PushOutcome::Advanced)
    }

    /// Whether the push landed alongside an existing head — a divergence,
    /// recorded rather than refused.
    pub fn created_new_head(&self) -> bool {
        self.outcome() == Some(PushOutcome::NewHead)
    }
}

/// Push the project's current head to the service. **One attempt.**
///
/// The sequence is the pre-flight one: ask what the service is missing
/// (`HaveBlobs`), upload exactly that, then `PushCommit`. Nothing here loops
/// and nothing here waits — an unreachable service comes back as
/// [`SyncError::Transport`] and the caller owns the retry (that is what makes
/// "offline" a caller state rather than a mode this crate has).
///
/// What is sent is the *difference* between the local log and the service's,
/// not a positional suffix: the service's log is the interleaving of every
/// client's line, so a client whose events are already there sends nothing
/// and one whose events were interleaved with somebody else's still sends
/// only its own.
pub async fn push<P: CloudPort + ?Sized>(
    port: &P,
    project: &LocalProject<'_>,
    sidecar: &SidecarMeta,
) -> Result<PushReport, SyncError> {
    let history = project.history()?;
    let head = history.head().ok_or(SyncError::NoLocalHead)?;

    let service_heads = get_heads(port, project.uid()).await?;
    let service_log = get_events(port, project.uid(), 0).await?;
    let outgoing = events_missing_from(history.events(), &service_log.events);

    let mut binding = project
        .binding()?
        .unwrap_or_else(|| CloudBinding::new(project.uid()));
    binding.last_event_seq = service_log.next_since;

    if outgoing.is_empty() {
        if !service_heads.iter().any(|h| h.tree == head) {
            // The service has all our events but not our head: it is holding
            // a line whose replay put a different version last. Nothing to
            // send — the divergence is resolved by a join, not a re-push.
            log::warn!(
                "push: service has every local event but head {} is not a service head",
                head.short()
            );
        }
        binding.observe_heads(&service_heads);
        project.put_binding(&binding)?;
        return Ok(PushReport::UpToDate {
            heads: service_heads,
        });
    }

    let parents = parents_of_head(&history, head, &service_heads);
    // The preview blob rides the same pre-flight as the version's own files:
    // the service checks for it on push like any other referenced hash.
    let preview: Vec<ContentHash> = sidecar.preview_png.into_iter().collect();
    let uploaded_objects = upload_version(port, project, head, &preview).await?;

    let sent_events = outgoing.len();
    let (outcome, heads) = push_commit(
        port,
        project.uid(),
        parents,
        head,
        outgoing,
        sidecar.clone(),
    )
    .await?;

    binding.observe_heads(&heads);
    project.put_binding(&binding)?;
    Ok(PushReport::Pushed {
        outcome,
        heads,
        uploaded_objects,
        sent_events,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block_on::block_on;
    use crate::cloud_port::TransportError;
    use crate::sync::publish::publish;
    use crate::test_support::{TestWorld, sidecar};
    use lpc_cloud_api::Visibility;

    #[test]
    fn a_second_push_advances_the_line() {
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

        dome.write("/project.json", b"{\"n\":2}");
        local.save(3.0).unwrap();
        let report = block_on(push(&yona, &local, &sidecar("Dome"))).unwrap();

        assert!(report.advanced());
        assert_eq!(report.heads().len(), 1);
        assert_eq!(report.heads()[0].tree, local.head().unwrap().unwrap());
        assert!(matches!(report, PushReport::Pushed { sent_events: 1, .. }));
    }

    #[test]
    fn pushing_twice_with_nothing_new_sends_nothing() {
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

        let report = block_on(push(&yona, &local, &sidecar("Dome"))).unwrap();
        assert!(matches!(report, PushReport::UpToDate { .. }));
        assert_eq!(report.heads().len(), 1);
    }

    /// The D5 rule from the client's side: a push from a stale base is
    /// accepted as a second head, not refused.
    #[test]
    fn a_push_from_a_stale_base_becomes_a_second_head() {
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

        // somebody else advances the service's line from the same base
        let oliver = world.member(&yona, dome.uid(), "oliver");
        let their_copy = world.tracking_copy(&oliver, 2, dome.uid());
        their_copy.write("/project.json", b"{\"them\":true}");
        let theirs = their_copy.local_as(dome.uid());
        theirs.save(3.0).unwrap();
        assert!(
            block_on(push(&oliver, &theirs, &sidecar("Dome")))
                .unwrap()
                .advanced()
        );

        // ours, from the base we still think is current
        dome.write("/project.json", b"{\"us\":true}");
        local.save(4.0).unwrap();
        let report = block_on(push(&yona, &local, &sidecar("Dome"))).unwrap();
        assert!(report.created_new_head());
        assert_eq!(report.heads().len(), 2);
    }

    /// Offline is one failed attempt, not a queue: the local work is
    /// untouched and the same push succeeds later.
    #[test]
    fn an_offline_push_fails_and_retries_clean() {
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

        yona.go_offline();
        dome.write("/project.json", b"{\"n\":2}");
        let v2 = local.save(3.0).unwrap();
        assert!(matches!(
            block_on(push(&yona, &local, &sidecar("Dome"))),
            Err(SyncError::Transport(TransportError::Offline))
        ));
        assert_eq!(local.head().unwrap(), Some(v2));

        yona.go_online();
        let report = block_on(push(&yona, &local, &sidecar("Dome"))).unwrap();
        assert!(report.advanced());
        assert_eq!(report.heads()[0].tree, v2);
    }

    #[test]
    fn a_project_with_no_saved_version_has_nothing_to_push() {
        let world = TestWorld::new();
        let yona = world.user("yona");
        let dome = world.project(1);
        let local = dome.init();
        assert!(matches!(
            block_on(push(&yona, &local, &sidecar("Dome"))),
            Err(SyncError::NoLocalHead)
        ));
    }
}
