//! Fetching what the service has, and working out what it means.

use alloc::vec::Vec;

use lpc_cloud_api::{HeadInfo, MemberInfo, ProjectMeta, SidecarMeta};
use lpc_history::{ContentHash, HistoryEvent, ProjectHistory, SyncRelation};

use crate::cloud_binding::CloudBinding;
use crate::cloud_port::CloudPort;
use crate::local_project::LocalProject;
use crate::sync::content_transfer::fetch_version;
use crate::sync::service_calls::{get_events, get_project};
use crate::sync_error::SyncError;

/// What the service has, and how it relates to the local copy.
///
/// A pull **banks and reports**; it never touches the working copy. Applying
/// is a separate act ([`apply_fast_forward`](crate::sync::apply_fast_forward),
/// [`resolve_clobber`](crate::sync::resolve_clobber)) because only applying
/// can lose anything, and the rules for when it is allowed are policy the
/// caller owns — see the crate README.
#[derive(Debug, Clone, PartialEq)]
pub struct PullReport {
    /// The service's record of the project.
    pub meta: ProjectMeta,
    /// Display metadata from the service's most recent commit.
    pub sidecar: SidecarMeta,
    /// The member roster, when the caller is entitled to it — `Some` only
    /// for a member (P2's anti-oracle rule). Carried so a pull loop can
    /// observe a membership change without a second `GetProject`.
    pub members: Option<Vec<MemberInfo>>,
    /// The service's head frontier. More than one entry means the service is
    /// holding a divergence somebody has to resolve.
    pub heads: Vec<HeadInfo>,
    /// The service's whole event log — the canonical, interleaved history.
    pub remote_events: Vec<HistoryEvent>,
    /// The version the service's history currently ends at.
    pub remote_head: Option<ContentHash>,
    /// The local head this report was computed against.
    pub local_head: Option<ContentHash>,
    /// **How the service's history sees our head.** This is the
    /// fast-forward decision, and it is deliberately taken from the
    /// service's side: a version superseded by somebody else's clobber join
    /// reads [`SyncRelation::Behind`] there, which is exactly the "peer
    /// carrying the losing side fast-forwards instead of re-diverging" rule
    /// `lpc-history` documents.
    pub relation: SyncRelation,
    /// How our history sees the service's head — `None` when the service has
    /// no commits. Together with `relation` this separates "we have unpushed
    /// work" from "we genuinely diverged": see [`local_ahead`](Self::local_ahead).
    pub local_relation: Option<SyncRelation>,
    /// The service event sequence number to continue from.
    pub next_since: u64,
    /// Versions whose content this pull banked locally.
    pub fetched_versions: Vec<ContentHash>,
}

impl PullReport {
    /// Both sides are at the same version.
    pub fn is_up_to_date(&self) -> bool {
        self.relation == SyncRelation::AtHead
    }

    /// The service's history contains our head, so adopting it loses
    /// nothing. This is the auto-apply case (D5) — subject to the caller's
    /// clean-copy rule.
    pub fn can_fast_forward(&self) -> bool {
        self.relation == SyncRelation::Behind && self.remote_head.is_some()
    }

    /// We have work the service has not seen, and we have seen everything it
    /// has. Push, do not pull.
    pub fn local_ahead(&self) -> bool {
        if self.local_head.is_none() {
            return false;
        }
        match self.local_relation {
            None => true,
            Some(SyncRelation::AtHead) | Some(SyncRelation::Behind) => {
                self.relation == SyncRelation::Diverged
            }
            Some(SyncRelation::Diverged) => false,
        }
    }

    /// Neither side contains the other: a real divergence, resolvable only
    /// by choosing ([`resolve_clobber`](crate::sync::resolve_clobber)).
    pub fn is_diverged(&self) -> bool {
        self.relation == SyncRelation::Diverged && !self.local_ahead()
    }
}

