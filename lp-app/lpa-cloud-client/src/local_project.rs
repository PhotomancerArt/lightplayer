//! One project on this machine: its working copy and its history root.

use alloc::vec::Vec;

use lpc_history::hash::is_hashed_path;
use lpc_history::{
    BlobStore, ContentHash, EventLog, HistoryEvent, PrefixedUid, ProjectHistory, SnapshotStore,
};
use lpfs::{FsError, LpFs, LpPath};

use crate::cloud_binding::CloudBinding;
use crate::sync_error::SyncError;

/// A local project, as the sync engine needs to see it: a uid, a working
/// copy, and a history root.
///
/// Two filesystems, because they are two different things. The **package**
/// is what the user edits and what gets content-hashed. The **history root**
/// is `lpc-history`'s own area — `blobs/`, `trees/`, `events.jsonl`, and this
/// crate's `cloud-binding.json`. Neither knows about the other's layout; a
/// caller wires them up (the studio over OPFS, a test over
/// `lpfs::LpFsMemory`).
///
/// This type is a thin composition over the `lpc-history` primitives, not a
/// second model of a project: it exists so the sync operations take one
/// argument instead of four, and so the two or three multi-primitive moves
/// they all need ([`save`](Self::save), [`bank_working_copy`](Self::bank_working_copy),
/// [`checkout`](Self::checkout)) are written once, in the open.
pub struct LocalProject<'a> {
    uid: PrefixedUid,
    package_fs: &'a dyn LpFs,
    history_fs: &'a dyn LpFs,
}

impl<'a> LocalProject<'a> {
    /// View an existing local project.
    pub fn new(uid: PrefixedUid, package_fs: &'a dyn LpFs, history_fs: &'a dyn LpFs) -> Self {
        Self {
            uid,
            package_fs,
            history_fs,
        }
    }

    /// Start a project's history with an origin event.
    ///
    /// Refuses a history root that already has a log — starting a second
    /// history over an existing one is how a project loses its past.
    pub fn init(
        uid: PrefixedUid,
        package_fs: &'a dyn LpFs,
        history_fs: &'a dyn LpFs,
        origin: HistoryEvent,
    ) -> Result<Self, SyncError> {
        let project = Self::new(uid, package_fs, history_fs);
        if !project.event_log().read_all()?.is_empty() {
            return Err(SyncError::LocalHistoryExists);
        }
        // Validate before writing: `ProjectHistory` owns what an origin is.
        ProjectHistory::new(origin.clone())?;
        project.event_log().append(&origin)?;
        Ok(project)
    }

    /// Start a new project from a version of an existing one.
    ///
    /// A fork is **a new project**: a new uid, a fresh history whose origin
    /// points at the parent, and no cloud binding — which is the whole point
    /// when a tracking copy diverges and its owner wants to keep their own
    /// line without fighting somebody else's. The parent is untouched, its
    /// binding included.
    ///
    /// The forked version becomes the new project's v1, content and all.
    pub fn fork_from(
        parent: &LocalProject<'_>,
        parent_version: ContentHash,
        uid: PrefixedUid,
        package_fs: &'a dyn LpFs,
        history_fs: &'a dyn LpFs,
        at: f64,
    ) -> Result<Self, SyncError> {
        let parent_history = parent.history()?;
        let forked = ProjectHistory::fork_from(&parent_history, parent.uid(), parent_version, at)?;
        let origin = forked
            .events()
            .first()
            .cloned()
            .ok_or(SyncError::NoLocalHistory)?;
        let fork = Self::init(uid, package_fs, history_fs, origin)?;
        parent
            .snapshots()
            .materialize(&parent_version, package_fs)?;
        let banked = fork.bank_working_copy()?;
        if banked != parent_version {
            return Err(SyncError::ContentMismatch {
                expected: parent_version,
                actual: banked,
            });
        }
        Ok(fork)
    }

    /// The project's uid — the same uid the cloud knows it by (D17).
    pub fn uid(&self) -> PrefixedUid {
        self.uid
    }

