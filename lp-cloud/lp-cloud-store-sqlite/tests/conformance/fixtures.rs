//! Records the conformance checks are built from.
//!
//! The `seed_*` helpers write parents before children. That is not
//! politeness: the SQLite adapter enforces foreign keys, so a membership
//! row for a project that was never stored is fatal there while the
//! in-memory adapter shrugs. Seeding through these helpers keeps every
//! check on the ground both adapters share.

use lp_cloud_domain::{
    AccountAccess, CloudProject, CloudUser, HeadRef, MemberRecord, MemberRole, MetaStore,
    ProjectRefs, SessionRecord,
};
use lpc_cloud_api::{Access, SidecarMeta};
use lpc_history::{ContentHash, EventKind, HistoryEvent, PrefixedUid, UidPrefix};

/// A stable user uid: the same `n` is the same account in every check.
pub fn user_uid(n: u8) -> PrefixedUid {
    PrefixedUid::mint(UidPrefix::User, &[n; 16])
}

/// A stable project uid.
pub fn project_uid(n: u8) -> PrefixedUid {
    PrefixedUid::mint(UidPrefix::Project, &[n; 16])
}

/// Store user `n` and hand back their uid.
pub fn seed_user(store: &mut dyn MetaStore, n: u8) -> PrefixedUid {
    let uid = user_uid(n);
    store.put_user(sample_user(uid, &format!("user{n}@example.com")));
    uid
}

/// Store project `n` owned by `owner` (whose row must already exist) and
/// hand back its uid.
pub fn seed_project(store: &mut dyn MetaStore, n: u8, owner: PrefixedUid) -> PrefixedUid {
    let uid = project_uid(n);
    store.put_project(sample_project(uid, owner));
    uid
}

/// A user record. The Google subject is derived from the uid so two
/// accounts never collide on it.
pub fn sample_user(uid: PrefixedUid, email: &str) -> CloudUser {
    CloudUser {
        uid,
        google_sub: format!("g-{uid}"),
        email: email.to_string(),
        display_name: "Sample".to_string(),
        given_name: None,
        family_name: None,
        picture_url: None,
        provider: "google".to_string(),
        created_at: 1.0,
        anonymous: false,
    }
}

/// A project whose link opens nothing, with a cosmetic slug.
pub fn sample_project(uid: PrefixedUid, owner: PrefixedUid) -> CloudProject {
    CloudProject {
        uid,
        owner,
        access: Access::None,
        slug: "sample".to_string(),
        created_at: 1.0,
        archived_at: None,
    }
}

/// A session for `user`, keyed by the hash of `token`.
pub fn sample_session(user: PrefixedUid, token: &[u8], expires_at: f64) -> SessionRecord {
    SessionRecord {
        token_hash: ContentHash::of(token),
        user,
        created_at: 1.0,
        expires_at,
        user_agent: None,
    }
}

/// A membership row. `user` is `None` for a pending invitation.
pub fn sample_member(
    project: PrefixedUid,
    email: &str,
    user: Option<PrefixedUid>,
    role: MemberRole,
) -> MemberRecord {
    MemberRecord {
        project,
        email: email.to_string(),
        user,
        role,
        added_at: 1.0,
    }
}

/// A head whose tree hashes `tree` and whose parents hash `parents`.
pub fn sample_head(tree: &[u8], parents: &[&[u8]]) -> HeadRef {
    HeadRef {
        tree: ContentHash::of(tree),
        parents: parents.iter().map(|bytes| ContentHash::of(bytes)).collect(),
    }
}

/// A frontier of the given heads, in the given order.
pub fn sample_refs(heads: Vec<HeadRef>) -> ProjectRefs {
    ProjectRefs { heads }
}

/// Display metadata with no preview.
pub fn sample_sidecar(name: &str) -> SidecarMeta {
    SidecarMeta {
        name: name.to_string(),
        format_version: 4,
        preview_png: None,
    }
}

/// An origin event at `at`.
pub fn sample_event(at: f64) -> HistoryEvent {
    HistoryEvent {
        at,
        kind: EventKind::Created,
    }
}

/// An account-access record for `user`, every byte field distinct so a
/// column swapped in an adapter's SQL cannot round-trip by accident.
pub fn sample_account_access(user: PrefixedUid) -> AccountAccess {
    AccountAccess {
        user,
        key_secret: [0x11; 32],
        key_salt: [0x22; 16],
        play_password_salt: [0x33; 16],
        edit_password_salt: [0x44; 16],
        play_password: None,
        edit_password: Some("crew only".to_string()),
        previous_key_salts: vec![[0x55; 16], [0x66; 16]],
        updated_at: 12.5,
    }
}
