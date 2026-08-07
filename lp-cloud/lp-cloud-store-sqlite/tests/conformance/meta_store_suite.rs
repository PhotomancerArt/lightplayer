//! What every `MetaStore` must do, whatever it stores state in.
//!
//! Each check takes `&mut dyn MetaStore` and gets a store that is empty.
//! Instantiate the whole battery for an adapter with
//! [`meta_store_conformance_tests!`].

use lp_cloud_domain::{CloudProject, MemberRole, MetaStore};
use lpc_cloud_api::Visibility;
use lpc_history::ContentHash;

use crate::conformance::fixtures::{
    project_uid, sample_event, sample_head, sample_member, sample_refs, sample_session,
    sample_sidecar, sample_user, seed_project, seed_user, user_uid,
};

/// Generate the whole `MetaStore` battery as `#[test]` functions.
///
/// `$with_store` names a function taking `impl FnOnce(&mut dyn MetaStore)`
/// that builds a fresh, empty store, runs the check against it, and tears
/// it down.
macro_rules! meta_store_conformance_tests {
    ($with_store:path) => {
        $crate::conformance::meta_store_suite::meta_store_conformance_tests!(
            @checks $with_store,
            users_round_trip_by_uid_subject_and_email,
            replacing_a_user_updates_its_lookup_indexes,
            unknown_user_lookups_are_none,
            sessions_round_trip_by_token_hash,
            deleting_a_session_is_idempotent,
            expired_sessions_are_still_returned,
            projects_round_trip_by_uid,
            replacing_a_project_keeps_its_members_refs_events_and_sidecar,
            projects_for_user_lists_only_resolved_memberships,
            pending_membership_resolves_at_first_login,
            membership_rows_are_keyed_by_project_and_email,
            member_for_user_ignores_pending_rows,
            refs_round_trip_a_multi_head_frontier_with_parents,
            refs_of_an_unknown_project_are_empty,
            replacing_the_frontier_drops_the_heads_that_are_gone,
            sidecars_round_trip_and_replace,
            event_log_sequences_are_monotonic_per_project,
            events_since_reads_forward_without_overlap,
            appending_nothing_reports_the_current_end,
            the_blob_index_records_sizes_and_is_idempotent,
            an_unrecorded_blob_is_absent_from_the_index,
        );
    };
    (@checks $with_store:path, $($check:ident),+ $(,)?) => {
        $(
            #[test]
            fn $check() {
                $with_store($crate::conformance::meta_store_suite::$check);
            }
        )+
    };
}

pub(crate) use meta_store_conformance_tests;

// ---- users ------------------------------------------------------------

pub fn users_round_trip_by_uid_subject_and_email(store: &mut dyn MetaStore) {
    let uid = user_uid(1);
    let user = sample_user(uid, "one@example.com");
    store.put_user(user.clone());

    assert_eq!(store.user(uid).as_ref(), Some(&user));
    assert_eq!(
        store.user_by_google_sub(&user.google_sub).as_ref(),
        Some(&user)
    );
    assert_eq!(store.user_by_email("one@example.com").as_ref(), Some(&user));
}

/// A user whose email changed must not still be findable at the old
/// address — a stale index here is an access-control bug, because a pending
/// invitation resolves by email.
pub fn replacing_a_user_updates_its_lookup_indexes(store: &mut dyn MetaStore) {
    let uid = user_uid(1);
    store.put_user(sample_user(uid, "old@example.com"));
    store.put_user(sample_user(uid, "new@example.com"));

    assert_eq!(store.user_by_email("old@example.com"), None);
    assert_eq!(
        store.user_by_email("new@example.com").map(|user| user.uid),
        Some(uid)
    );
    assert_eq!(
        store.user(uid).map(|user| user.email).as_deref(),
        Some("new@example.com")
    );
}

pub fn unknown_user_lookups_are_none(store: &mut dyn MetaStore) {
    assert_eq!(store.user(user_uid(9)), None);
    assert_eq!(store.user_by_google_sub("g-nobody"), None);
    assert_eq!(store.user_by_email("nobody@example.com"), None);
}

