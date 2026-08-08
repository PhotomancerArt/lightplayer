//! One project's trip to the service: publish it or push it, once.
//!
//! Runtime-neutral on purpose (AGENTS.md sans-IO): it takes a
//! [`CloudPort`] and a [`LocalProject`] and returns a future with no
//! spawning and no executor-flavored sleeps, so the browser driver drives it
//! with `spawn_local` and the tests drive it with `block_on` against the
//! in-process service. Scheduling — when to come back, and whether to come
//! back at all — is [`SyncQueue`](super::sync_queue::SyncQueue)'s; this file
//! only knows how to make one attempt and how to describe the result.

use lpa_cloud_client::cloud_port::{CloudPort, TransportError};
use lpa_cloud_client::{LocalProject, PublishReport, PushReport, SyncError, publish, push};
use lpc_cloud_api::request::GetProject;
use lpc_cloud_api::{Access, CloudError, SidecarMeta};

use super::sync_queue::TripResult;

/// What publishing a project for the first time grants (D3): the client
/// decides what publishing means, and today it means the link opens a
/// read-only copy.
pub const DEFAULT_ACCESS: Access = Access::View;

/// What one trip did.
#[derive(Debug, Clone)]
pub enum TripReport {
    /// The project's cloud record was created, or restated at a new slug.
    Published(PublishReport),
    /// The project was already published; its content went up.
    Pushed(PushReport),
    /// The project has no saved version yet — there is nothing for the
    /// service to hold, and no failure either.
    Nothing,
}

/// Publish or push `project`, once.
///
/// The fork is the local binding, not a flag the caller carries: a project
/// with no [`CloudBinding`](lpa_cloud_client::CloudBinding) has never been on
/// the service and gets published, and everything else gets pushed.
/// `restate` overrides that for a rename — publishing is also how a slug is
/// changed, since the API has no separate verb for it and the URL heals
/// through the route's canonicalization.
///
/// # Publishing never downgrades access
///
/// [`DEFAULT_ACCESS`] applies to a project the service does not know. When
/// it already has one — a restate, or a project published from another
/// library on the same account — its current access is read back and stated
/// again unchanged, so a rename cannot silently un-share a project the owner
/// set to `Edit` or shut down to `None`.
pub async fn run_trip<P: CloudPort + ?Sized>(
    port: &P,
    project: &LocalProject<'_>,
    sidecar: &SidecarMeta,
    slug: &str,
    restate: bool,
) -> Result<TripReport, SyncError> {
    // Nothing to publish and nothing to push: a package whose history has an
    // origin and no save. Not an error, and not worth a round trip.
    if project.head()?.is_none() {
        return Ok(TripReport::Nothing);
    }

    let bound = project.binding()?.is_some();
    if bound && !restate {
        return Ok(TripReport::Pushed(push(port, project, sidecar).await?));
    }

    let access = current_access(port, project).await?;
    Ok(TripReport::Published(
        publish(port, project, access, slug, sidecar).await?,
    ))
}

/// How the driver should treat a failed trip.
///
/// The split is the one [`SyncError`] already draws: not reaching the service
/// is the offline case and the caller's loop owns it, while the service
/// answering "no" is an answer — repeating it changes nothing until
/// something else does. Local-state failures are in the second family too: a
/// package with an unreadable history will not heal on a timer.
pub fn classify(error: &SyncError) -> TripResult {
    match error {
        SyncError::Transport(TransportError::Offline) => TripResult::Retry,
        // A version mismatch is a tab left open across a redeploy: nothing
        // this page can send will be understood until it reloads.
        SyncError::Cloud(CloudError::VersionMismatch { .. }) => TripResult::Refused,
        // The push pre-flight uploads what the service says it is missing,
        // so this means the two disagreed mid-flight — worth one more go.
        SyncError::Cloud(CloudError::MissingBlobs { .. }) => TripResult::Retry,
        // "Not yours to write" (P6): a visitor pushing a tracking copy —
        // NotAuthorized signed in, NotAuthenticated anonymous or with a
        // session that expired mid-trip. Latched, not merely dropped: the
        // server will keep saying no until access or membership changes,
        // and only an observed change (a later pull, a sign-in) lifts it.
        SyncError::Cloud(CloudError::NotAuthorized | CloudError::NotAuthenticated) => {
            TripResult::Denied
        }
        _ => TripResult::Refused,
    }
}

/// The access the service already records for this project, or
/// [`DEFAULT_ACCESS`] when it has never heard of it.
///
/// `NotFound` covers both "never published" and "published by somebody
/// else", deliberately and identically (existence must not leak). Treating
/// it as the default is right for the first case; in the second the publish
/// that follows is refused anyway, and refused is the correct outcome.
async fn current_access<P: CloudPort + ?Sized>(
    port: &P,
    project: &LocalProject<'_>,
) -> Result<Access, SyncError> {
    let uid = project.uid();
    match lpa_cloud_client::call(port, GetProject { uid }).await {
        Ok(info) => Ok(info.meta.access),
        Err(SyncError::Cloud(CloudError::NotFound)) => Ok(DEFAULT_ACCESS),
        Err(error) => Err(error),
    }
}

