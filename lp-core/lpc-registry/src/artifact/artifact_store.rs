//! Project artifact catalog: locations, freshness, transient reads.

use alloc::collections::BTreeMap;
use alloc::vec::Vec;

use lpc_model::{ArtifactLocation, ArtifactSpec, Revision};
use lpfs::{FsEvent, FsEventKind, LpFs, LpPath, LpPathBuf};

use super::{ArtifactEntry, ArtifactError, ArtifactReadFailure, ArtifactReadState};

/// Catalog of project file artifacts keyed by [`ArtifactLocation`].
///
/// An artifact remains registered until [`Self::unregister`]. Registration is
/// idempotent: [`Self::register_file`] returns the same location for the same path.
pub struct ArtifactStore {
    by_location: BTreeMap<ArtifactLocation, ArtifactEntry>,
}

impl ArtifactStore {
    pub fn new() -> Self {
        Self {
            by_location: BTreeMap::new(),
        }
    }

    /// Register `path` in the project catalog, or return the existing location.
    pub fn register_file(&mut self, path: LpPathBuf, frame: Revision) -> ArtifactLocation {
        self.register_location(ArtifactLocation::file(path), frame)
    }

    /// Register a resolved location, or return the existing entry's location.
    pub fn register_location(
        &mut self,
        location: ArtifactLocation,
        frame: Revision,
    ) -> ArtifactLocation {
        if let Some(entry) = self.by_location.get(&location) {
            return entry.location.clone();
        }
        self.by_location.insert(
            location.clone(),
            ArtifactEntry {
                location: location.clone(),
                revision: frame,
                read_state: ArtifactReadState::Unread,
            },
        );
        location
    }

    pub fn acquire_specifier(
        &mut self,
        specifier: &ArtifactSpec,
        frame: Revision,
    ) -> Result<ArtifactLocation, ArtifactError> {
        let location = ArtifactLocation::try_from_specifier(specifier)?;
        let path = location.file_path().clone();
        Ok(self.register_file(path, frame))
    }

    /// Drop a registered artifact when nothing in the project references it.
    pub fn unregister(&mut self, location: &ArtifactLocation) -> Result<(), ArtifactError> {
        self.by_location
            .remove(location)
            .ok_or(ArtifactError::UnknownArtifact {
                location: location.clone(),
            })?;
        Ok(())
    }

    pub fn location_for_path(&self, path: &LpPath) -> Option<ArtifactLocation> {
        let location = ArtifactLocation::location_for_path(path);
        self.by_location
            .get(&location)
            .map(|entry| entry.location.clone())
    }

    pub fn locations(&self) -> impl Iterator<Item = ArtifactLocation> + '_ {
        self.by_location
            .values()
            .map(|entry| entry.location.clone())
    }

    pub fn apply_fs_changes(&mut self, changes: &[FsEvent], frame: Revision) {
        for change in changes {
            self.apply_fs_change(change, frame);
        }
    }

    pub fn read_bytes(
        &mut self,
        location: &ArtifactLocation,
        fs: &dyn LpFs,
    ) -> Result<Vec<u8>, ArtifactError> {
        let path = location.file_path().clone();

        if self.entry(location).is_none() {
            return Err(ArtifactError::UnknownArtifact {
                location: location.clone(),
            });
        }

        match fs.read_file(path.as_path()) {
            Ok(bytes) => {
                if let Some(entry) = self.by_location.get_mut(location) {
                    entry.read_state = ArtifactReadState::ReadOk;
                }
                Ok(bytes)
            }
            Err(err) => {
                let failure = ArtifactReadFailure::from_fs_error(err);
                if let Some(entry) = self.by_location.get_mut(location) {
                    entry.read_state = ArtifactReadState::Failed(failure.clone());
                }
                Err(ArtifactError::Read(failure))
            }
        }
    }

    pub fn revision(&self, location: &ArtifactLocation) -> Option<Revision> {
        self.entry(location).map(|entry| entry.revision)
    }

    pub fn entry(&self, location: &ArtifactLocation) -> Option<&ArtifactEntry> {
        self.by_location.get(location)
    }
}

impl Default for ArtifactStore {
    fn default() -> Self {
        Self::new()
    }
}