// ---- sessions ---------------------------------------------------------

pub fn sessions_round_trip_by_token_hash(store: &mut dyn MetaStore) {
    let user = seed_user(store, 1);
    let session = sample_session(user, b"token", 100.0);
    store.put_session(session.clone());

    assert_eq!(store.session(session.token_hash).as_ref(), Some(&session));
    assert_eq!(store.session(ContentHash::of(b"other token")), None);
}

pub fn deleting_a_session_is_idempotent(store: &mut dyn MetaStore) {
    let user = seed_user(store, 1);
    let session = sample_session(user, b"token", 100.0);
    store.put_session(session.clone());

    store.delete_session(session.token_hash);
    assert_eq!(store.session(session.token_hash), None);
    // Logging out twice is not an error.
    store.delete_session(session.token_hash);
    assert_eq!(store.session(session.token_hash), None);
}

/// Expiry is the domain's business, not the store's: a store that hid
/// expired rows would make "log out everywhere" untestable and would answer
/// a question nobody asked it.
pub fn expired_sessions_are_still_returned(store: &mut dyn MetaStore) {
    let user = seed_user(store, 1);
    let session = sample_session(user, b"stale", 0.0);
    store.put_session(session.clone());

    assert_eq!(store.session(session.token_hash), Some(session));
}

// ---- projects ---------------------------------------------------------

pub fn projects_round_trip_by_uid(store: &mut dyn MetaStore) {
    let owner = seed_user(store, 1);
    let uid = seed_project(store, 1, owner);

    let project = store.project(uid).expect("the project was stored");
    assert_eq!(project.owner, owner);
    assert_eq!(project.visibility, Visibility::Private);
    assert_eq!(store.project(project_uid(9)), None);
}

/// `put_project` is an upsert, and the service calls it that way — a
/// visibility change re-puts the record. Everything hanging off the project
/// has to survive that. (In SQL this is the difference between an upsert
/// and `INSERT OR REPLACE`, which deletes the row first and takes every
/// cascading child with it.)
pub fn replacing_a_project_keeps_its_members_refs_events_and_sidecar(store: &mut dyn MetaStore) {
    let owner = seed_user(store, 1);
    let project = seed_project(store, 1, owner);
    store.put_member(sample_member(
        project,
        "owner@example.com",
        Some(owner),
        MemberRole::Owner,
    ));
    store.put_refs(project, sample_refs(vec![sample_head(b"tree", &[])]));
    store.append_events(project, &[sample_event(1.0)]);
    store.put_sidecar(project, sample_sidecar("Zook Dome"));

    let existing = store.project(project).expect("the project was stored");
    store.put_project(CloudProject {
        visibility: Visibility::Link,
        ..existing
    });

    assert_eq!(
        store.project(project).map(|project| project.visibility),
        Some(Visibility::Link)
    );
    assert_eq!(store.members(project).len(), 1);
    assert_eq!(store.refs(project).heads.len(), 1);
    assert_eq!(store.events(project).len(), 1);
    assert_eq!(store.last_event_seq(project), 1);
    assert_eq!(
        store
            .sidecar(project)
            .map(|sidecar| sidecar.name)
            .as_deref(),
        Some("Zook Dome")
    );
}

pub fn projects_for_user_lists_only_resolved_memberships(store: &mut dyn MetaStore) {
    let owner = seed_user(store, 1);
    let member = seed_user(store, 2);
    let joined = seed_project(store, 1, owner);
    let invited = seed_project(store, 2, owner);

    store.put_member(sample_member(
        joined,
        "user2@example.com",
        Some(member),
        MemberRole::Member,
    ));
    // Pending: an invitation is not a key.
    store.put_member(sample_member(
        invited,
        "user2@example.com",
        None,
        MemberRole::Member,
    ));

    let uids: Vec<_> = store
        .projects_for_user(member)
        .into_iter()
        .map(|project| project.uid)
        .collect();
    assert_eq!(uids, vec![joined]);
    assert!(store.projects_for_user(user_uid(9)).is_empty());
}

