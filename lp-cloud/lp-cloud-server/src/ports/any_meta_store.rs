//! Whichever [`MetaStore`] the configuration named.

use lp_cloud_domain::{
    CloudProject, CloudUser, MemberRecord, MetaStore, ProjectRefs, SessionRecord, StoredEvent,
};
use lpc_cloud_api::SidecarMeta;
use lpc_history::{ContentHash, HistoryEvent, PrefixedUid};

/// A [`MetaStore`] chosen at runtime.
///
/// [`CloudService`](lp_cloud_domain::CloudService) is generic over its
/// store, but the axum router needs **one** concrete state type, so the
/// choice between the in-memory and SQLite adapters has to collapse
/// somewhere. It collapses here, in a newtype over a boxed trait object —
/// the trait was made object-safe for exactly this kind of caller.
///
/// Every method forwards, and nothing else happens in this file. The
/// forwarding is written out rather than defaulted or macro-generated
/// because a defaulted trait method is a silent no-op waiting to happen in
/// a delegating wrapper (docs/… M4's "defaulted trait method + delegating
/// wrapper = silent no-op").
pub struct AnyMetaStore(Box<dyn MetaStore + Send>);

impl AnyMetaStore {
    /// Wrap a concrete adapter.
    pub fn new(store: impl MetaStore + Send + 'static) -> Self {
        Self(Box::new(store))
    }
}

impl MetaStore for AnyMetaStore {
    fn put_user(&mut self, user: CloudUser) {
        self.0.put_user(user);
    }

    fn user(&self, uid: PrefixedUid) -> Option<CloudUser> {
        self.0.user(uid)
    }

    fn user_by_google_sub(&self, google_sub: &str) -> Option<CloudUser> {
        self.0.user_by_google_sub(google_sub)
    }

    fn user_by_email(&self, email: &str) -> Option<CloudUser> {
        self.0.user_by_email(email)
    }

    fn users(&self, limit: usize) -> Vec<CloudUser> {
        self.0.users(limit)
    }

    fn put_session(&mut self, session: SessionRecord) {
        self.0.put_session(session);
    }

    fn session(&self, token_hash: ContentHash) -> Option<SessionRecord> {
        self.0.session(token_hash)
    }

    fn delete_session(&mut self, token_hash: ContentHash) {
        self.0.delete_session(token_hash);
    }

    fn sessions_for_user(&self, user: PrefixedUid) -> Vec<SessionRecord> {
        self.0.sessions_for_user(user)
    }

    fn put_project(&mut self, project: CloudProject) {
        self.0.put_project(project);
    }

    fn project(&self, uid: PrefixedUid) -> Option<CloudProject> {
        self.0.project(uid)
    }

    fn projects_for_user(&self, user: PrefixedUid) -> Vec<CloudProject> {
        self.0.projects_for_user(user)
    }

    fn put_member(&mut self, member: MemberRecord) {
        self.0.put_member(member);
    }

    fn remove_member(&mut self, project: PrefixedUid, email: &str) -> bool {
        self.0.remove_member(project, email)
    }

    fn members(&self, project: PrefixedUid) -> Vec<MemberRecord> {
        self.0.members(project)
    }

    fn member_for_user(&self, project: PrefixedUid, user: PrefixedUid) -> Option<MemberRecord> {
        self.0.member_for_user(project, user)
    }

    fn resolve_pending_members(&mut self, email: &str, user: PrefixedUid) -> usize {
        self.0.resolve_pending_members(email, user)
    }

    fn refs(&self, project: PrefixedUid) -> ProjectRefs {
        self.0.refs(project)
    }

    fn put_refs(&mut self, project: PrefixedUid, refs: ProjectRefs) {
        self.0.put_refs(project, refs);
    }

    fn sidecar(&self, project: PrefixedUid) -> Option<SidecarMeta> {
        self.0.sidecar(project)
    }

    fn put_sidecar(&mut self, project: PrefixedUid, sidecar: SidecarMeta) {
        self.0.put_sidecar(project, sidecar);
    }

    fn append_events(&mut self, project: PrefixedUid, events: &[HistoryEvent]) -> u64 {
        self.0.append_events(project, events)
    }

    fn events(&self, project: PrefixedUid) -> Vec<StoredEvent> {
        self.0.events(project)
    }

    fn events_since(&self, project: PrefixedUid, since: u64) -> Vec<StoredEvent> {
        self.0.events_since(project, since)
    }

    fn last_event_seq(&self, project: PrefixedUid) -> u64 {
        self.0.last_event_seq(project)
    }

    fn has_blob(&self, hash: ContentHash) -> bool {
        self.0.has_blob(hash)
    }

    fn record_blob(&mut self, hash: ContentHash, size: u64) {
        self.0.record_blob(hash, size);
    }

    fn blob_size(&self, hash: ContentHash) -> Option<u64> {
        self.0.blob_size(hash)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lp_cloud_store_mem::MemMetaStore;
    use lpc_history::UidPrefix;

    /// The wrapper is a pass-through, not a second store: what goes in
    /// through the box comes back out of it.
    #[test]
    fn forwards_reads_and_writes() {
        let mut store = AnyMetaStore::new(MemMetaStore::new());
        let hash = ContentHash::of(b"blob");
        store.record_blob(hash, 4);

        assert!(store.has_blob(hash));
        assert_eq!(store.blob_size(hash), Some(4));
        assert_eq!(
            store.project(PrefixedUid::mint(UidPrefix::Project, &[1u8; 16])),
            None
        );
    }
}
