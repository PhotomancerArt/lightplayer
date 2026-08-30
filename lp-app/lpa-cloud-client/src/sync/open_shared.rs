//! Opening somebody else's project as a tracking copy.

use alloc::vec::Vec;

use lpc_cloud_api::{HeadInfo, ProjectMeta, SidecarMeta};
use lpc_history::ContentHash;

use crate::cloud_binding::CloudBinding;
use crate::cloud_port::CloudPort;
use crate::local_project::LocalProject;
use crate::sync::content_transfer::fetch_version;
use crate::sync::pull::choose_remote_head;
use crate::sync::service_calls::{get_events, get_project};
use crate::sync_error::SyncError;

/// The copy that opening a share link produced.
#[derive(Debug, Clone, PartialEq)]
pub struct OpenSharedReport {
    /// The service's record of the project.
    pub meta: ProjectMeta,
    /// Display metadata from the service's most recent commit — the name
    /// the copy's library entry should wear.
    pub sidecar: SidecarMeta,
    /// Who has access by email, when the caller is entitled to know —
    /// `Some` only for a member (the anti-oracle rule). Carried so the
    /// caller can classify itself (member / edit-visitor / view-visitor)
    /// without a second `GetProject`.
    pub members: Option<Vec<lpc_cloud_api::MemberInfo>>,
    /// The service's frontier.
    pub heads: Vec<HeadInfo>,
    /// The version checked out into the working copy.
    pub head: Option<ContentHash>,
    /// Events adopted as this copy's history.
    pub adopted_events: usize,
}

/// Pull a project the local library has never seen into a fresh copy.
///
/// This is Q10, ratified: opening a share link produces a **tracking copy**,
/// not a viewer mode. There is no read-only UI to build — a viewer who is not
/// a member simply has pushes refused by the service, and everything else
/// about their copy is an ordinary local project that pulls.
///
/// Two things make it a copy of *the same project* rather than an import:
///
/// - **The uid is preserved** (D17). The copy carries the project's own uid,
///   which is why its next [`pull`](crate::sync::pull) is a fast-forward and
///   not a stranger's history.
/// - **The service's log becomes the history, verbatim.** No origin event is
///   minted. The pulled log already has one — the project's real one — and
///   inventing an "imported" event on top would be a permanent lie in a
///   record the user can never correct.
///
/// The target must be a fresh copy: this refuses a history root that already
/// has a log ([`SyncError::LocalHistoryExists`]).
pub async fn open_shared<P: CloudPort + ?Sized>(
    port: &P,
    project: &LocalProject<'_>,
) -> Result<OpenSharedReport, SyncError> {
    if !project.event_log().read_all()?.is_empty() {
        return Err(SyncError::LocalHistoryExists);
    }

    let remote = get_project(port, project.uid()).await?;
    let log = get_events(port, project.uid(), 0).await?;
    if log.events.is_empty() {
        return Err(SyncError::NoRemoteHead);
    }

    for head in &remote.heads {
        fetch_version(port, project, head.tree).await?;
    }

    // A fresh copy has no head of its own, so the frontier's first head is
    // the one to check out — and with a single head (the normal case) it is
    // also the log's replayed tip.
    let head = choose_remote_head(&remote.heads, None);

    let event_log = project.event_log();
    for event in &log.events {
        event_log.append(event)?;
    }

    let mut binding = CloudBinding::new(project.uid());
    binding.observe_heads(&remote.heads);
    binding.last_event_seq = log.next_since;
    project.put_binding(&binding)?;

    if let Some(head) = head {
        project.checkout(head)?;
    }

    Ok(OpenSharedReport {
        meta: remote.meta,
        sidecar: remote.sidecar,
        members: remote.members,
        heads: remote.heads,
        head,
        adopted_events: log.events.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block_on::block_on;
    use crate::sync::publish::publish;
    use crate::sync::pull::pull;
    use crate::test_support::{TestWorld, sidecar};
    use lpc_cloud_api::{Access, CloudError};

    #[test]
    fn an_anonymous_viewer_gets_content_history_and_a_binding() {
        let world = TestWorld::new();
        let yona = world.user("yona");
        let dome = world.project(1);
        let local = dome.init();
        dome.write("/project.json", b"{}");
        dome.write("/shader.glsl", b"void main() {}");
        local.save(2.0).unwrap();
        dome.write("/shader.glsl", b"void main() { }");
        let v2 = local.save(3.0).unwrap();
        let published = block_on(publish(
            &yona,
            &local,
            Access::View,
            "zook-dome",
            &sidecar("Zook Dome"),
        ))
        .unwrap();

        let viewer = world.anonymous();
        let copy = world.project(2);
        let tracking = copy.local_as(published.meta.uid);
        let report = block_on(open_shared(&viewer, &tracking)).unwrap();

        // same project, not a new one
        assert_eq!(report.meta.uid, dome.uid());
        assert_eq!(report.head, Some(v2));
        assert_eq!(report.adopted_events, 3);
        assert_eq!(copy.read("/shader.glsl"), Some(b"void main() { }".to_vec()));

        // the history is the project's own, origin and all — nothing minted
        let history = tracking.history().unwrap();
        assert_eq!(history.events(), local.history().unwrap().events());
        assert_eq!(tracking.require_binding().unwrap().project, dome.uid());

        // and it tracks: the next pull is a fast-forward question, not a
        // stranger's history
        assert!(block_on(pull(&viewer, &tracking)).unwrap().is_up_to_date());
    }

    #[test]
    fn a_private_project_is_not_found_by_a_stranger() {
        let world = TestWorld::new();
        let yona = world.user("yona");
        let dome = world.project(1);
        let local = dome.init();
        dome.write("/project.json", b"{}");
        local.save(2.0).unwrap();
        block_on(publish(
            &yona,
            &local,
            Access::None,
            "dome",
            &sidecar("Dome"),
        ))
        .unwrap();

        let viewer = world.anonymous();
        let copy = world.project(2);
        let error = block_on(open_shared(&viewer, &copy.local_as(dome.uid()))).unwrap_err();
        assert!(matches!(error, SyncError::Cloud(CloudError::NotFound)));
        assert!(copy.local_as(dome.uid()).binding().unwrap().is_none());
    }

    #[test]
    fn opening_over_an_existing_copy_is_refused() {
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

        assert!(matches!(
            block_on(open_shared(&yona, &local)),
            Err(SyncError::LocalHistoryExists)
        ));
    }
}
