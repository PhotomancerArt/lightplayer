//! Domain-level tests for [`lp_cloud_domain::CloudService`].
//!
//! These live in `tests/` rather than at the bottom of `cloud_service.rs`
//! for one mechanical reason: they exercise the service against the real
//! in-memory adapters, and `lp-cloud-store-mem` depends on this crate. That
//! dev-dependency cycle is fine for an integration target (it links the same
//! `lp-cloud-domain` the adapters link) but not for the lib's own `#[cfg(test)]`
//! module, which compiles a *second* copy of the crate whose traits the
//! adapters do not implement. The alternative — a hand-rolled second store
//! inside this crate — is exactly the fake-drifts-from-real hazard the
//! shared adapter is meant to prevent.
//!
//! Pure logic that needs no store is unit-tested in place: see the test
//! modules at the bottom of `push_validation.rs` and `model/project_refs.rs`.

use lp_cloud_domain::{
    Caller, CloudService, DevPickerConnection, LoginProviders, MetaStore, OidcConnection,
    session_token_hash,
};
use lp_cloud_store_mem::{MemClock, MemIdMint, MemMetaStore};
use lpc_cloud_api::request::{
    AddMember, ArchiveProject, GetEvents, GetHeads, GetProject, HaveBlobs, PublishProject,
    PushCommit, RemoveMember, RestoreProject, RevokeSession, SetAccess, UpdateMe,
};
use lpc_cloud_api::response::{
    Events, MissingBlobs, ProjectInfo, ProjectList, PushResult, UserInfo,
};
use lpc_cloud_api::{
    Access, Ack, Actor, CloudError, CloudRequest, CloudResponse, MemberRole, PushOutcome,
    SidecarMeta,
};
use lpc_history::{ContentHash, EventKind, HistoryEvent, PrefixedUid, UidPrefix};

type Service = CloudService<MemMetaStore, MemClock, MemIdMint>;

// ---- access matrix -----------------------------------------------

/// Reads: `View` and `Edit` are open to everyone holding the uid; `None`
/// is open to members only, and everyone else is told `NotFound`.
#[test]
fn read_access_matrix() {
    for access in [Access::None, Access::View, Access::Edit] {
        let mut svc = service();
        let world = World::publish(&mut svc, access);

        for actor in [world.owner, world.member] {
            assert!(
                svc.handle(actor, get_project(world.project)).is_ok(),
                "members read {access:?} projects"
            );
        }

        for actor in [Actor::Anonymous, world.stranger] {
            let answer = svc.handle(actor, get_project(world.project));
            match access {
                Access::View | Access::Edit => {
                    assert!(answer.is_ok(), "a {access:?} link opens the project")
                }
                Access::None => assert_eq!(
                    answer,
                    Err(CloudError::NotFound),
                    "a project no link reaches must not confirm its own existence"
                ),
            }
        }
    }
}

/// Writes: members always, plus anybody holding an `Edit` link — anonymous
/// included (D-Q6). Otherwise anonymous is `NotAuthenticated`; a non-member
/// gets `NotAuthorized` where existence is already public and `NotFound`
/// where it is not.
#[test]
fn write_access_matrix() {
    for access in [Access::None, Access::View, Access::Edit] {
        let mut svc = service();
        let world = World::publish(&mut svc, access);
        // Restates the access it already has: the point is who may write,
        // and the matrix must not move the project out from under the row
        // being tested.
        let request = || {
            CloudRequest::SetAccess(SetAccess {
                uid: world.project,
                access,
            })
        };

        for actor in [world.owner, world.member] {
            assert!(svc.handle(actor, request()).is_ok(), "members write");
        }

        let expected_anonymous = match access {
            Access::Edit => None,
            _ => Some(CloudError::NotAuthenticated),
        };
        assert_eq!(
            svc.handle(Actor::Anonymous, request()).err(),
            expected_anonymous,
            "anonymous on a {access:?} project"
        );

        let expected_stranger = match access {
            Access::Edit => None,
            Access::View => Some(CloudError::NotAuthorized),
            Access::None => Some(CloudError::NotFound),
        };
        assert_eq!(
            svc.handle(world.stranger, request()).err(),
            expected_stranger,
            "a signed-in non-member on a {access:?} project"
        );
    }
}

/// The one that makes the `Edit` link a real capability: no account, no
/// membership, and the push lands.
#[test]
fn anonymous_pushes_to_an_edit_project() {
    let mut svc = service();
    let world = World::publish(&mut svc, Access::Edit);

    let pushed = push(
        &mut svc,
        Actor::Anonymous,
        world.project,
        &[],
        v(1),
        origin_batch(1),
    );
    assert_eq!(outcome_of(&pushed), PushOutcome::Advanced);
    assert_eq!(heads_of(&pushed), vec![v(1)]);
}

/// `HaveBlobs` is the pre-flight of a push, so it has to be reachable by
/// everybody a push is: it names no project, and the hash is the secret.
#[test]
fn have_blobs_is_anonymous_callable() {
    let mut svc = service();
    svc.store_mut().record_blob(v(1), 10);
    let CloudResponse::MissingBlobs(MissingBlobs { hashes }) = svc
        .handle(
            Actor::Anonymous,
            CloudRequest::HaveBlobs(HaveBlobs {
                hashes: vec![v(1), v(2)],
            }),
        )
        .unwrap()
    else {
        panic!("expected MissingBlobs");
    };
    assert_eq!(hashes, vec![v(2)]);
}

