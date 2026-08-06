//! Putting a local project on the cloud for the first time.

use alloc::string::String;

use lpc_cloud_api::{ProjectMeta, SidecarMeta, Visibility};

use crate::cloud_binding::CloudBinding;
use crate::cloud_port::CloudPort;
use crate::local_project::LocalProject;
use crate::project_link::ProjectLink;
use crate::sync::push::{PushReport, push};
use crate::sync::service_calls::publish_project;
use crate::sync_error::SyncError;

/// What publishing produced.
#[derive(Debug, Clone, PartialEq)]
pub struct PublishReport {
    /// The service's record of the project: uid, slug, visibility, owner.
    pub meta: ProjectMeta,
    /// The initial push of the project's content and history.
    pub push: PushReport,
}

impl PublishReport {
    /// The canonical share address for the published project.
    pub fn link(&self) -> ProjectLink {
        ProjectLink::new(self.meta.slug.clone(), self.meta.uid)
    }
}

/// Publish a project: create its cloud record, then push its history.
///
/// The uid is **not minted here**. The project already has one, it was minted
/// locally (D21), and publishing records that uid rather than assigning a new
/// one — which is what lets the same project exist in two libraries without
/// becoming two projects (D17). It is also the share token: 95 bits of
/// keyspace, so holding the link is the permission.
///
/// Publishing an already-published project of your own restates its slug and
/// visibility and pushes whatever is new; it is not an error. Publishing a
/// uid somebody else owns answers `NotFound` — the same answer as a uid that
/// was never published, because the alternative turns the endpoint into an
/// oracle for which uids exist.
pub async fn publish<P: CloudPort + ?Sized>(
    port: &P,
    project: &LocalProject<'_>,
    visibility: Visibility,
    slug: impl Into<String>,
    sidecar: &SidecarMeta,
) -> Result<PublishReport, SyncError> {
    let remote = publish_project(port, project.uid(), visibility, slug.into()).await?;

    let mut binding = project
        .binding()?
        .unwrap_or_else(|| CloudBinding::new(project.uid()));
    binding.observe_heads(&remote.heads);
    project.put_binding(&binding)?;

    let push = push(port, project, sidecar).await?;
    Ok(PublishReport {
        meta: remote.meta,
        push,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block_on::block_on;
    use crate::test_support::{TestWorld, sidecar};
    use lpc_cloud_api::CloudError;

    #[test]
    fn publishing_uploads_content_and_binds_the_project() {
        let world = TestWorld::new();
        let yona = world.user("yona");
        let dome = world.project(1);
        let local = dome.init();
        dome.write("/project.json", b"{}");
        dome.write("/shader.glsl", b"void main() {}");
        let v1 = local.save(2.0).unwrap();

        let report = block_on(publish(
            &yona,
            &local,
            Visibility::Link,
            "zook-dome",
            &sidecar("Zook Dome"),
        ))
        .unwrap();

        // identity is the local uid, not a service-minted one
        assert_eq!(report.meta.uid, dome.uid());
        assert_eq!(report.meta.slug, "zook-dome");
        assert_eq!(report.meta.visibility, Visibility::Link);
        assert!(report.push.advanced());
        assert_eq!(report.push.heads()[0].tree, v1);
        assert_eq!(
            report.link().path(),
            alloc::format!("/p/zook-dome-{}", dome.uid())
        );

        let binding = local.require_binding().unwrap();
        assert_eq!(binding.project, dome.uid());
        assert_eq!(binding.last_seen_heads, alloc::vec![v1]);
        assert_eq!(world.server().tree_count(), 1);
        assert_eq!(world.server().blob_count(), 2);
    }

    #[test]
    fn republishing_your_own_project_restates_it() {
        let world = TestWorld::new();
        let yona = world.user("yona");
        let dome = world.project(1);
        let local = dome.init();
        dome.write("/project.json", b"{}");
        local.save(2.0).unwrap();

        block_on(publish(
            &yona,
            &local,
            Visibility::Private,
            "dome",
            &sidecar("Dome"),
        ))
        .unwrap();
        let again = block_on(publish(
            &yona,
            &local,
            Visibility::Link,
            "zook-dome",
            &sidecar("Dome"),
        ))
        .unwrap();

        assert_eq!(again.meta.visibility, Visibility::Link);
        assert_eq!(again.meta.slug, "zook-dome");
        // nothing new to send the second time
        assert!(matches!(again.push, PushReport::UpToDate { .. }));
    }

    #[test]
    fn an_anonymous_client_cannot_publish() {
        let world = TestWorld::new();
        let stranger = world.anonymous();
        let dome = world.project(1);
        let local = dome.init();
        dome.write("/project.json", b"{}");
        local.save(2.0).unwrap();

        let error = block_on(publish(
            &stranger,
            &local,
            Visibility::Link,
            "dome",
            &sidecar("Dome"),
        ))
        .unwrap_err();
        assert!(matches!(
            error,
            SyncError::Cloud(CloudError::NotAuthenticated)
        ));
        assert!(local.binding().unwrap().is_none());
    }
}