/// Fetch the service's heads, events, and content, and classify.
///
/// Everything the service holds is banked into the local snapshot store
/// before anything is classified, including *every* head when the service is
/// holding a divergence — a client cannot offer "keep theirs" over content it
/// does not have.
///
/// # Caller contract (D5/D18)
///
/// This function applies nothing. When [`PullReport::can_fast_forward`] is
/// true the caller decides whether to adopt:
///
/// - a **clean** copy (working copy equal to its head, no open uncommitted
///   edits) fast-forwards — that is the Dropbox-shaped behavior, toast and
///   all;
/// - a copy with **uncommitted edits** does not. It badges instead. The user
///   saves, and then it is an ordinary divergence to resolve.
///
/// This crate enforces neither rule; it makes the losing paths impossible
/// instead, by banking before adopting.
pub async fn pull<P: CloudPort + ?Sized>(
    port: &P,
    project: &LocalProject<'_>,
) -> Result<PullReport, SyncError> {
    let remote = get_project(port, project.uid()).await?;
    let log = get_events(port, project.uid(), 0).await?;

    let mut fetched_versions = Vec::new();
    for head in &remote.heads {
        if fetch_version(port, project, head.tree).await? {
            fetched_versions.push(head.tree);
        }
    }

    let remote_history = replay(&log.events);
    let local_history = project.history()?;
    let local_head = local_history.head();
    let remote_head = choose_remote_head(&remote.heads, local_head);
    let relation = classify_local_head(&remote.heads, remote_history.as_ref(), local_head);
    let local_relation = remote_head.map(|head| local_history.classify(head));

    let mut binding = project
        .binding()?
        .unwrap_or_else(|| CloudBinding::new(project.uid()));
    binding.observe_heads(&remote.heads);
    binding.last_event_seq = log.next_since;
    project.put_binding(&binding)?;

    Ok(PullReport {
        meta: remote.meta,
        sidecar: remote.sidecar,
        members: remote.members,
        heads: remote.heads,
        remote_events: log.events,
        remote_head,
        local_head,
        relation,
        local_relation,
        next_since: log.next_since,
        fetched_versions,
    })
}

/// Which service head this pull is about.
///
/// The **frontier** decides, not the log's replayed tip. The service's log is
/// one flat, interleaved sequence, so when two clients push from one base the
/// log ends at whichever push landed second — which would make the pusher
/// look up to date while the service is visibly holding two heads. The
/// frontier cannot lie that way.
///
/// With two heads and one of them ours, the interesting one is the *other*:
/// that is the version a resolution has to choose against. Heads arrive
/// sorted by tree hash, so the choice is deterministic.
pub(crate) fn choose_remote_head(
    heads: &[HeadInfo],
    local_head: Option<ContentHash>,
) -> Option<ContentHash> {
    if heads.len() > 1 && local_head.is_some() {
        if let Some(other) = heads
            .iter()
            .map(|head| head.tree)
            .find(|tree| Some(*tree) != local_head)
        {
            return Some(other);
        }
    }
    heads.first().map(|head| head.tree)
}

/// How the service's history sees our head.
fn classify_local_head(
    heads: &[HeadInfo],
    remote_history: Option<&ProjectHistory>,
    local_head: Option<ContentHash>,
) -> SyncRelation {
    let Some(local) = local_head else {
        // Nothing local to lose: whatever the service has is ahead of us.
        return if heads.is_empty() {
            SyncRelation::AtHead
        } else {
            SyncRelation::Behind
        };
    };

    // Our head is one of several live tips: the service is holding our
    // divergence, whatever order its log happens to be in.
    if heads.len() > 1 && heads.iter().any(|head| head.tree == local) {
        return SyncRelation::Diverged;
    }

    match remote_history {
        Some(history) => history.classify(local),
        // The service's log does not replay as one line (it is the
        // interleaving of every client's, and a divergent batch need not
        // replay — see `lp-cloud-domain::push_validation`). Fall back to the
        // frontier, which is true even when the ordering is not.
        None => {
            if heads.iter().any(|head| head.tree == local) {
                SyncRelation::AtHead
            } else {
                SyncRelation::Diverged
            }
        }
    }
}