// ---- members exposure --------------------------------------------

/// The member list is a list of people's email addresses: the people on it
/// get it, and a link-holder does not — not even one holding an `Edit` link
/// they may push with.
#[test]
fn the_member_list_goes_only_to_members() {
    let mut svc = service();
    let world = World::publish(&mut svc, Access::Edit);

    let editors =
        members_seen_by(&mut svc, world.member, world.project).expect("an editor sees it");
    let emails: Vec<&str> = editors.iter().map(|m| m.email.as_str()).collect();
    assert_eq!(emails, vec!["member@example.com", "owner@example.com"]);
    let owner_row = editors
        .iter()
        .find(|m| m.email == "owner@example.com")
        .unwrap();
    assert_eq!(owner_row.role, MemberRole::Owner);
    assert!(!owner_row.pending);
    assert_eq!(
        editors
            .iter()
            .find(|m| m.email == "member@example.com")
            .unwrap()
            .role,
        MemberRole::Editor
    );

    assert_eq!(
        members_seen_by(&mut svc, Actor::Anonymous, world.project),
        None,
        "an anonymous link-holder never learns who else has access"
    );
    assert_eq!(
        members_seen_by(&mut svc, world.stranger, world.project),
        None,
        "nor does a signed-in caller who is only holding the link — even \
         though this Edit link lets them push"
    );
}

/// A pending invitation shows up on the list as pending, which is what lets
/// the share UI say "invited" rather than pretending they are in.
#[test]
fn a_pending_invitation_is_listed_as_pending() {
    let mut svc = service();
    let world = World::publish(&mut svc, Access::None);
    svc.handle(
        world.owner,
        CloudRequest::AddMember(AddMember {
            uid: world.project,
            email: "later@example.com".into(),
        }),
    )
    .unwrap();

    let members = members_seen_by(&mut svc, world.owner, world.project).expect("the owner sees it");
    let pending = members
        .iter()
        .find(|m| m.email == "later@example.com")
        .expect("the invitation is listed");
    assert!(pending.pending);
    assert_eq!(pending.user, None);
    assert_eq!(pending.role, MemberRole::Editor);
}

// ---- archive / restore -------------------------------------------

/// Archiving stops the link resolving for everyone but the project's own
/// members — and refuses writes even for them. Restoring puts it all back.
#[test]
fn archiving_closes_the_link_and_freezes_writes() {
    let mut svc = service();
    let world = World::publish(&mut svc, Access::View);
    assert!(
        svc.handle(Actor::Anonymous, get_project(world.project))
            .is_ok()
    );

    let archived = svc
        .handle(world.owner, archive(world.project))
        .expect("the owner archives");
    let CloudResponse::ProjectInfo(ProjectInfo { meta, .. }) = archived else {
        panic!("expected ProjectInfo");
    };
    assert!(meta.archived);
    assert_eq!(meta.access, Access::View, "the access level is remembered");

    for actor in [Actor::Anonymous, world.stranger] {
        assert_eq!(
            svc.handle(actor, get_project(world.project)),
            Err(CloudError::NotFound),
            "an archived project stops resolving for visitors"
        );
    }
    for actor in [world.owner, world.member] {
        assert!(
            svc.handle(actor, get_project(world.project)).is_ok(),
            "its members can still read it"
        );
    }

    // Every write, including a member's, and including the owner's own.
    let writes = [
        CloudRequest::SetAccess(SetAccess {
            uid: world.project,
            access: Access::Edit,
        }),
        CloudRequest::AddMember(AddMember {
            uid: world.project,
            email: "later@example.com".into(),
        }),
    ];
    for actor in [world.owner, world.member] {
        for request in writes.clone() {
            assert!(
                matches!(
                    svc.handle(actor, request),
                    Err(CloudError::InvalidRequest { .. })
                ),
                "an archived project refuses its members' writes too"
            );
        }
    }
    assert!(matches!(
        try_push(
            &mut svc,
            world.owner,
            world.project,
            &[],
            v(1),
            origin_batch(1)
        ),
        Err(CloudError::InvalidRequest { .. })
    ));

    let restored = svc
        .handle(world.owner, restore(world.project))
        .expect("the owner restores");
    let CloudResponse::ProjectInfo(ProjectInfo { meta, .. }) = restored else {
        panic!("expected ProjectInfo");
    };
    assert!(!meta.archived);
    assert!(
        svc.handle(Actor::Anonymous, get_project(world.project))
            .is_ok(),
        "the link resolves again"
    );
    let pushed = push(
        &mut svc,
        world.owner,
        world.project,
        &[],
        v(1),
        origin_batch(1),
    );
    assert_eq!(outcome_of(&pushed), PushOutcome::Advanced);
}

