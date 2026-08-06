//! Every piece of service state, in `BTreeMap`s.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use lp_cloud_domain::{
    CloudProject, CloudUser, MemberRecord, MetaStore, ProjectRefs, SessionRecord, StoredEvent,
};
use lpc_cloud_api::SidecarMeta;
use lpc_history::{ContentHash, HistoryEvent, PrefixedUid};

/// The whole service state in memory.
///
/// Maps are keyed the way the SQLite schema is indexed, so the two adapters
/// answer the same questions the same way: users by uid with secondary
/// indexes on Google subject and email, membership by `(project, email)`,
/// events as a per-project append-only vector.
#[derive(Debug, Clone, Default)]
pub struct MemMetaStore {
    users: BTreeMap<PrefixedUid, CloudUser>,
    users_by_google_sub: BTreeMap<String, PrefixedUid>,
    users_by_email: BTreeMap<String, PrefixedUid>,
    sessions: BTreeMap<ContentHash, SessionRecord>,
    projects: BTreeMap<PrefixedUid, CloudProject>,
    members: BTreeMap<(PrefixedUid, String), MemberRecord>,
    refs: BTreeMap<PrefixedUid, ProjectRefs>,
    sidecars: BTreeMap<PrefixedUid, SidecarMeta>,
    events: BTreeMap<PrefixedUid, Vec<StoredEvent>>,
    blob_index: BTreeMap<ContentHash, u64>,
}

impl MemMetaStore {
    /// An empty store.
    pub fn new() -> Self {
        Self::default()
    }

    /// Drop the secondary index entries pointing at a user, so a replacement
    /// record cannot leave a stale email or subject behind.
    fn unindex_user(&mut self, uid: PrefixedUid) {
        if let Some(previous) = self.users.get(&uid) {
            let google_sub = previous.google_sub.clone();
            let email = previous.email.clone();
            self.users_by_google_sub.remove(&google_sub);
            self.users_by_email.remove(&email);
        }
    }
}

impl MetaStore for MemMetaStore {
    // ---- users -------------------------------------------------------

    fn put_user(&mut self, user: CloudUser) {
        self.unindex_user(user.uid);
        self.users_by_google_sub
            .insert(user.google_sub.clone(), user.uid);
        self.users_by_email.insert(user.email.clone(), user.uid);
        self.users.insert(user.uid, user);
    }

    fn user(&self, uid: PrefixedUid) -> Option<CloudUser> {
        self.users.get(&uid).cloned()
    }

    fn user_by_google_sub(&self, google_sub: &str) -> Option<CloudUser> {
        self.users_by_google_sub
            .get(google_sub)
            .and_then(|uid| self.users.get(uid))
            .cloned()
    }

    fn user_by_email(&self, email: &str) -> Option<CloudUser> {
        self.users_by_email
            .get(email)
            .and_then(|uid| self.users.get(uid))
            .cloned()
    }

    // ---- sessions ----------------------------------------------------

    fn put_session(&mut self, session: SessionRecord) {
        self.sessions.insert(session.token_hash, session);
    }

    fn session(&self, token_hash: ContentHash) -> Option<SessionRecord> {
        self.sessions.get(&token_hash).cloned()
    }

    fn delete_session(&mut self, token_hash: ContentHash) {
        self.sessions.remove(&token_hash);
    }

    // ---- projects ----------------------------------------------------

    fn put_project(&mut self, project: CloudProject) {
        self.projects.insert(project.uid, project);
    }

    fn project(&self, uid: PrefixedUid) -> Option<CloudProject> {
        self.projects.get(&uid).cloned()
    }

    fn projects_for_user(&self, user: PrefixedUid) -> Vec<CloudProject> {
        self.members
            .values()
            .filter(|member| member.user == Some(user))
            .filter_map(|member| self.projects.get(&member.project))
            .cloned()
            .collect()
    }

    // ---- membership --------------------------------------------------

    fn put_member(&mut self, member: MemberRecord) {
        self.members
            .insert((member.project, member.email.clone()), member);
    }

    fn remove_member(&mut self, project: PrefixedUid, email: &str) -> bool {
        self.members.remove(&(project, email.into())).is_some()
    }