    /// The working copy.
    pub fn package_fs(&self) -> &'a dyn LpFs {
        self.package_fs
    }

    /// The history root.
    pub fn history_fs(&self) -> &'a dyn LpFs {
        self.history_fs
    }

    /// Content-addressed snapshots (blobs + trees) in the history root.
    pub fn snapshots(&self) -> SnapshotStore<'a> {
        SnapshotStore::new(self.history_fs)
    }

    /// File blobs in the history root.
    pub fn blobs(&self) -> BlobStore<'a> {
        BlobStore::new(self.history_fs)
    }

    /// The persistent event log in the history root.
    pub fn event_log(&self) -> EventLog<'a> {
        EventLog::new(self.history_fs)
    }

    /// Replay the project's history.
    pub fn history(&self) -> Result<ProjectHistory, SyncError> {
        let events = self.event_log().read_all()?;
        if events.is_empty() {
            return Err(SyncError::NoLocalHistory);
        }
        Ok(ProjectHistory::from_events(events)?)
    }

    /// The current version, if the project has one yet.
    pub fn head(&self) -> Result<Option<ContentHash>, SyncError> {
        Ok(self.history()?.head())
    }

    /// Snapshot the working copy without recording anything.
    ///
    /// Idempotent and content-addressed, so calling it is never destructive
    /// and never surprising. Every operation that is about to overwrite the
    /// working copy calls this first: that is the banking-before-adopt rule,
    /// and it is why an "uncommitted edits" refusal can name the hash the
    /// content is recoverable at.
    pub fn bank_working_copy(&self) -> Result<ContentHash, SyncError> {
        let (hash, _) = self.snapshots().put_package(self.package_fs)?;
        Ok(hash)
    }

    /// Snapshot the working copy and record it as the new head.
    ///
    /// A save whose content equals the current head records nothing —
    /// re-saving unchanged content is not an event.
    pub fn save(&self, at: f64) -> Result<ContentHash, SyncError> {
        let hash = self.bank_working_copy()?;
        let mut history = self.history()?;
        if history.head() == Some(hash) {
            return Ok(hash);
        }
        let event = history.record_save(hash, at);
        self.event_log().append(&event)?;
        Ok(hash)
    }

    /// Replace the working copy with a banked version.
    ///
    /// Unlike [`SnapshotStore::materialize`], this removes package files the
    /// target version does not have — a checkout that only ever wrote files
    /// would leave deleted ones behind and produce a working copy that
    /// hashes to neither version. `/.lp/**` is left alone: it is outside the
    /// canonical hash by specification.
    ///
    /// Callers are expected to have banked whatever the working copy holds
    /// first; this method will not do it for them, because the decision of
    /// what to do about uncommitted edits belongs to the operation, not to
    /// the file copy.
    pub fn checkout(&self, version: ContentHash) -> Result<(), SyncError> {
        let snapshots = self.snapshots();
        if !snapshots.has(&version)? {
            return Err(SyncError::UnbankedVersion(version));
        }
        let manifest = snapshots.get_tree(&version)?;
        let existing = match self.package_fs.list_dir(LpPath::new("/"), true) {
            Ok(paths) => paths,
            Err(FsError::NotFound(_)) => Vec::new(),
            Err(e) => return Err(e.into()),
        };
        for path in existing {
            if !is_hashed_path(&path) || self.package_fs.is_dir(&path)? {
                continue;
            }
            if manifest.entries().iter().any(|entry| entry.path == path) {
                continue;
            }
            self.package_fs.delete_file(&path)?;
        }
        snapshots.materialize(&version, self.package_fs)?;
        Ok(())
    }

    /// The project's cloud binding, if it has one.
    pub fn binding(&self) -> Result<Option<CloudBinding>, SyncError> {
        CloudBinding::load(self.history_fs)
    }

    /// The project's cloud binding, or [`SyncError::NotBound`].
    pub fn require_binding(&self) -> Result<CloudBinding, SyncError> {
        self.binding()?.ok_or(SyncError::NotBound)
    }

    /// Store the project's cloud binding.
    pub fn put_binding(&self, binding: &CloudBinding) -> Result<(), SyncError> {
        binding.save(self.history_fs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{TestProject, created};
    use lpc_history::SyncRelation;

    #[test]
    fn init_then_save_records_a_version() {
        let project = TestProject::new(1);
        let local = project.init();
        project.write("/project.json", b"{}");

        let v1 = local.save(2.0).unwrap();
        assert_eq!(local.head().unwrap(), Some(v1));
        assert_eq!(local.history().unwrap().events().len(), 2);
    }

    #[test]
    fn saving_unchanged_content_records_nothing() {
        let project = TestProject::new(1);
        let local = project.init();
        project.write("/project.json", b"{}");

        let v1 = local.save(2.0).unwrap();
        let again = local.save(3.0).unwrap();
        assert_eq!(v1, again);
        assert_eq!(local.history().unwrap().events().len(), 2);
    }

    #[test]
    fn a_second_init_is_refused() {
        let project = TestProject::new(1);
        project.init();
        let again = LocalProject::init(
            project.uid(),
            project.package(),
            project.history(),
            created(1.0),
        );
        assert!(matches!(again, Err(SyncError::LocalHistoryExists)));
    }

    /// A fork keeps its parent's content and drops its parent's cloud
    /// binding: new project, new uid, no tracking.
    #[test]
    fn a_fork_is_a_new_project_at_the_parents_version() {
        let parent_slot = TestProject::new(1);
        let parent = parent_slot.init();
        parent_slot.write("/project.json", b"{}");
        let v1 = parent.save(2.0).unwrap();
        parent
            .put_binding(&crate::CloudBinding::new(parent_slot.uid()))
            .unwrap();

        let fork_slot = TestProject::new(2);
        let fork = LocalProject::fork_from(
            &parent,
            v1,
            fork_slot.uid(),
            fork_slot.package(),
            fork_slot.history(),
            3.0,
        )
        .unwrap();

        assert_ne!(fork.uid(), parent.uid());
        assert_eq!(fork.head().unwrap(), Some(v1));
        assert_eq!(fork_slot.read("/project.json"), Some(b"{}".to_vec()));
        assert!(fork.binding().unwrap().is_none());
        // the parent is untouched, binding included
        assert_eq!(parent.head().unwrap(), Some(v1));
        assert!(parent.binding().unwrap().is_some());
    }

    #[test]
    fn history_of_an_untouched_root_is_missing_not_empty() {
        let project = TestProject::new(1);
        let local = project.local();
        assert!(matches!(local.history(), Err(SyncError::NoLocalHistory)));
    }

    /// The whole point of `checkout` over a bare materialize: a file the
    /// target version does not have must not survive the switch.
    #[test]
    fn checkout_removes_files_the_target_version_does_not_have() {
        let project = TestProject::new(1);
        let local = project.init();
        project.write("/project.json", b"{}");
        project.write("/shader.glsl", b"v1");
        let v1 = local.save(2.0).unwrap();

        project.delete("/shader.glsl");
        project.write("/project.json", b"{\"n\":2}");
        let v2 = local.save(3.0).unwrap();

        local.checkout(v1).unwrap();
        assert_eq!(project.read("/shader.glsl"), Some(b"v1".to_vec()));
        assert_eq!(local.bank_working_copy().unwrap(), v1);

        local.checkout(v2).unwrap();
        assert_eq!(project.read("/shader.glsl"), None);
        assert_eq!(local.bank_working_copy().unwrap(), v2);
    }

    #[test]
    fn checkout_refuses_an_unbanked_version() {
        let project = TestProject::new(1);
        let local = project.init();
        project.write("/project.json", b"{}");
        local.save(2.0).unwrap();

        let stranger = ContentHash::of(b"never stored");
        assert!(matches!(
            local.checkout(stranger),
            Err(SyncError::UnbankedVersion(_))
        ));
    }

    /// Banking is what makes uncommitted work recoverable, so it has to be
    /// true that the banked hash is a real, restorable version.
    #[test]
    fn banking_the_working_copy_makes_it_restorable() {
        let project = TestProject::new(1);
        let local = project.init();
        project.write("/project.json", b"{}");
        let v1 = local.save(2.0).unwrap();

        project.write("/project.json", b"{\"dirty\":true}");
        let dirty = local.bank_working_copy().unwrap();
        assert_ne!(dirty, v1);
        // the history never heard about it...
        assert_eq!(local.head().unwrap(), Some(v1));
        assert_eq!(
            local.history().unwrap().classify(dirty),
            SyncRelation::Diverged
        );
        // ...but the content is there, by hash
        local.checkout(v1).unwrap();
        local.checkout(dirty).unwrap();
        assert_eq!(
            project.read("/project.json"),
            Some(b"{\"dirty\":true}".to_vec())
        );
    }
}