/// Archive and restore are the owner's alone. An editor can see the project,
/// so they are told `NotAuthorized`; nobody else learns anything.
#[test]
fn only_the_owner_archives_or_restores() {
    let mut svc = service();
    let world = World::publish(&mut svc, Access::View);

    for request in [archive(world.project), restore(world.project)] {
        assert_eq!(
            svc.handle(world.member, request.clone()),
            Err(CloudError::NotAuthorized),
            "an editor is not the owner"
        );
        assert_eq!(
            svc.handle(world.stranger, request.clone()),
            Err(CloudError::NotAuthorized),
            "a link-holder can see it exists, so the refusal may say so"
        );
        assert_eq!(
            svc.handle(Actor::Anonymous, request),
            Err(CloudError::NotAuthenticated)
        );
    }
}

/// An unpublished (or unreachable) project's archive verbs must not become
/// an existence oracle.
#[test]
fn archiving_an_unreachable_project_answers_not_found() {
    let mut svc = service();
    let world = World::publish(&mut svc, Access::None);
    assert_eq!(
        svc.handle(world.stranger, archive(world.project)),
        Err(CloudError::NotFound)
    );
    assert_eq!(
        svc.handle(
            world.owner,
            archive(PrefixedUid::mint(UidPrefix::Project, &[8u8; 16]))
        ),
        Err(CloudError::NotFound)
    );
}

/// Re-publishing restates slug and access, and deliberately does *not*
/// bring an archived project back — that is `RestoreProject`'s job.
#[test]
fn publishing_does_not_un_archive() {
    let mut svc = service();
    let world = World::publish(&mut svc, Access::View);
    svc.handle(world.owner, archive(world.project)).unwrap();

    let answer = svc
        .handle(
            world.owner,
            CloudRequest::PublishProject(PublishProject {
                uid: world.project,
                access: Access::Edit,
                slug: "renamed".into(),
            }),
        )
        .unwrap();
    let CloudResponse::ProjectInfo(ProjectInfo { meta, .. }) = answer else {
        panic!("expected ProjectInfo");
    };
    assert_eq!(meta.slug, "renamed");
    assert_eq!(meta.access, Access::Edit);
    assert!(meta.archived, "still archived");
    assert_eq!(
        svc.handle(Actor::Anonymous, get_project(world.project)),
        Err(CloudError::NotFound)
    );
}

/// The read rule applies to every read verb, not just `GetProject`.
#[test]
fn heads_and_events_follow_the_read_rule() {
    let mut svc = service();
    let world = World::publish(&mut svc, Access::None);
    for request in [
        CloudRequest::GetHeads(GetHeads { uid: world.project }),
        CloudRequest::GetEvents(GetEvents {
            uid: world.project,
            since: 0,
        }),
    ] {
        assert_eq!(
            svc.handle(Actor::Anonymous, request.clone()),
            Err(CloudError::NotFound)
        );
        assert!(svc.handle(world.member, request).is_ok());
    }
}

// ---- push --------------------------------------------------------

/// A push whose parent is the sole head replaces it.
#[test]
fn push_fast_forwards_the_line() {
    let mut svc = service();
    let world = World::publish(&mut svc, Access::None);

    let first = push(
        &mut svc,
        world.owner,
        world.project,
        &[],
        v(1),
        origin_batch(1),
    );
    assert_eq!(outcome_of(&first), PushOutcome::Advanced);
    assert_eq!(heads_of(&first), vec![v(1)]);

    let second = push(
        &mut svc,
        world.owner,
        world.project,
        &[v(1)],
        v(2),
        vec![saved(2, 3.0)],
    );
    assert_eq!(outcome_of(&second), PushOutcome::Advanced);
    assert_eq!(heads_of(&second), vec![v(2)]);
}

/// Two clients pushing from the same base both succeed: the second
/// becomes a sibling head. Nothing is refused and nothing is lost.
#[test]
fn divergent_push_adds_a_second_head() {
    let mut svc = service();
    let world = World::publish(&mut svc, Access::None);
    push(
        &mut svc,
        world.owner,
        world.project,
        &[],
        v(1),
        origin_batch(1),
    );

    push(
        &mut svc,
        world.owner,
        world.project,
        &[v(1)],
        v(2),
        vec![saved(2, 3.0)],
    );
    let diverged = push(
        &mut svc,
        world.member,
        world.project,
        &[v(1)],
        v(3),
        vec![saved(3, 3.5)],
    );

    assert_eq!(outcome_of(&diverged), PushOutcome::NewHead);
    let mut heads = heads_of(&diverged);
    heads.sort();
    let mut expected = vec![v(2), v(3)];
    expected.sort();
    assert_eq!(heads, expected);
}

/// The join a client pushes to resolve that divergence names both heads
/// as parents, and the frontier collapses back to one.
#[test]
fn join_push_collapses_the_frontier() {
    let mut svc = service();
    let world = World::publish(&mut svc, Access::None);
    push(
        &mut svc,
        world.owner,
        world.project,
        &[],
        v(1),
        origin_batch(1),
    );
    push(
        &mut svc,
        world.owner,
        world.project,
        &[v(1)],
        v(2),
        vec![saved(2, 3.0)],
    );
    push(
        &mut svc,
        world.member,
        world.project,
        &[v(1)],
        v(3),
        vec![saved(3, 3.5)],
    );

    let join = HistoryEvent {
        at: 4.0,
        kind: EventKind::Joined {
            kept: v(2),
            set_aside: v(3),
        },
    };
    let collapsed = push(
        &mut svc,
        world.owner,
        world.project,
        &[v(2), v(3)],
        v(2),
        vec![join],
    );

    assert_eq!(outcome_of(&collapsed), PushOutcome::Advanced);
    assert_eq!(heads_of(&collapsed), vec![v(2)]);
}

