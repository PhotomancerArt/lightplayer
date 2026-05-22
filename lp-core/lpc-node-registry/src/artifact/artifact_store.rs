//! Refcounted freshness-only artifact cache.

use alloc::collections::BTreeMap;
use alloc::vec::Vec;

use lpc_model::{ArtifactLocator, Revision};
use lpfs::{FsEvent, FsEventKind, LpFs};

use super::{
    ArtifactEntry, ArtifactError, ArtifactId, ArtifactLocation, ArtifactReadFailure,
    ArtifactReadState,
};

/// Cache of held file artifacts keyed by opaque handle and resolved location.
///
/// Entries exist only while requesters hold refs ([`Self::acquire_location`] /
/// [`Self::release`]). Filesystem changes invalidate held entries; they do not
/// register new ones.
pub struct ArtifactStore {
    by_handle: BTreeMap<u32, ArtifactEntry>,
    location_to_handle: BTreeMap<ArtifactLocation, u32>,
    next_handle: u32,
}

impl ArtifactStore {
    pub fn new() -> Self {
        Self {
            by_handle: BTreeMap::new(),
            location_to_handle: BTreeMap::new(),
            next_handle: 1,
        }
    }

    pub fn acquire_location(&mut self, location: ArtifactLocation, frame: Revision) -> ArtifactId {
        if let Some(&handle) = self.location_to_handle.get(&location) {
            if let Some(entry) = self.by_handle.get_mut(&handle) {
                entry.refcount += 1;
                return entry.id;
            }
            self.location_to_handle.remove(&location);
        }

        let handle = self.alloc_handle();
        let id = ArtifactId::from_raw(handle);
        self.location_to_handle.insert(location.clone(), handle);
        self.by_handle.insert(
            handle,
            ArtifactEntry {
                id,
                location,
                refcount: 1,
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
        Ok(self.acquire_location(location, frame))
    }

    pub fn release(&mut self, id: &ArtifactId, _frame: Revision) -> Result<(), ArtifactError> {
        let handle = id.handle();
        let entry = self
            .by_handle
            .get_mut(&handle)
            .ok_or(ArtifactError::UnknownHandle { handle })?;
        if entry.refcount == 0 {
            return Err(ArtifactError::InvalidRelease { handle });
        }
        entry.refcount -= 1;
        if entry.refcount != 0 {
            return Ok(());
        }
        let location = entry.location.clone();
        self.by_handle.remove(&handle);
        self.location_to_handle.remove(&location);
        Ok(())
    }

    pub fn apply_fs_changes(&mut self, changes: &[FsEvent], frame: Revision) {
        for change in changes {
            self.apply_fs_change(change, frame);
        }
    }

    pub fn read_bytes(&mut self, id: &ArtifactId, fs: &dyn LpFs) -> Result<Vec<u8>, ArtifactError> {
        let handle = id.handle();
        let path = {
            let entry = self
                .entry(id)
                .ok_or(ArtifactError::UnknownHandle { handle })?;
            entry
                .location
                .file_path()
                .cloned()
                .ok_or_else(|| ArtifactError::internal("expected file artifact location"))?
        };

        match fs.read_file(path.as_path()) {
            Ok(bytes) => {
                if let Some(entry) = self.by_handle.get_mut(&handle) {
                    entry.read_state = ArtifactReadState::ReadOk;
                }
                Ok(bytes)
            }
            Err(err) => {
                let failure = ArtifactReadFailure::from_fs_error(err);
                if let Some(entry) = self.by_handle.get_mut(&handle) {
                    entry.read_state = ArtifactReadState::Failed(failure.clone());
                }
                Err(ArtifactError::Read(failure))
            }
        }
    }

    pub fn revision(&self, id: &ArtifactId) -> Option<Revision> {
        self.entry(id).map(|entry| entry.revision)
    }

    pub fn refcount(&self, id: &ArtifactId) -> Option<u32> {
        self.entry(id).map(|entry| entry.refcount)
    }

    pub fn entry(&self, id: &ArtifactId) -> Option<&ArtifactEntry> {
        self.by_handle.get(&id.handle())
    }
}

impl Default for ArtifactStore {
    fn default() -> Self {
        Self::new()
    }
}

impl ArtifactStore {
    fn alloc_handle(&mut self) -> u32 {
        let handle = self.next_handle;
        self.next_handle = self.next_handle.wrapping_add(1);
        if self.next_handle == 0 {
            self.next_handle = 1;
        }
        handle
    }

    fn apply_fs_change(&mut self, change: &FsEvent, frame: Revision) {
        for entry in self.by_handle.values_mut() {
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
    use lpfs::{FsEvent, FsEventKind, LpFsMemory, LpPathBuf};

    fn file_location(path: &str) -> ArtifactLocation {
        ArtifactLocation::file(path)
    }

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
    fn acquire_same_location_reuses_handle_and_increments_refcount() {
        let mut store = ArtifactStore::new();
        let location = file_location("/shader.glsl");
        let id1 = store.acquire_location(location.clone(), Revision::new(1));
        let id2 = store.acquire_location(location, Revision::new(2));
        assert_eq!(id1, id2);
        assert_eq!(store.refcount(&id1), Some(2));
    }

    #[test]
    fn release_at_zero_removes_entry() {
        let mut store = ArtifactStore::new();
        let id = store.acquire_location(file_location("/a.toml"), Revision::new(1));
        store.release(&id, Revision::new(1)).unwrap();
        assert!(store.entry(&id).is_none());
    }

    #[test]
    fn fs_modify_bumps_revision_and_sets_unread() {
        let mut store = ArtifactStore::new();
        let id = store.acquire_location(file_location("/b.glsl"), Revision::new(1));
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
    fn fs_change_on_unacquired_path_is_noop() {
        let mut store = ArtifactStore::new();
        store.apply_fs_changes(
            &[fs_change("/missing.glsl", FsEventKind::Modify)],
            Revision::new(9),
        );
        let id = store.acquire_location(file_location("/missing.glsl"), Revision::new(2));
        assert_eq!(store.revision(&id), Some(Revision::new(2)));
        assert_eq!(
            store.entry(&id).unwrap().read_state,
            ArtifactReadState::Unread
        );
    }

    #[test]
    fn fs_delete_sets_deleted_failure_while_entry_held() {
        let mut store = ArtifactStore::new();
        let id = store.acquire_location(file_location("/c.svg"), Revision::new(1));
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
        let id = store.acquire_location(file_location("/after.toml"), Revision::new(1));
        assert_eq!(id.handle(), 1);
    }

    #[test]
    fn read_bytes_success_sets_read_ok() {
        let mut fs = LpFsMemory::new();
        fs.write_file_mut(project_path("shader.glsl").as_path(), b"void main() {}")
            .unwrap();

        let mut store = ArtifactStore::new();
        let id = store.acquire_location(file_location("/shader.glsl"), Revision::new(1));
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
        let id = store.acquire_location(file_location("/nope.glsl"), Revision::new(1));
        let err = store.read_bytes(&id, &fs).unwrap_err();
        assert!(matches!(
            err,
            ArtifactError::Read(ArtifactReadFailure::NotFound)
        ));
        assert_eq!(
            store.entry(&id).unwrap().read_state,
            ArtifactReadState::Failed(ArtifactReadFailure::NotFound)
        );
        assert_eq!(store.refcount(&id), Some(1));
    }

    #[test]
    fn read_after_fs_modify_gets_new_content() {
        let mut fs = LpFsMemory::new();
        fs.write_file_mut(project_path("x.glsl").as_path(), b"v1")
            .unwrap();

        let mut store = ArtifactStore::new();
        let id = store.acquire_location(file_location("/x.glsl"), Revision::new(1));
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