    fn members(&self, project: PrefixedUid) -> Vec<MemberRecord> {
        self.members
            .iter()
            .filter(|((row_project, _), _)| *row_project == project)
            .map(|(_, member)| member.clone())
            .collect()
    }

    fn member_for_user(&self, project: PrefixedUid, user: PrefixedUid) -> Option<MemberRecord> {
        self.members
            .iter()
            .find(|((row_project, _), member)| *row_project == project && member.user == Some(user))
            .map(|(_, member)| member.clone())
    }

    fn resolve_pending_members(&mut self, email: &str, user: PrefixedUid) -> usize {
        let mut resolved = 0;
        for member in self.members.values_mut() {
            if member.email == email && member.user.is_none() {
                member.user = Some(user);
                resolved += 1;
            }
        }
        resolved
    }

    // ---- refs / heads ------------------------------------------------

    fn refs(&self, project: PrefixedUid) -> ProjectRefs {
        self.refs.get(&project).cloned().unwrap_or_default()
    }

    fn put_refs(&mut self, project: PrefixedUid, refs: ProjectRefs) {
        self.refs.insert(project, refs);
    }

    // ---- sidecars ----------------------------------------------------

    fn sidecar(&self, project: PrefixedUid) -> Option<SidecarMeta> {
        self.sidecars.get(&project).cloned()
    }

    fn put_sidecar(&mut self, project: PrefixedUid, sidecar: SidecarMeta) {
        self.sidecars.insert(project, sidecar);
    }

    // ---- event log ---------------------------------------------------

    fn append_events(&mut self, project: PrefixedUid, events: &[HistoryEvent]) -> u64 {
        let log = self.events.entry(project).or_default();
        let mut seq = log.last().map(|entry| entry.seq).unwrap_or(0);
        for event in events {
            seq += 1;
            log.push(StoredEvent {
                seq,
                event: event.clone(),
            });
        }
        seq
    }

    fn events(&self, project: PrefixedUid) -> Vec<StoredEvent> {
        self.events.get(&project).cloned().unwrap_or_default()
    }