#[test]
fn push_refuses_hashes_the_blob_index_lacks() {
    let mut svc = service();
    let world = World::publish(&mut svc, Access::None);
    // Deliberately does NOT record the blob first.
    let answer = svc.handle(
        world.owner,
        CloudRequest::PushCommit(PushCommit {
            uid: world.project,
            parents: vec![],
            tree: v(1),
            events: origin_batch(1),
            sidecar: sidecar(),
        }),
    );
    assert_eq!(answer, Err(CloudError::MissingBlobs { hashes: vec![v(1)] }));
}

#[test]
fn push_refuses_a_batch_without_an_origin_on_an_empty_log() {
    let mut svc = service();
    let world = World::publish(&mut svc, Access::None);
    let answer = try_push(
        &mut svc,
        world.owner,
        world.project,
        &[],
        v(1),
        vec![saved(1, 2.0)],
    );
    assert!(matches!(answer, Err(CloudError::InvalidRequest { .. })));
}

#[test]
fn push_follows_the_write_rule() {
    let mut svc = service();
    let world = World::publish(&mut svc, Access::View);
    for (actor, expected) in [
        (Actor::Anonymous, CloudError::NotAuthenticated),
        (world.stranger, CloudError::NotAuthorized),
    ] {
        let answer = try_push(&mut svc, actor, world.project, &[], v(1), origin_batch(1));
        assert_eq!(answer, Err(expected));
    }
}

/// A push updates the sidecar the service reports, verbatim.
#[test]
fn push_replaces_the_sidecar_verbatim() {
    let mut svc = service();
    let world = World::publish(&mut svc, Access::None);
    push(
        &mut svc,
        world.owner,
        world.project,
        &[],
        v(1),
        origin_batch(1),
    );
    let CloudResponse::ProjectInfo(ProjectInfo { sidecar, .. }) =
        svc.handle(world.owner, get_project(world.project)).unwrap()
    else {
        panic!("expected ProjectInfo");
    };
    assert_eq!(sidecar, self::sidecar());
}

// ---- event log ---------------------------------------------------

#[test]
fn get_events_reads_forward_without_gap_or_overlap() {
    let mut svc = service();
    let world = World::publish(&mut svc, Access::None);
    push(
        &mut svc,
        world.owner,
        world.project,
        &[],
        v(1),
        origin_batch(1),
    );

    let CloudResponse::Events(Events { events, next_since }) = svc
        .handle(
            world.owner,
            CloudRequest::GetEvents(GetEvents {
                uid: world.project,
                since: 0,
            }),
        )
        .unwrap()
    else {
        panic!("expected Events");
    };
    assert_eq!(events.len(), 2);
    assert_eq!(next_since, 2);

    push(
        &mut svc,
        world.owner,
        world.project,
        &[v(1)],
        v(2),
        vec![saved(2, 3.0)],
    );
    let CloudResponse::Events(Events { events, next_since }) = svc
        .handle(
            world.owner,
            CloudRequest::GetEvents(GetEvents {
                uid: world.project,
                since: next_since,
            }),
        )
        .unwrap()
    else {
        panic!("expected Events");
    };
    assert_eq!(events, vec![saved(2, 3.0)]);
    assert_eq!(next_since, 3);

    // Nothing new: the cursor stands still rather than rewinding.
    let CloudResponse::Events(Events { events, next_since }) = svc
        .handle(
            world.owner,
            CloudRequest::GetEvents(GetEvents {
                uid: world.project,
                since: next_since,
            }),
        )
        .unwrap()
    else {
        panic!("expected Events");
    };
    assert!(events.is_empty());
    assert_eq!(next_since, 3);
}

// ---- publish / membership ----------------------------------------

#[test]
fn publishing_someone_elses_uid_answers_not_found() {
    let mut svc = service();
    let world = World::publish(&mut svc, Access::None);
    let answer = svc.handle(
        world.stranger,
        CloudRequest::PublishProject(PublishProject {
            uid: world.project,
            access: Access::View,
            slug: "stolen".into(),
        }),
    );
    assert_eq!(answer, Err(CloudError::NotFound));
}

#[test]
fn republishing_restates_slug_and_access() {
    let mut svc = service();
    let world = World::publish(&mut svc, Access::None);
    let answer = svc
        .handle(
            world.owner,
            CloudRequest::PublishProject(PublishProject {
                uid: world.project,
                access: Access::View,
                slug: "renamed".into(),
            }),
        )
        .unwrap();
    let CloudResponse::ProjectInfo(ProjectInfo { meta, .. }) = answer else {
        panic!("expected ProjectInfo");
    };
    assert_eq!(meta.slug, "renamed");
    assert_eq!(meta.access, Access::View);
}

