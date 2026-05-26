//! Project artifact catalog: stable ids, freshness, transient reads.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

use lpc_model::{ArtifactLocator, Revision};
use lpfs::{FsEvent, FsEventKind, LpFs, LpPath, LpPathBuf};

use super::{
    ArtifactEntry, ArtifactError, ArtifactId, ArtifactLocation, ArtifactReadFailure,
    ArtifactReadState,
};

/// Catalog of project file artifacts keyed by stable [`ArtifactId`] and path.
///
/// An artifact remains registered until [`Self::unregister`]. Registration is
/// idempotent: [`Self::register_file`] returns the same id for the same path.
/// Filesystem changes invalidate read state on registered entries; they do not
/// register new paths.
pub struct ArtifactStore {
    by_id: BTreeMap<ArtifactId, ArtifactEntry>,
    path_to_id: BTreeMap<String, ArtifactId>,
    next_id: u32,
}

impl ArtifactStore {
    pub fn new() -> Self {
        Self {
            by_id: BTreeMap::new(),
            path_to_id: BTreeMap::new(),
            next_id: 1,
        }
    }

    /// Register `path` in the project catalog, or return the existing id.
    pub fn register_file(&mut self, path: LpPathBuf, frame: Revision) -> ArtifactId {
        if let Some(&id) = self.path_to_id.get(path.as_str()) {
            return id;
        }

        let id = self.alloc_id();
        let location = ArtifactLocation::file(path.clone());
        self.path_to_id.insert(String::from(path.as_str()), id);
        self.by_id.insert(
            id,
            ArtifactEntry {
                id,
                location,
                revision: frame,
                read_state: ArtifactReadState::Unread,
            },
        );
        id
    }

    pub fn acquire_locator(
        &mut self,
        locator: &ArtifactLocator,
        frame: Revision,
    ) -> Result<ArtifactId, ArtifactError> {
        let location = ArtifactLocation::try_from_locator(locator)?;
        let path = location
            .file_path()
            .cloned()
            .ok_or_else(|| ArtifactError::internal("expected file artifact location"))?;
        Ok(self.register_file(path, frame))
    }

    /// Drop a registered artifact when nothing in the project references it.
    pub fn unregister(&mut self, id: &ArtifactId) -> Result<(), ArtifactError> {
        let entry = self
            .by_id
            .remove(id)
            .ok_or(ArtifactError::UnknownArtifact { id: *id })?;
        if let Some(path) = entry.location.file_path() {
            self.path_to_id.remove(path.as_str());
        }
        Ok(())
    }

    pub fn id_for_path(&self, path: &LpPath) -> Option<ArtifactId> {
        self.path_to_id.get(path.as_str()).copied()
    }

    pub fn path_for_id(&self, id: ArtifactId) -> Option<&LpPathBuf> {
        self.by_id
            .get(&id)
            .and_then(|entry| entry.location.file_path())
    }

    pub fn artifact_ids(&self) -> impl Iterator<Item = ArtifactId> + '_ {
        self.by_id.keys().copied()
    }

    pub fn apply_fs_changes(&mut self, changes: &[FsEvent], frame: Revision) {
        for change in changes {
            self.apply_fs_change(change, frame);
        }
    }

    pub fn read_bytes(&mut self, id: &ArtifactId, fs: &dyn LpFs) -> Result<Vec<u8>, ArtifactError> {
        let path = {
            let entry = self
                .entry(id)
                .ok_or(ArtifactError::UnknownArtifact { id: *id })?;
            entry
                .location
                .file_path()
                .cloned()
                .ok_or_else(|| ArtifactError::internal("expected file artifact location"))?
        };

        match fs.read_file(path.as_path()) {
            Ok(bytes) => {
                if let Some(entry) = self.by_id.get_mut(id) {
                    entry.read_state = ArtifactReadState::ReadOk;
                }
                Ok(bytes)
            }
            Err(err) => {
                let failure = ArtifactReadFailure::from_fs_error(err);
                if let Some(entry) = self.by_id.get_mut(id) {
                    entry.read_state = ArtifactReadState::Failed(failure.clone());
                }
                Err(ArtifactError::Read(failure))
            }
        }
    }

    pub fn revision(&self, id: &ArtifactId) -> Option<Revision> {
        self.entry(id).map(|entry| entry.revision)
    }

    pub fn entry(&self, id: &ArtifactId) -> Option<&ArtifactEntry> {
        self.by_id.get(id)
    }
}

impl Default for ArtifactStore {
    fn default() -> Self {
        Self::new()
    }
}

impl ArtifactStore {
    fn alloc_id(&mut self) -> ArtifactId {
        let raw = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        if self.next_id == 0 {
            self.next_id = 1;
        }
        ArtifactId::from_raw(raw)
    }