    fn events_since(&self, project: PrefixedUid, since: u64) -> Vec<StoredEvent> {
        self.events
            .get(&project)
            .map(|log| {
                log.iter()
                    .filter(|entry| entry.seq > since)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    fn last_event_seq(&self, project: PrefixedUid) -> u64 {
        self.events
            .get(&project)
            .and_then(|log| log.last())
            .map(|entry| entry.seq)
            .unwrap_or(0)
    }

    // ---- blob index --------------------------------------------------

    fn has_blob(&self, hash: ContentHash) -> bool {
        self.blob_index.contains_key(&hash)
    }

    fn record_blob(&mut self, hash: ContentHash, size: u64) {
        self.blob_index.insert(hash, size);
    }

    fn blob_size(&self, hash: ContentHash) -> Option<u64> {
        self.blob_index.get(&hash).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;
    use alloc::vec;
    use lp_cloud_domain::{HeadRef, MemberRole};
    use lpc_cloud_api::Visibility;
    use lpc_history::{EventKind, UidPrefix};

    /// The pending-membership hook (Q4) at store level: an invitation by
    /// email grants nothing until that address resolves to an account, and
    /// resolving it is what puts the project in their list.
    #[test]
    fn pending_membership_resolves_at_first_login() {
        let mut store = MemMetaStore::new();
        let project = project_uid(1);
        store.put_project(sample_project(project, user_uid(9)));
        store.put_member(MemberRecord {
            project,
            email: "later@example.com".to_string(),
            user: None,
            role: MemberRole::Member,
            added_at: 1.0,
        });

        let user = user_uid(1);
        assert_eq!(store.member_for_user(project, user), None);
        assert!(store.projects_for_user(user).is_empty());

        assert_eq!(store.resolve_pending_members("later@example.com", user), 1);
        assert!(store.member_for_user(project, user).is_some());
        assert_eq!(store.projects_for_user(user).len(), 1);

        // Already resolved: a second login resolves nothing new.
        assert_eq!(store.resolve_pending_members("later@example.com", user), 0);
    }

    #[test]
    fn replacing_a_user_does_not_leave_a_stale_email_index() {
        let mut store = MemMetaStore::new();
        let uid = user_uid(1);
        store.put_user(sample_user(uid, "old@example.com"));
        store.put_user(sample_user(uid, "new@example.com"));

        assert!(store.user_by_email("old@example.com").is_none());
        assert_eq!(
            store.user_by_email("new@example.com").map(|u| u.uid),
            Some(uid)
        );
        assert_eq!(store.user_by_google_sub("g-1").map(|u| u.uid), Some(uid));
    }

    #[test]
    fn membership_rows_are_keyed_by_project_and_email() {
        let mut store = MemMetaStore::new();
        let project = project_uid(1);
        let other = project_uid(2);
        for (project, email) in [
            (project, "b@example.com"),
            (project, "a@example.com"),
            (other, "c@example.com"),
        ] {
            store.put_member(MemberRecord {
                project,
                email: email.to_string(),
                user: None,
                role: MemberRole::Member,
                added_at: 1.0,
            });
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
    }

    #[test]
    fn event_log_sequences_are_monotonic_per_project() {
        let mut store = MemMetaStore::new();
        let project = project_uid(1);
        let other = project_uid(2);

        assert_eq!(store.last_event_seq(project), 0);
        assert!(store.events(project).is_empty());

        assert_eq!(store.append_events(project, &[event(1.0), event(2.0)]), 2);
        assert_eq!(store.append_events(project, &[event(3.0)]), 3);
        assert_eq!(store.append_events(other, &[event(1.0)]), 1);

        assert_eq!(store.last_event_seq(project), 3);
        let seqs: Vec<u64> = store
            .events(project)
            .into_iter()
            .map(|entry| entry.seq)
            .collect();
        assert_eq!(seqs, vec![1, 2, 3]);
        assert_eq!(store.events_since(project, 2).len(), 1);
        assert!(store.events_since(project, 3).is_empty());

        // Appending nothing reports the log's current end.
        assert_eq!(store.append_events(project, &[]), 3);
    }

    #[test]
    fn refs_sidecars_and_blob_index_round_trip() {
        let mut store = MemMetaStore::new();
        let project = project_uid(1);
        assert!(store.refs(project).is_empty());
        assert_eq!(store.sidecar(project), None);

        let refs = ProjectRefs {
            heads: vec![HeadRef {
                tree: ContentHash::of(b"tree"),
                parents: vec![],
            }],
        };
        store.put_refs(project, refs.clone());
        assert_eq!(store.refs(project), refs);

        let sidecar = SidecarMeta {
            name: "Zook Dome".to_string(),
            format_version: 4,
            preview_png: None,
        };
        store.put_sidecar(project, sidecar.clone());
        assert_eq!(store.sidecar(project), Some(sidecar));

        let hash = ContentHash::of(b"blob");
        assert!(!store.has_blob(hash));
        store.record_blob(hash, 12);
        assert!(store.has_blob(hash));
        assert_eq!(store.blob_size(hash), Some(12));
    }

    #[test]
    fn sessions_round_trip_by_token_hash() {
        let mut store = MemMetaStore::new();
        let token_hash = ContentHash::of(b"token");
        let record = SessionRecord {
            token_hash,
            user: user_uid(1),
            expires_at: 100.0,
        };
        store.put_session(record.clone());
        assert_eq!(store.session(token_hash), Some(record));
        store.delete_session(token_hash);
        assert_eq!(store.session(token_hash), None);
    }

    // ---- helpers ------------------------------------------------------

    fn user_uid(n: u8) -> PrefixedUid {
        PrefixedUid::mint(UidPrefix::User, &[n; 16])
    }

    fn project_uid(n: u8) -> PrefixedUid {
        PrefixedUid::mint(UidPrefix::Project, &[n; 16])
    }

    fn sample_user(uid: PrefixedUid, email: &str) -> CloudUser {
        CloudUser {
            uid,
            google_sub: "g-1".to_string(),
            email: email.to_string(),
            display_name: "Sample".to_string(),
            created_at: 1.0,
        }
    }

    fn sample_project(uid: PrefixedUid, owner: PrefixedUid) -> CloudProject {
        CloudProject {
            uid,
            owner,
            visibility: Visibility::Private,
            slug: "sample".to_string(),
            created_at: 1.0,
        }
    }

    fn event(at: f64) -> HistoryEvent {
        HistoryEvent {
            at,
            kind: EventKind::Created,
        }
    }
}