/// The Q4 hook at store level: an invitation by email grants nothing until
/// that address resolves to an account, and resolving it is what puts the
/// project in their list.
pub fn pending_membership_resolves_at_first_login(store: &mut dyn MetaStore) {
    let owner = seed_user(store, 1);
    let project = seed_project(store, 1, owner);
    store.put_member(sample_member(
        project,
        "later@example.com",
        None,
        MemberRole::Member,
    ));

    let user = seed_user(store, 2);
    assert_eq!(store.member_for_user(project, user), None);
    assert!(store.projects_for_user(user).is_empty());

    assert_eq!(store.resolve_pending_members("later@example.com", user), 1);
    assert!(store.member_for_user(project, user).is_some());
    assert_eq!(store.projects_for_user(user).len(), 1);

    // Already resolved: a second login resolves nothing new.
    assert_eq!(store.resolve_pending_members("later@example.com", user), 0);
}

pub fn membership_rows_are_keyed_by_project_and_email(store: &mut dyn MetaStore) {
    let owner = seed_user(store, 1);
    let project = seed_project(store, 1, owner);
    let other = seed_project(store, 2, owner);
    for (project, email) in [
        (project, "b@example.com"),
        (project, "a@example.com"),
        (other, "c@example.com"),
    ] {
        store.put_member(sample_member(project, email, None, MemberRole::Member));
    }

    // Deterministic order: email-sorted within a project.
    let emails: Vec<String> = store
        .members(project)
        .into_iter()
        .map(|member| member.email)
        .collect();
    assert_eq!(emails, vec!["a@example.com", "b@example.com"]);

    assert!(store.remove_member(project, "a@example.com"));
    assert!(!store.remove_member(project, "a@example.com"));
    assert_eq!(store.members(project).len(), 1);
    assert_eq!(store.members(other).len(), 1);

    // Re-adding the same email replaces the row rather than doubling it.
    store.put_member(sample_member(
        project,
        "b@example.com",
        None,
        MemberRole::Owner,
    ));
    assert_eq!(store.members(project).len(), 1);
    assert_eq!(store.members(project)[0].role, MemberRole::Owner);
}

pub fn member_for_user_ignores_pending_rows(store: &mut dyn MetaStore) {
    let owner = seed_user(store, 1);
    let user = seed_user(store, 2);
    let project = seed_project(store, 1, owner);
    store.put_member(sample_member(
        project,
        "pending@example.com",
        None,
        MemberRole::Member,
    ));

    assert_eq!(store.member_for_user(project, user), None);

    store.put_member(sample_member(
        project,
        "user2@example.com",
        Some(user),
        MemberRole::Member,
    ));
    let found = store
        .member_for_user(project, user)
        .expect("the resolved row");
    assert_eq!(found.email, "user2@example.com");
}

// ---- refs / heads -----------------------------------------------------

pub fn refs_round_trip_a_multi_head_frontier_with_parents(store: &mut dyn MetaStore) {
    let owner = seed_user(store, 1);
    let project = seed_project(store, 1, owner);
    assert!(store.refs(project).is_empty());

    let refs = sample_refs(vec![
        sample_head(b"head-a", &[b"base"]),
        sample_head(b"head-b", &[b"base", b"other"]),
    ]);
    store.put_refs(project, refs.clone());

    // Both heads, in order, each with its parents: the frontier is the
    // whole state a client needs to build a clobber join.
    assert_eq!(store.refs(project), refs);
}

pub fn refs_of_an_unknown_project_are_empty(store: &mut dyn MetaStore) {
    assert!(store.refs(project_uid(9)).is_empty());
    assert_eq!(store.refs(project_uid(9)).heads.len(), 0);
}

