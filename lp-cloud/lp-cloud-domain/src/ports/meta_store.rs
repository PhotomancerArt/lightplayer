//! All service state, behind one trait.

use alloc::string::String;
use alloc::vec::Vec;
use lpc_cloud_api::SidecarMeta;
use lpc_history::{ContentHash, HistoryEvent, PrefixedUid};

use crate::model::cloud_project::CloudProject;
use crate::model::cloud_user::CloudUser;
use crate::model::member_record::MemberRecord;
use crate::model::project_refs::ProjectRefs;
use crate::model::session_record::SessionRecord;
use crate::model::stored_event::StoredEvent;

/// Everything the service remembers: users, sessions, projects, membership,
/// head refs, sidecars, the per-project event log, and the blob index.
///
/// # Why one trait
///
/// These are **one consistency domain**, not seven. A push appends events,
/// moves the frontier, and replaces the sidecar; a login upserts a user and
/// resolves their pending memberships. Each of those is one SQLite
/// transaction in the real adapter (P04), and splitting the trait would
/// invite an implementation that can half-apply one.
///
/// # Why the methods are infallible
///
/// `CloudError` is the client-facing vocabulary and deliberately has no
/// backend-failure code — there is nothing useful to tell a client about a
/// disk that stopped answering. So the port is total from the domain's point
/// of view, and an adapter whose backend can fail owns that policy itself
/// (for a single-process service over a local SQLite file, a failing
/// statement is a bug or a dead disk — fatal either way). This is what lets
/// [`CloudService::handle`](crate::cloud_service::CloudService::handle)
/// return exactly `Result<CloudResponse, CloudError>` without inventing an
/// error the API does not have.
///
/// # Implementing one
///
/// The trait is object-safe (`&mut dyn MetaStore`) so one conformance suite
/// can run against every adapter — the mem store and the SQLite store must
/// not drift. Nothing here has a default body on purpose: a defaulted method
/// is a silent no-op waiting to happen in a delegating wrapper.
/// Query methods return owned values rather than references, because a real
/// adapter reads rows rather than handing out borrows into its own map.
pub trait MetaStore {
    // ---- users -------------------------------------------------------

    /// Insert or replace a user record, keeping any lookup indexes in step.
    fn put_user(&mut self, user: CloudUser);

    /// Look a user up by uid.
    fn user(&self, uid: PrefixedUid) -> Option<CloudUser>;

    /// Look a user up by Google subject identifier (their real identity).
    fn user_by_google_sub(&self, google_sub: &str) -> Option<CloudUser>;

    /// Look a user up by normalized email.
    fn user_by_email(&self, email: &str) -> Option<CloudUser>;

    /// Up to `limit` accounts, oldest (`created_at`) first — the dev
    /// picker's candidate list (P3): a deployment with the picker on shows
    /// its earliest-seeded profiles, not an arbitrary sample.
    fn users(&self, limit: usize) -> Vec<CloudUser>;

    // ---- sessions ----------------------------------------------------

    /// Insert or replace a session row, keyed by its token hash.
    fn put_session(&mut self, session: SessionRecord);

    /// Look a session up by [`session_token_hash`](crate::model::session_record::session_token_hash).
    /// Expiry is the domain's business, not the store's — this returns
    /// expired rows too.
    fn session(&self, token_hash: ContentHash) -> Option<SessionRecord>;

    /// Delete a session row (logout). Silent if there was none.
    fn delete_session(&mut self, token_hash: ContentHash);

    /// Every session open on this account, in no particular order — a
    /// caller that cares about order (`ListSessions` sorts newest-first)
    /// does its own sort. Includes expired rows, same as
    /// [`session`](Self::session): expiry is the domain's business.
    fn sessions_for_user(&self, user: PrefixedUid) -> Vec<SessionRecord>;

    // ---- projects ----------------------------------------------------

    /// Insert or replace a project record.
    fn put_project(&mut self, project: CloudProject);

    /// Look a project up by uid.
    fn project(&self, uid: PrefixedUid) -> Option<CloudProject>;

    /// Every project this user is a *resolved* member of, ordered by uid.
    /// Pending (unresolved) membership rows do not count.
    fn projects_for_user(&self, user: PrefixedUid) -> Vec<CloudProject>;

    // ---- membership --------------------------------------------------

    /// Insert or replace a membership row, keyed by `(project, email)`.
    fn put_member(&mut self, member: MemberRecord);

    /// Remove a membership row. Returns whether one was there.
    fn remove_member(&mut self, project: PrefixedUid, email: &str) -> bool;

    /// Every membership row on a project, ordered by email.
    fn members(&self, project: PrefixedUid) -> Vec<MemberRecord>;

    /// The membership row granting this *account* access, if any. Only
    /// resolved rows match: a pending invitation is not a key.
    fn member_for_user(&self, project: PrefixedUid, user: PrefixedUid) -> Option<MemberRecord>;

    /// Attach an account to every pending membership row for its email
    /// (Q4). Returns how many rows were resolved.
    ///
    /// This is the moment an invited-by-email person actually gains access,
    /// and it belongs to the store because it is a single indexed update
    /// across many rows.
    fn resolve_pending_members(&mut self, email: &str, user: PrefixedUid) -> usize;

    // ---- refs / heads ------------------------------------------------

    /// A project's head frontier. An unknown or commit-less project has an
    /// empty one.
    fn refs(&self, project: PrefixedUid) -> ProjectRefs;

    /// Replace a project's head frontier.
    fn put_refs(&mut self, project: PrefixedUid, refs: ProjectRefs);

    // ---- sidecars ----------------------------------------------------

    /// A project's most recent client-computed display metadata.
    fn sidecar(&self, project: PrefixedUid) -> Option<SidecarMeta>;

    /// Replace a project's display metadata. Stored verbatim — the server
    /// never derives or corrects it (D3).
    fn put_sidecar(&mut self, project: PrefixedUid, sidecar: SidecarMeta);

    // ---- event log ---------------------------------------------------

    /// Append events to a project's log, assigning each the next sequence
    /// number. Returns the last sequence number assigned (or the log's
    /// current last, if `events` is empty).
    fn append_events(&mut self, project: PrefixedUid, events: &[HistoryEvent]) -> u64;

    /// A project's whole log, in sequence order.
    fn events(&self, project: PrefixedUid) -> Vec<StoredEvent>;

    /// A project's log entries with `seq > since`, in sequence order.
    fn events_since(&self, project: PrefixedUid, since: u64) -> Vec<StoredEvent>;

    /// The highest sequence number in a project's log, or 0 if empty.
    fn last_event_seq(&self, project: PrefixedUid) -> u64;

    // ---- blob index --------------------------------------------------

    /// Whether the service holds this blob.
    ///
    /// The *index* lives here rather than on [`BlobStore`](crate::ports::blob_store::BlobStore)
    /// because push validation reads it inside the same transaction as the
    /// rest of a push. The edge records a blob here after storing its bytes.
    fn has_blob(&self, hash: ContentHash) -> bool;

    /// Record that a blob of `size` bytes is stored. Idempotent.
    fn record_blob(&mut self, hash: ContentHash, size: u64);

    /// A stored blob's size in bytes, if the index knows it.
    fn blob_size(&self, hash: ContentHash) -> Option<u64>;
}

/// Normalize an email for storage and lookup: trimmed and lowercased.
///
/// Every email key in the store is normalized this way, so an invitation to
/// `Yona@Example.com` resolves when `yona@example.com` logs in.
pub fn normalize_email(email: &str) -> String {
    email.trim().to_lowercase()
}