#[test]
fn publish_validates_uid_prefix_and_slug() {
    let mut svc = service();
    let owner = svc.upsert_user(
        "g-owner",
        "owner@example.com",
        "Owner",
        "google",
        None,
        None,
        None,
    );
    let actor = Actor::User(owner.uid);

    let wrong_prefix = svc.handle(
        actor,
        CloudRequest::PublishProject(PublishProject {
            uid: PrefixedUid::mint(UidPrefix::Device, &[7u8; 16]),
            access: Access::View,
            slug: "fine".into(),
        }),
    );
    assert!(matches!(
        wrong_prefix,
        Err(CloudError::InvalidRequest { .. })
    ));

    let bad_slug = svc.handle(
        actor,
        CloudRequest::PublishProject(PublishProject {
            uid: project_uid(),
            access: Access::View,
            slug: "not/a/slug".into(),
        }),
    );
    assert!(matches!(bad_slug, Err(CloudError::InvalidRequest { .. })));

    // Tightened per P1 (2026-08-07): the slug alphabet is exactly what
    // `share_link::slugify` ever produces — `[a-z0-9-]`, no leading or
    // trailing `-`. Underscores and uppercase used to be accepted; they no
    // longer are, since publishing never sends either.
    for slug in ["under_score", "Uppercase", "-leading", "trailing-"] {
        let result = svc.handle(
            actor,
            CloudRequest::PublishProject(PublishProject {
                uid: project_uid(),
                access: Access::View,
                slug: slug.into(),
            }),
        );
        assert!(
            matches!(result, Err(CloudError::InvalidRequest { .. })),
            "{slug:?} should be rejected"
        );
    }

    // The empty slug is the bare-uid share path, not an invalid one.
    let bare = svc.handle(
        actor,
        CloudRequest::PublishProject(PublishProject {
            uid: project_uid(),
            access: Access::View,
            slug: "".into(),
        }),
    );
    assert!(bare.is_ok(), "empty slug should be accepted: {bare:?}");
}

#[test]
fn anonymous_cannot_publish() {
    let mut svc = service();
    let answer = svc.handle(
        Actor::Anonymous,
        CloudRequest::PublishProject(PublishProject {
            uid: project_uid(),
            access: Access::View,
            slug: "mine".into(),
        }),
    );
    assert_eq!(answer, Err(CloudError::NotAuthenticated));
}

/// An invitation to an address nobody has logged in with yet grants
/// nothing — until that address logs in (Q4).
#[test]
fn membership_invited_by_email_resolves_at_first_login() {
    let mut svc = service();
    let world = World::publish(&mut svc, Access::None);

    svc.handle(
        world.owner,
        CloudRequest::AddMember(AddMember {
            uid: world.project,
            email: "Later@Example.com".into(),
        }),
    )
    .unwrap();

    // Nobody holds that address yet, so nobody gained access.
    assert_eq!(
        svc.handle(world.stranger, get_project(world.project)),
        Err(CloudError::NotFound)
    );

    let later = svc.upsert_user(
        "g-later",
        "later@example.com",
        "Later",
        "google",
        None,
        None,
        None,
    );
    assert!(
        svc.handle(Actor::User(later.uid), get_project(world.project))
            .is_ok(),
        "first login resolves the pending row"
    );
}

#[test]
fn removing_a_member_revokes_their_access() {
    let mut svc = service();
    let world = World::publish(&mut svc, Access::None);
    svc.handle(
        world.owner,
        CloudRequest::RemoveMember(RemoveMember {
            uid: world.project,
            email: "member@example.com".into(),
        }),
    )
    .unwrap();
    assert_eq!(
        svc.handle(world.member, get_project(world.project)),
        Err(CloudError::NotFound)
    );
}

#[test]
fn the_owner_cannot_be_removed() {
    let mut svc = service();
    let world = World::publish(&mut svc, Access::None);
    let answer = svc.handle(
        world.member,
        CloudRequest::RemoveMember(RemoveMember {
            uid: world.project,
            email: "owner@example.com".into(),
        }),
    );
    assert!(matches!(answer, Err(CloudError::InvalidRequest { .. })));
    assert!(svc.handle(world.owner, get_project(world.project)).is_ok());
}

#[test]
fn add_member_validates_the_email() {
    let mut svc = service();
    let world = World::publish(&mut svc, Access::None);
    let answer = svc.handle(
        world.owner,
        CloudRequest::AddMember(AddMember {
            uid: world.project,
            email: "not-an-email".into(),
        }),
    );
    assert!(matches!(answer, Err(CloudError::InvalidRequest { .. })));
}

// ---- identity, listing, blobs ------------------------------------

#[test]
fn who_am_i_reports_the_caller_without_failing() {
    let mut svc = service();
    assert_eq!(
        svc.handle(Actor::Anonymous, CloudRequest::WhoAmI),
        Ok(CloudResponse::UserInfo(UserInfo {
            actor: Actor::Anonymous
        }))
    );
    let user = svc.upsert_user(
        "g-user",
        "user@example.com",
        "User",
        "google",
        None,
        None,
        None,
    );
    assert_eq!(
        svc.handle(Actor::User(user.uid), CloudRequest::WhoAmI),
        Ok(CloudResponse::UserInfo(UserInfo {
            actor: Actor::User(user.uid)
        }))
    );
}