pub fn replacing_the_frontier_drops_the_heads_that_are_gone(store: &mut dyn MetaStore) {
    let owner = seed_user(store, 1);
    let project = seed_project(store, 1, owner);
    store.put_refs(
        project,
        sample_refs(vec![
            sample_head(b"head-a", &[]),
            sample_head(b"head-b", &[]),
        ]),
    );

    // A clobber join collapses two heads into one; the old ones must not
    // linger as a phantom frontier.
    let joined = sample_refs(vec![sample_head(b"join", &[b"head-a", b"head-b"])]);
    store.put_refs(project, joined.clone());
    assert_eq!(store.refs(project), joined);
}

// ---- sidecars ---------------------------------------------------------

pub fn sidecars_round_trip_and_replace(store: &mut dyn MetaStore) {
    let owner = seed_user(store, 1);
    let project = seed_project(store, 1, owner);
    assert_eq!(store.sidecar(project), None);

    let first = sample_sidecar("Zook Dome");
    store.put_sidecar(project, first.clone());
    assert_eq!(store.sidecar(project), Some(first));

    let mut second = sample_sidecar("Zook Dome v2");
    second.preview_png = Some(ContentHash::of(b"preview"));
    store.put_sidecar(project, second.clone());
    assert_eq!(store.sidecar(project), Some(second));
}

// ---- event log --------------------------------------------------------

pub fn event_log_sequences_are_monotonic_per_project(store: &mut dyn MetaStore) {
    let owner = seed_user(store, 1);
    let project = seed_project(store, 1, owner);
    let other = seed_project(store, 2, owner);

    assert_eq!(store.last_event_seq(project), 0);
    assert!(store.events(project).is_empty());

    assert_eq!(
        store.append_events(project, &[sample_event(1.0), sample_event(2.0)]),
        2
    );
    assert_eq!(store.append_events(project, &[sample_event(3.0)]), 3);
    // Sequences are per project, not global.
    assert_eq!(store.append_events(other, &[sample_event(1.0)]), 1);

    assert_eq!(store.last_event_seq(project), 3);
    let seqs: Vec<u64> = store
        .events(project)
        .into_iter()
        .map(|entry| entry.seq)
        .collect();
    assert_eq!(seqs, vec![1, 2, 3]);

    // Events come back exactly as they were pushed.
    assert_eq!(store.events(project)[1].event, sample_event(2.0));
}

pub fn events_since_reads_forward_without_overlap(store: &mut dyn MetaStore) {
    let owner = seed_user(store, 1);
    let project = seed_project(store, 1, owner);
    store.append_events(
        project,
        &[sample_event(1.0), sample_event(2.0), sample_event(3.0)],
    );

    assert_eq!(store.events_since(project, 0).len(), 3);
    let tail: Vec<u64> = store
        .events_since(project, 1)
        .into_iter()
        .map(|entry| entry.seq)
        .collect();
    assert_eq!(tail, vec![2, 3]);
    assert!(store.events_since(project, 3).is_empty());
    assert!(store.events_since(project_uid(9), 0).is_empty());
}

pub fn appending_nothing_reports_the_current_end(store: &mut dyn MetaStore) {
    let owner = seed_user(store, 1);
    let project = seed_project(store, 1, owner);

    assert_eq!(store.append_events(project, &[]), 0);
    store.append_events(project, &[sample_event(1.0)]);
    assert_eq!(store.append_events(project, &[]), 1);
    assert_eq!(store.events(project).len(), 1);
}

// ---- blob index -------------------------------------------------------

pub fn the_blob_index_records_sizes_and_is_idempotent(store: &mut dyn MetaStore) {
    let hash = ContentHash::of(b"blob");
    store.record_blob(hash, 12);
    assert!(store.has_blob(hash));
    assert_eq!(store.blob_size(hash), Some(12));

    // Recording it again is a no-op, not a duplicate row.
    store.record_blob(hash, 12);
    assert_eq!(store.blob_size(hash), Some(12));
}

pub fn an_unrecorded_blob_is_absent_from_the_index(store: &mut dyn MetaStore) {
    let hash = ContentHash::of(b"never recorded");
    assert!(!store.has_blob(hash));
    assert_eq!(store.blob_size(hash), None);
}