    fn apply_fs_change(&mut self, change: &FsEvent, frame: Revision) {
        for entry in self.by_id.values_mut() {
            let Some(path) = entry.location.file_path() else {
                continue;
            };
            if path != &change.path {
                continue;
            }
            entry.revision = frame;
            entry.read_state = match change.kind {
                FsEventKind::Delete => ArtifactReadState::Failed(ArtifactReadFailure::Deleted),
                FsEventKind::Modify | FsEventKind::Create => ArtifactReadState::Unread,
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpfs::{FsEvent, FsEventKind, LpFsMemory};

    fn fs_change(path: &str, kind: FsEventKind) -> FsEvent {
        FsEvent {
            path: LpPathBuf::from(path),
            kind,
        }
    }

    fn project_path(name: &str) -> LpPathBuf {
        LpPathBuf::from(alloc::format!("/{name}"))
    }

    #[test]
    fn register_same_path_reuses_artifact_id() {
        let mut store = ArtifactStore::new();
        let id1 = store.register_file(LpPathBuf::from("/shader.glsl"), Revision::new(1));
        let id2 = store.register_file(LpPathBuf::from("/shader.glsl"), Revision::new(2));
        assert_eq!(id1, id2);
        assert_eq!(store.artifact_ids().count(), 1);
    }

    #[test]
    fn unregister_removes_entry_and_path_lookup() {
        let mut store = ArtifactStore::new();
        let id = store.register_file(LpPathBuf::from("/a.toml"), Revision::new(1));
        store.unregister(&id).unwrap();
        assert!(store.entry(&id).is_none());
        assert!(store.id_for_path(LpPath::new("/a.toml")).is_none());
    }

    #[test]
    fn fs_modify_bumps_revision_and_sets_unread() {
        let mut store = ArtifactStore::new();
        let id = store.register_file(LpPathBuf::from("/b.glsl"), Revision::new(1));
        store.apply_fs_changes(
            &[fs_change("/b.glsl", FsEventKind::Modify)],
            Revision::new(5),
        );
        assert_eq!(store.revision(&id), Some(Revision::new(5)));
        assert_eq!(
            store.entry(&id).unwrap().read_state,
            ArtifactReadState::Unread
        );
    }

    #[test]
    fn fs_change_on_unregistered_path_is_noop() {
        let mut store = ArtifactStore::new();
        store.apply_fs_changes(
            &[fs_change("/missing.glsl", FsEventKind::Modify)],
            Revision::new(9),
        );
        let id = store.register_file(LpPathBuf::from("/missing.glsl"), Revision::new(2));
        assert_eq!(store.revision(&id), Some(Revision::new(2)));
        assert_eq!(
            store.entry(&id).unwrap().read_state,
            ArtifactReadState::Unread
        );
    }

    #[test]
    fn fs_delete_sets_deleted_failure_while_registered() {
        let mut store = ArtifactStore::new();
        let id = store.register_file(LpPathBuf::from("/c.svg"), Revision::new(1));
        store.apply_fs_changes(
            &[fs_change("/c.svg", FsEventKind::Delete)],
            Revision::new(3),
        );
        assert_eq!(store.revision(&id), Some(Revision::new(3)));
        assert_eq!(
            store.entry(&id).unwrap().read_state,
            ArtifactReadState::Failed(ArtifactReadFailure::Deleted)
        );
    }

    #[test]
    fn acquire_locator_rejects_lib() {
        let mut store = ArtifactStore::new();
        let locator = ArtifactLocator::parse("lib:core/x").unwrap();
        let err = store
            .acquire_locator(&locator, Revision::new(1))
            .unwrap_err();
        assert!(matches!(err, ArtifactError::Resolution(_)));
        let id = store.register_file(LpPathBuf::from("/after.toml"), Revision::new(1));
        assert_eq!(id.raw(), 1);
    }

    #[test]
    fn read_bytes_success_sets_read_ok() {
        let mut fs = LpFsMemory::new();
        fs.write_file_mut(project_path("shader.glsl").as_path(), b"void main() {}")
            .unwrap();

        let mut store = ArtifactStore::new();
        let id = store.register_file(LpPathBuf::from("/shader.glsl"), Revision::new(1));
        let bytes = store.read_bytes(&id, &fs).unwrap();
        assert_eq!(bytes, b"void main() {}");
        assert_eq!(
            store.entry(&id).unwrap().read_state,
            ArtifactReadState::ReadOk
        );
    }

    #[test]
    fn read_bytes_missing_file_sets_not_found() {
        let fs = LpFsMemory::new();
        let mut store = ArtifactStore::new();
        let id = store.register_file(LpPathBuf::from("/nope.glsl"), Revision::new(1));
        let err = store.read_bytes(&id, &fs).unwrap_err();
        assert!(matches!(
            err,
            ArtifactError::Read(ArtifactReadFailure::NotFound)
        ));
        assert_eq!(
            store.entry(&id).unwrap().read_state,
            ArtifactReadState::Failed(ArtifactReadFailure::NotFound)
        );
        assert!(store.entry(&id).is_some());
    }

    #[test]
    fn read_after_fs_modify_gets_new_content() {
        let mut fs = LpFsMemory::new();
        fs.write_file_mut(project_path("x.glsl").as_path(), b"v1")
            .unwrap();

        let mut store = ArtifactStore::new();
        let id = store.register_file(LpPathBuf::from("/x.glsl"), Revision::new(1));
        assert_eq!(store.read_bytes(&id, &fs).unwrap(), b"v1");

        fs.write_file_mut(project_path("x.glsl").as_path(), b"v2")
            .unwrap();
        store.apply_fs_changes(
            &[fs_change("/x.glsl", FsEventKind::Modify)],
            Revision::new(2),
        );
        assert_eq!(
            store.entry(&id).unwrap().read_state,
            ArtifactReadState::Unread
        );
        assert_eq!(store.read_bytes(&id, &fs).unwrap(), b"v2");
    }
}