#[test]
fn minted_user_uids_are_usr_prefixed() {
    let mut svc = service();
    let user = svc.upsert_user(
        "g-user",
        "user@example.com",
        "User",
        "google",
        None,
        None,
        None,
    );
    assert_eq!(user.uid.prefix(), UidPrefix::User);
    // Same Google subject, second login: same account.
    let again = svc.upsert_user(
        "g-user",
        "user@example.com",
        "User",
        "google",
        None,
        None,
        None,
    );
    assert_eq!(again.uid, user.uid);
}

#[test]
fn list_my_projects_covers_owner_and_member_and_nobody_else() {
    let mut svc = service();
    let world = World::publish(&mut svc, Access::None);
    for actor in [world.owner, world.member] {
        let CloudResponse::ProjectList(ProjectList { projects }) =
            svc.handle(actor, CloudRequest::ListMyProjects).unwrap()
        else {
            panic!("expected ProjectList");
        };
        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].uid, world.project);
    }
    for actor in [Actor::Anonymous, world.stranger] {
        let CloudResponse::ProjectList(ProjectList { projects }) =
            svc.handle(actor, CloudRequest::ListMyProjects).unwrap()
        else {
            panic!("expected ProjectList");
        };
        assert!(projects.is_empty());
    }
}

#[test]
fn have_blobs_reports_only_what_is_missing() {
    let mut svc = service();
    let world = World::publish(&mut svc, Access::None);
    svc.store_mut().record_blob(v(1), 10);
    let CloudResponse::MissingBlobs(MissingBlobs { hashes }) = svc
        .handle(
            world.owner,
            CloudRequest::HaveBlobs(HaveBlobs {
                hashes: vec![v(1), v(2), v(2)],
            }),
        )
        .unwrap()
    else {
        panic!("expected MissingBlobs");
    };
    assert_eq!(hashes, vec![v(2)], "and the duplicate is asked about once");
}

#[test]
fn sessions_resolve_until_they_expire() {
    let mut svc = service();
    let user = svc.upsert_user(
        "g-user",
        "user@example.com",
        "User",
        "google",
        None,
        None,
        None,
    );
    let token = svc.open_session(user.uid, 60.0, None);
    assert_eq!(svc.resolve_session(&token), Actor::User(user.uid));

    svc.clock().advance(61.0);
    assert_eq!(svc.resolve_session(&token), Actor::Anonymous);

    svc.clock().advance(-61.0);
    assert_eq!(svc.resolve_session(&token), Actor::User(user.uid));
    svc.close_session(&token);
    assert_eq!(svc.resolve_session(&token), Actor::Anonymous);
}

#[test]
fn an_unknown_token_is_simply_anonymous() {
    let svc = service();
    assert_eq!(svc.resolve_session(b"nonsense"), Actor::Anonymous);
}

// ---- account / sessions (P2) ---------------------------------------

#[test]
fn get_me_refuses_an_anonymous_caller() {
    let mut svc = service();
    assert_eq!(
        svc.handle(Actor::Anonymous, CloudRequest::GetMe),
        Err(CloudError::NotAuthenticated)
    );
}

/// `provider_label` comes from the account's `provider`, not a guess: a
/// dev-picker account (`google_sub` namespaced `dev-auth:…`, per
/// `dev_auth.rs`) is labeled "Dev" precisely because it was created with
/// `provider: "dev"`, not because of anything in its `google_sub` text.
#[test]
fn get_me_reports_the_providers_label() {
    let mut svc = service();
    let google_user = svc.upsert_user("g-1", "one@example.com", "One", "google", None, None, None);
    let dev_user = svc.upsert_user(
        "dev-auth:two@example.com",
        "two@example.com",
        "Two",
        "dev",
        None,
        None,
        None,
    );

    let CloudResponse::MeInfo(info) = svc
        .handle(Actor::User(google_user.uid), CloudRequest::GetMe)
        .unwrap()
    else {
        panic!("expected MeInfo");
    };
    assert_eq!(info.provider_label, "Google");
    assert_eq!(info.email, "one@example.com");

    let CloudResponse::MeInfo(info) = svc
        .handle(Actor::User(dev_user.uid), CloudRequest::GetMe)
        .unwrap()
    else {
        panic!("expected MeInfo");
    };
    assert_eq!(info.provider_label, "Dev");
}

#[test]
fn update_me_trims_and_recomputes_display_name() {
    let mut svc = service();
    let user = svc.upsert_user(
        "g-1",
        "one@example.com",
        "Provider Name",
        "google",
        None,
        None,
        None,
    );
    let actor = Actor::User(user.uid);

    let CloudResponse::MeInfo(info) = svc
        .handle(
            actor,
            CloudRequest::UpdateMe(UpdateMe {
                given_name: Some("  Yona  ".to_string()),
                family_name: Some("  Appletree  ".to_string()),
            }),
        )
        .unwrap()
    else {
        panic!("expected MeInfo");
    };
    assert_eq!(info.given_name.as_deref(), Some("Yona"));
    assert_eq!(info.family_name.as_deref(), Some("Appletree"));
    assert_eq!(info.display_name, "Yona Appletree");

    // The mononym case: a family name that is empty after trimming clears
    // the field, and display_name follows.
    let CloudResponse::MeInfo(info) = svc
        .handle(
            actor,
            CloudRequest::UpdateMe(UpdateMe {
                given_name: Some("Yona".to_string()),
                family_name: Some("   ".to_string()),
            }),
        )
        .unwrap()
    else {
        panic!("expected MeInfo");
    };
    assert_eq!(info.family_name, None);
    assert_eq!(info.display_name, "Yona");
}