/// Against the real service, in process (`lpa-cloud-client`'s `in-process`
/// transport is a dev-dependency here — the wasm bundle builds it off).
#[cfg(test)]
mod tests {
    use super::*;
    use lpa_cloud_client::{InProcessCloud, InProcessServer, block_on};
    use lpc_cloud_api::request::SetAccess;
    use lpc_history::{EventKind, HistoryEvent, PrefixedUid, UidPrefix};
    use lpfs::{LpFs, LpFsMemory, LpPath};
    use std::rc::Rc;

    /// The publish-on-create path end to end: an unbound project's first
    /// trip mints its cloud record at the slug its name produces, sends its
    /// content, and leaves a binding behind.
    #[test]
    fn an_unbound_project_publishes_and_uploads() {
        let world = World::new();
        let dome = world.project(1, b"{\"n\":1}");

        let report = world.trip(&dome, "zook-dome", false).unwrap();
        let TripReport::Published(published) = report else {
            panic!("expected a publish, got {report:?}");
        };
        assert_eq!(published.meta.uid, dome.uid);
        assert_eq!(published.meta.slug, "zook-dome");
        assert_eq!(published.meta.access, Access::View);
        assert!(published.push.advanced());
        assert!(dome.local().binding().unwrap().is_some());
    }

    /// The push-on-save path: a project the service already knows is not
    /// re-published on every save, it is pushed.
    #[test]
    fn a_bound_project_pushes() {
        let world = World::new();
        let dome = world.project(1, b"{\"n\":1}");
        world.trip(&dome, "zook-dome", false).unwrap();

        dome.write(b"{\"n\":2}");
        dome.save(3.0);
        let report = world.trip(&dome, "zook-dome", false).unwrap();
        let TripReport::Pushed(pushed) = report else {
            panic!("expected a push, got {report:?}");
        };
        assert!(pushed.advanced());
        assert_eq!(
            pushed.heads()[0].tree,
            dome.local().head().unwrap().unwrap()
        );
    }

    /// The sweep's rule, stated as the trip sees it: which verb runs is a
    /// function of the binding alone, so a sweep over a mixed library
    /// publishes exactly the projects that had never been published.
    #[test]
    fn a_sweep_publishes_only_the_unbound() {
        let world = World::new();
        let published = world.project(1, b"{\"a\":1}");
        let fresh = world.project(2, b"{\"b\":1}");
        world.trip(&published, "published", false).unwrap();

        let verbs: Vec<&'static str> = [&published, &fresh]
            .iter()
            .map(
                |project| match world.trip(project, "swept", false).unwrap() {
                    TripReport::Published(_) => "publish",
                    TripReport::Pushed(_) => "push",
                    TripReport::Nothing => "nothing",
                },
            )
            .collect();
        assert_eq!(verbs, vec!["push", "publish"]);
    }

    /// A rename re-publishes, which is how the slug changes — and the
    /// project's uid, and therefore every link already handed out, does not.
    #[test]
    fn a_restate_republishes_at_the_new_slug() {
        let world = World::new();
        let dome = world.project(1, b"{\"n\":1}");
        world.trip(&dome, "old-name", false).unwrap();

        let report = world.trip(&dome, "new-name", true).unwrap();
        let TripReport::Published(published) = report else {
            panic!("expected a publish, got {report:?}");
        };
        assert_eq!(published.meta.slug, "new-name");
        assert_eq!(published.meta.uid, dome.uid);
    }

    /// The access guard: a rename must not quietly re-share (or un-share) a
    /// project whose owner chose a level other than the client default.
    #[test]
    fn a_restate_keeps_the_access_the_owner_chose() {
        let world = World::new();
        let dome = world.project(1, b"{\"n\":1}");
        world.trip(&dome, "old-name", false).unwrap();
        block_on(lpa_cloud_client::call(
            &world.client,
            SetAccess {
                uid: dome.uid,
                access: Access::Edit,
            },
        ))
        .unwrap();

        let report = world.trip(&dome, "new-name", true).unwrap();
        let TripReport::Published(published) = report else {
            panic!("expected a publish, got {report:?}");
        };
        assert_eq!(published.meta.access, Access::Edit);
    }

    /// The sidecar the producer builds is what the service ends up holding —
    /// the name and format a listing renders from.
    #[test]
    fn the_sidecar_reaches_the_service() {
        let world = World::new();
        let dome = world.project(1, b"{\"n\":1}");
        world.trip(&dome, "zook-dome", false).unwrap();

        let info = block_on(lpa_cloud_client::call(
            &world.client,
            GetProject { uid: dome.uid },
        ))
        .unwrap();
        assert_eq!(info.sidecar, sidecar());
    }