/// Replay the service's log, tolerating one that does not.
pub(crate) fn replay(events: &[HistoryEvent]) -> Option<ProjectHistory> {
    if events.is_empty() {
        return None;
    }
    match ProjectHistory::from_events(events.to_vec()) {
        Ok(history) => Some(history),
        Err(error) => {
            log::warn!("service history does not replay as one line: {error}");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block_on::block_on;
    use crate::sync::publish::publish;
    use crate::sync::push::push;
    use crate::test_support::{TestWorld, sidecar};
    use lpc_cloud_api::{Access, CloudError};

    #[test]
    fn a_pull_with_nothing_new_is_up_to_date() {
        let world = TestWorld::new();
        let yona = world.user("yona");
        let dome = world.project(1);
        let local = dome.init();
        dome.write("/project.json", b"{}");
        let v1 = local.save(2.0).unwrap();
        block_on(publish(
            &yona,
            &local,
            Access::View,
            "dome",
            &sidecar("Dome"),
        ))
        .unwrap();

        let report = block_on(pull(&yona, &local)).unwrap();
        assert!(report.is_up_to_date());
        assert!(!report.can_fast_forward());
        assert!(!report.is_diverged());
        assert_eq!(report.remote_head, Some(v1));
        assert!(report.fetched_versions.is_empty());
    }

    #[test]
    fn a_tracking_copy_sees_a_fast_forward_and_banks_the_content() {
        let world = TestWorld::new();
        let yona = world.user("yona");
        let dome = world.project(1);
        let local = dome.init();
        dome.write("/project.json", b"{}");
        local.save(2.0).unwrap();
        block_on(publish(
            &yona,
            &local,
            Access::View,
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
        assert!(report.can_fast_forward());
        assert!(!report.is_diverged());
        assert_eq!(report.remote_head, Some(v2));
        assert_eq!(report.fetched_versions, alloc::vec![v2]);
        // banked, but not adopted: the working copy is untouched
        assert!(tracking.snapshots().has(&v2).unwrap());
        assert_eq!(copy.read("/project.json"), Some(b"{}".to_vec()));
    }

    #[test]
    fn unpushed_local_work_reads_as_ahead_not_diverged() {
        let world = TestWorld::new();
        let yona = world.user("yona");
        let dome = world.project(1);
        let local = dome.init();
        dome.write("/project.json", b"{}");
        local.save(2.0).unwrap();
        block_on(publish(
            &yona,
            &local,
            Access::View,
            "dome",
            &sidecar("Dome"),
        ))
        .unwrap();

        dome.write("/project.json", b"{\"n\":2}");
        local.save(3.0).unwrap();

        let report = block_on(pull(&yona, &local)).unwrap();
        assert!(report.local_ahead());
        assert!(!report.is_diverged());
        assert!(!report.can_fast_forward());
    }

    #[test]
    fn two_lines_from_one_base_read_as_diverged() {
        let world = TestWorld::new();
        let yona = world.user("yona");
        let dome = world.project(1);
        let local = dome.init();
        dome.write("/project.json", b"{}");
        local.save(2.0).unwrap();
        block_on(publish(
            &yona,
            &local,
            Access::View,
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
        local.save(4.0).unwrap();

        let report = block_on(pull(&yona, &local)).unwrap();
        assert!(report.is_diverged());
        assert!(!report.can_fast_forward());
        assert!(!report.local_ahead());
        // their version is banked here, ready for a "keep theirs"
        assert_eq!(report.fetched_versions.len(), 1);
    }

    /// Q12: flipping to private revokes the link view. The local copy is
    /// still perfectly usable — it just stops hearing anything.
    #[test]
    fn a_revoked_link_stops_answering_but_leaves_the_copy_alone() {
        let world = TestWorld::new();
        let yona = world.user("yona");
        let dome = world.project(1);
        let local = dome.init();
        dome.write("/project.json", b"{}");
        let v1 = local.save(2.0).unwrap();
        block_on(publish(
            &yona,
            &local,
            Access::View,
            "dome",
            &sidecar("Dome"),
        ))
        .unwrap();

        let viewer = world.anonymous();
        let copy = world.tracking_copy(&viewer, 2, dome.uid());
        let tracking = copy.local_as(dome.uid());
        world.set_access(&yona, dome.uid(), Access::None);

        let error = block_on(pull(&viewer, &tracking)).unwrap_err();
        assert!(matches!(error, SyncError::Cloud(CloudError::NotFound)));
        assert_eq!(tracking.head().unwrap(), Some(v1));
        assert_eq!(copy.read("/project.json"), Some(b"{}".to_vec()));
    }
}