#[test]
fn update_me_refuses_a_name_over_the_length_cap() {
    let mut svc = service();
    let user = svc.upsert_user("g-1", "one@example.com", "One", "google", None, None, None);
    let too_long = "x".repeat(201);
    let answer = svc.handle(
        Actor::User(user.uid),
        CloudRequest::UpdateMe(UpdateMe {
            given_name: Some(too_long),
            family_name: None,
        }),
    );
    assert!(matches!(answer, Err(CloudError::InvalidRequest { .. })));
}

/// The caller cannot report its own session id (it lives in an HttpOnly
/// cookie it never reads), so `ListSessions` has to mark `current` itself —
/// and only ever among the caller's own rows.
#[test]
fn list_sessions_marks_the_caller_and_isolates_by_account() {
    let mut svc = service();
    let alice = svc.upsert_user(
        "g-alice",
        "alice@example.com",
        "Alice",
        "google",
        None,
        None,
        None,
    );
    let bob = svc.upsert_user(
        "g-bob",
        "bob@example.com",
        "Bob",
        "google",
        None,
        None,
        None,
    );
    let alice_token = svc.open_session(alice.uid, 60.0, Some("Mozilla/5.0".to_string()));
    let _second_alice_token = svc.open_session(alice.uid, 60.0, None);
    let _bob_token = svc.open_session(bob.uid, 60.0, None);

    let caller = Caller {
        actor: Actor::User(alice.uid),
        session: Some(session_token_hash(&alice_token)),
    };
    let CloudResponse::SessionList(list) = svc.handle(caller, CloudRequest::ListSessions).unwrap()
    else {
        panic!("expected SessionList");
    };
    assert_eq!(list.sessions.len(), 2, "only alice's own sessions");
    let current: Vec<_> = list.sessions.iter().filter(|s| s.current).collect();
    assert_eq!(current.len(), 1, "exactly the calling session");
    assert_eq!(current[0].user_agent.as_deref(), Some("Mozilla/5.0"));
}

#[test]
fn revoke_session_refuses_bad_hex_and_someone_elses_session() {
    let mut svc = service();
    let alice = svc.upsert_user(
        "g-alice",
        "alice@example.com",
        "Alice",
        "google",
        None,
        None,
        None,
    );
    let bob = svc.upsert_user(
        "g-bob",
        "bob@example.com",
        "Bob",
        "google",
        None,
        None,
        None,
    );
    let bob_token = svc.open_session(bob.uid, 60.0, None);

    let bad_hex = svc.handle(
        Actor::User(alice.uid),
        CloudRequest::RevokeSession(RevokeSession {
            id: "not-hex".to_string(),
        }),
    );
    assert!(matches!(bad_hex, Err(CloudError::InvalidRequest { .. })));

    let bob_session_id = session_token_hash(&bob_token).to_string();
    let foreign = svc.handle(
        Actor::User(alice.uid),
        CloudRequest::RevokeSession(RevokeSession { id: bob_session_id }),
    );
    assert_eq!(foreign, Err(CloudError::NotFound));
    // Bob's session is untouched by alice's refused attempt.
    assert_eq!(svc.resolve_session(&bob_token), Actor::User(bob.uid));

    let own_token = svc.open_session(alice.uid, 60.0, None);
    let own_id = session_token_hash(&own_token).to_string();
    let revoked = svc.handle(
        Actor::User(alice.uid),
        CloudRequest::RevokeSession(RevokeSession { id: own_id }),
    );
    assert_eq!(revoked, Ok(CloudResponse::Ack(Ack)));
    assert_eq!(svc.resolve_session(&own_token), Actor::Anonymous);
}

#[test]
fn login_options_reports_the_dev_picker_only_when_enabled() {
    let mut plain = service();
    let CloudResponse::LoginOptionsInfo(info) = plain
        .handle(Actor::Anonymous, CloudRequest::LoginOptions)
        .unwrap()
    else {
        panic!("expected LoginOptionsInfo");
    };
    assert!(info.oidc.is_empty());
    assert!(info.dev_picker.is_none());

    let mut with_picker = CloudService::new(
        MemMetaStore::new(),
        MemClock::new(1_700_000_000.0),
        MemIdMint::new(),
    )
    .with_login_providers(LoginProviders {
        oidc: vec![OidcConnection {
            id: "google".to_string(),
            label: "Google".to_string(),
            start_path: "/auth/google".to_string(),
        }],
        dev_picker: Some(DevPickerConnection {
            start_path: "/auth/dev".to_string(),
        }),
    });
    with_picker.upsert_user("g-1", "one@example.com", "One", "google", None, None, None);
    with_picker.upsert_user(
        "dev-auth:two@example.com",
        "two@example.com",
        "Two",
        "dev",
        None,
        None,
        None,
    );

    let CloudResponse::LoginOptionsInfo(info) = with_picker
        .handle(Actor::Anonymous, CloudRequest::LoginOptions)
        .unwrap()
    else {
        panic!("expected LoginOptionsInfo");
    };
    assert_eq!(info.oidc.len(), 1);
    assert_eq!(info.oidc[0].id, "google");
    let choices: Vec<_> = info
        .dev_picker
        .expect("dev picker is on")
        .choices
        .into_iter()
        .map(|choice| choice.email)
        .collect();
    assert_eq!(choices, vec!["one@example.com", "two@example.com"]);
}