impl ArtifactStore {
    fn apply_fs_change(&mut self, change: &FsEvent, frame: Revision) {
        for entry in self.by_location.values_mut() {
            let path = entry.location.file_path();
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

    fn file_loc(path: &str) -> ArtifactLocation {
        ArtifactLocation::file(path)
    }

    #[test]
    fn register_same_path_reuses_location() {
        let mut store = ArtifactStore::new();
        let loc1 = store.register_file(LpPathBuf::from("/shader.glsl"), Revision::new(1));
        let loc2 = store.register_file(LpPathBuf::from("/shader.glsl"), Revision::new(2));
        assert_eq!(loc1, loc2);
        assert_eq!(store.locations().count(), 1);
    }

    #[test]
    fn unregister_removes_entry() {
        let mut store = ArtifactStore::new();
        let location = store.register_file(LpPathBuf::from("/a.toml"), Revision::new(1));
        store.unregister(&location).unwrap();
        assert!(store.entry(&location).is_none());
        assert!(store.location_for_path(LpPath::new("/a.toml")).is_none());
    }

    #[test]
    fn fs_modify_bumps_revision_and_sets_unread() {
        let mut store = ArtifactStore::new();
        let location = store.register_file(LpPathBuf::from("/b.glsl"), Revision::new(1));
        store.apply_fs_changes(
            &[fs_change("/b.glsl", FsEventKind::Modify)],
            Revision::new(5),
        );
        assert_eq!(store.revision(&location), Some(Revision::new(5)));
        assert_eq!(
            store.entry(&location).unwrap().read_state,
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
        let location = store.register_file(LpPathBuf::from("/missing.glsl"), Revision::new(2));
        assert_eq!(store.revision(&location), Some(Revision::new(2)));
    }

    #[test]
    fn fs_delete_sets_deleted_failure_while_registered() {
        let mut store = ArtifactStore::new();
        let location = store.register_file(LpPathBuf::from("/c.svg"), Revision::new(1));
        store.apply_fs_changes(
            &[fs_change("/c.svg", FsEventKind::Delete)],
            Revision::new(3),
        );
        assert_eq!(store.revision(&location), Some(Revision::new(3)));
        assert_eq!(
            store.entry(&location).unwrap().read_state,
            ArtifactReadState::Failed(ArtifactReadFailure::Deleted)
        );
    }

    #[test]
    fn acquire_specifier_rejects_lib() {
        let mut store = ArtifactStore::new();
        let specifier = ArtifactSpec::parse("lib:core/x").unwrap();
        let err = store
            .acquire_specifier(&specifier, Revision::new(1))
            .unwrap_err();
        assert!(matches!(err, ArtifactError::Resolution(_)));
        let location = store.register_file(LpPathBuf::from("/after.toml"), Revision::new(1));
        assert_eq!(location, file_loc("/after.toml"));
    }

    #[test]
    fn read_bytes_success_sets_read_ok() {
        let mut fs = LpFsMemory::new();
        fs.write_file_mut(project_path("shader.glsl").as_path(), b"void main() {}")
            .unwrap();

        let mut store = ArtifactStore::new();
        let location = store.register_file(LpPathBuf::from("/shader.glsl"), Revision::new(1));
        let bytes = store.read_bytes(&location, &fs).unwrap();
        assert_eq!(bytes, b"void main() {}");
        assert_eq!(
            store.entry(&location).unwrap().read_state,
            ArtifactReadState::ReadOk
        );
    }

    #[test]
    fn read_bytes_missing_file_sets_not_found() {
        let fs = LpFsMemory::new();
        let mut store = ArtifactStore::new();
        let location = store.register_file(LpPathBuf::from("/nope.glsl"), Revision::new(1));
        let err = store.read_bytes(&location, &fs).unwrap_err();
        assert!(matches!(
            err,
            ArtifactError::Read(ArtifactReadFailure::NotFound)
        ));
        assert_eq!(
            store.entry(&location).unwrap().read_state,
            ArtifactReadState::Failed(ArtifactReadFailure::NotFound)
        );
    }

    #[test]
    fn read_after_fs_modify_gets_new_content() {
        let mut fs = LpFsMemory::new();
        fs.write_file_mut(project_path("x.glsl").as_path(), b"v1")
            .unwrap();

        let mut store = ArtifactStore::new();
        let location = store.register_file(LpPathBuf::from("/x.glsl"), Revision::new(1));
        assert_eq!(store.read_bytes(&location, &fs).unwrap(), b"v1");

        fs.write_file_mut(project_path("x.glsl").as_path(), b"v2")
            .unwrap();
        store.apply_fs_changes(
            &[fs_change("/x.glsl", FsEventKind::Modify)],
            Revision::new(2),
        );
        assert_eq!(
            store.entry(&location).unwrap().read_state,
            ArtifactReadState::Unread
        );
        assert_eq!(store.read_bytes(&location, &fs).unwrap(), b"v2");
    }
}