    /// A package that was created but never saved has nothing to put in the
    /// cloud — and that is not a failure the driver should retry.
    #[test]
    fn a_project_with_no_saved_version_does_nothing() {
        let world = World::new();
        let empty = TestProject::new(3);
        empty.init();
        assert!(matches!(
            world.trip(&empty, "empty", false).unwrap(),
            TripReport::Nothing
        ));
    }

    /// Offline is one failed attempt against untouched local state, and the
    /// same trip settles once the service is reachable again.
    #[test]
    fn an_offline_trip_is_retryable_and_settles_later() {
        let world = World::new();
        let dome = world.project(1, b"{\"n\":1}");

        world.client.go_offline();
        let error = world.trip(&dome, "zook-dome", false).unwrap_err();
        assert_eq!(classify(&error), TripResult::Retry);
        assert!(
            dome.local().binding().unwrap().is_none(),
            "nothing was banked against a service we never reached"
        );

        world.client.go_online();
        assert!(matches!(
            world.trip(&dome, "zook-dome", false).unwrap(),
            TripReport::Published(_)
        ));
    }

    /// A signed-out client is denied, not retried: the driver does not even
    /// get here (it no-ops signed out), but a session that expires mid-trip
    /// must not turn into a minute-by-minute retry loop — the latch holds
    /// until the sign-in edge resets it.
    #[test]
    fn a_signed_out_refusal_is_terminal() {
        let world = World::new();
        let dome = world.project(1, b"{\"n\":1}");
        let stranger = InProcessCloud::anonymous(world.server.clone());

        let error = block_on(run_trip(
            &stranger,
            &dome.local(),
            &sidecar(),
            "zook-dome",
            false,
        ))
        .unwrap_err();
        assert!(matches!(
            error,
            SyncError::Cloud(CloudError::NotAuthenticated)
        ));
        assert_eq!(classify(&error), TripResult::Denied);
    }

    /// The P6 refusal: a signed-in visitor pushing somebody else's `View`
    /// project is denied — latched, so the queue stops asking.
    #[test]
    fn a_visitors_push_against_a_view_link_is_denied() {
        let world = World::new();
        let dome = world.project(1, b"{\"n\":1}");
        world.trip(&dome, "zook-dome", false).unwrap();
        block_on(lpa_cloud_client::call(
            &world.client,
            SetAccess {
                uid: dome.uid,
                access: Access::View,
            },
        ))
        .unwrap();

        let (visitor, _) = InProcessCloud::sign_in(
            world.server.clone(),
            "sub-oliver",
            "oliver@example.com",
            "O",
        );
        dome.write(b"{\"n\":2}");
        dome.save(4.0);
        let error =
            block_on(run_trip(&visitor, &dome.local(), &sidecar(), "dome", false)).unwrap_err();
        assert!(matches!(error, SyncError::Cloud(CloudError::NotAuthorized)));
        assert_eq!(classify(&error), TripResult::Denied);
    }

    // helpers ---------------------------------------------------------------

    struct World {
        server: Rc<InProcessServer>,
        client: InProcessCloud,
    }

    impl World {
        fn new() -> Self {
            let server = InProcessServer::new(1_000.0);
            let (client, _) =
                InProcessCloud::sign_in(server.clone(), "sub-yona", "yona@example.com", "Yona");
            Self { server, client }
        }

        /// A project slot with one saved version in it.
        fn project(&self, n: u8, contents: &[u8]) -> TestProject {
            let project = TestProject::new(n);
            project.init();
            project.write(contents);
            project.save(2.0);
            project
        }

        fn trip(
            &self,
            project: &TestProject,
            slug: &str,
            restate: bool,
        ) -> Result<TripReport, SyncError> {
            block_on(run_trip(
                &self.client,
                &project.local(),
                &sidecar(),
                slug,
                restate,
            ))
        }
    }

    /// One project's two filesystems, owned so a `LocalProject` can borrow
    /// them (the shape the OPFS host hands the driver).
    struct TestProject {
        uid: PrefixedUid,
        package: LpFsMemory,
        history: LpFsMemory,
    }

    impl TestProject {
        fn new(n: u8) -> Self {
            Self {
                uid: PrefixedUid::mint(UidPrefix::Project, &[n; 16]),
                package: LpFsMemory::new(),
                history: LpFsMemory::new(),
            }
        }

        fn local(&self) -> LocalProject<'_> {
            LocalProject::new(self.uid, &self.package, &self.history)
        }

        fn init(&self) {
            LocalProject::init(
                self.uid,
                &self.package,
                &self.history,
                HistoryEvent {
                    at: 1.0,
                    kind: EventKind::Created,
                },
            )
            .expect("init history");
        }

        fn write(&self, contents: &[u8]) {
            self.package
                .write_file(LpPath::new("/project.json"), contents)
                .expect("write");
        }

        fn save(&self, at: f64) {
            self.local().save(at).expect("save");
        }
    }

    fn sidecar() -> SidecarMeta {
        SidecarMeta {
            name: "Zook Dome".to_string(),
            format_version: 5,
            preview_png: None,
        }
    }
}