// ---- helpers ------------------------------------------------------

/// The cast for the access tests: one project, its owner, one member,
/// and one authenticated outsider.
struct World {
    project: PrefixedUid,
    owner: Actor,
    member: Actor,
    stranger: Actor,
}

impl World {
    fn publish(svc: &mut Service, access: Access) -> Self {
        let owner = svc.upsert_user(
            "g-owner",
            "owner@example.com",
            "Owner",
            "google",
            None,
            None,
            None,
        );
        let member = svc.upsert_user(
            "g-member",
            "member@example.com",
            "Member",
            "google",
            None,
            None,
            None,
        );
        let stranger = svc.upsert_user(
            "g-stranger",
            "stranger@example.com",
            "Stranger",
            "google",
            None,
            None,
            None,
        );
        let project = project_uid();

        svc.handle(
            Actor::User(owner.uid),
            CloudRequest::PublishProject(PublishProject {
                uid: project,
                access,
                slug: "zook-dome".into(),
            }),
        )
        .expect("publish");
        svc.handle(
            Actor::User(owner.uid),
            CloudRequest::AddMember(AddMember {
                uid: project,
                email: "member@example.com".into(),
            }),
        )
        .expect("add member");

        Self {
            project,
            owner: Actor::User(owner.uid),
            member: Actor::User(member.uid),
            stranger: Actor::User(stranger.uid),
        }
    }
}

fn service() -> Service {
    CloudService::new(
        MemMetaStore::new(),
        MemClock::new(1_700_000_000.0),
        MemIdMint::new(),
    )
}

fn project_uid() -> PrefixedUid {
    PrefixedUid::mint(UidPrefix::Project, &[1u8; 16])
}

/// Version `n`'s content hash.
fn v(n: u8) -> ContentHash {
    ContentHash::of(&[n])
}

fn saved(n: u8, at: f64) -> HistoryEvent {
    HistoryEvent {
        at,
        kind: EventKind::Saved { version: v(n) },
    }
}

/// The batch a first push carries: the origin event plus the first save.
fn origin_batch(n: u8) -> Vec<HistoryEvent> {
    vec![
        HistoryEvent {
            at: 1.0,
            kind: EventKind::Created,
        },
        saved(n, 2.0),
    ]
}

fn sidecar() -> SidecarMeta {
    SidecarMeta {
        name: "Zook Dome".into(),
        format_version: 4,
        preview_png: None,
    }
}

fn get_project(uid: PrefixedUid) -> CloudRequest {
    CloudRequest::GetProject(GetProject { uid })
}

fn archive(uid: PrefixedUid) -> CloudRequest {
    CloudRequest::ArchiveProject(ArchiveProject { uid })
}

fn restore(uid: PrefixedUid) -> CloudRequest {
    CloudRequest::RestoreProject(RestoreProject { uid })
}

/// The member list `actor` is answered with when they read the project —
/// `None` being a real answer ("you may not know"), not a failure.
fn members_seen_by(
    svc: &mut Service,
    actor: Actor,
    uid: PrefixedUid,
) -> Option<Vec<lpc_cloud_api::MemberInfo>> {
    let CloudResponse::ProjectInfo(ProjectInfo { members, .. }) = svc
        .handle(actor, get_project(uid))
        .expect("the caller can read the project")
    else {
        panic!("expected ProjectInfo");
    };
    members
}

/// Push, having first told the blob index the tree is stored (the edge's
/// job after an upload).
fn push(
    svc: &mut Service,
    actor: Actor,
    project: PrefixedUid,
    parents: &[ContentHash],
    tree: ContentHash,
    events: Vec<HistoryEvent>,
) -> CloudResponse {
    try_push(svc, actor, project, parents, tree, events).expect("push")
}

fn try_push(
    svc: &mut Service,
    actor: Actor,
    project: PrefixedUid,
    parents: &[ContentHash],
    tree: ContentHash,
    events: Vec<HistoryEvent>,
) -> Result<CloudResponse, CloudError> {
    svc.store_mut().record_blob(tree, 32);
    svc.handle(
        actor,
        CloudRequest::PushCommit(PushCommit {
            uid: project,
            parents: parents.to_vec(),
            tree,
            events,
            sidecar: sidecar(),
        }),
    )
}

fn outcome_of(response: &CloudResponse) -> PushOutcome {
    match response {
        CloudResponse::PushResult(PushResult { outcome, .. }) => *outcome,
        other => panic!("expected PushResult, got {other:?}"),
    }
}

fn heads_of(response: &CloudResponse) -> Vec<ContentHash> {
    match response {
        CloudResponse::PushResult(PushResult { heads, .. }) => {
            heads.iter().map(|head| head.tree).collect()
        }
        other => panic!("expected PushResult, got {other:?}"),
    }
}
