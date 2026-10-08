//! The tree changes not yet written as directories: what a transaction (or
//! one per-call write) holds in RAM between writing its content records and
//! path-copying its directories. Bounded by `StoreConfig::txn_delta_max`:
//! past it, the store writes the delta's directories as pending records
//! (`dir_rebuild.rs`) and starts an empty one.

use alloc::string::String;
use alloc::vec::Vec;
use core::mem::size_of;

use crate::object_id::ObjectId;

/// A committed file: its node id and logical size.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FileEntry {
    pub id: ObjectId,
    pub size: u32,
}

/// One change at one path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Change {
    Set(FileEntry),
    Delete,
    /// The directory at this path and everything under it are gone (later
    /// changes under it start from empty).
    DeleteTree,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeltaEntry {
    pub path: String,
    pub change: Change,
}

impl DeltaEntry {
    fn key(&self) -> (&[u8], bool) {
        (self.path.as_bytes(), self.change == Change::DeleteTree)
    }
}

/// Sorted by (path bytes, is-tree): a file and a directory may share a path.
#[derive(Default)]
pub struct TreeDelta {
    entries: Vec<DeltaEntry>,
}

impl TreeDelta {
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn entries(&self) -> &[DeltaEntry] {
        &self.entries
    }

    pub fn clear(&mut self) {
        self.entries = Vec::new();
    }

    pub fn take(&mut self) -> Vec<DeltaEntry> {
        core::mem::take(&mut self.entries)
    }

    pub fn restore(&mut self, entries: Vec<DeltaEntry>) {
        self.entries = entries;
    }

    fn find(&self, path: &str, tree: bool) -> Result<usize, usize> {
        let key = (path.as_bytes(), tree);
        self.entries.binary_search_by(|e| e.key().cmp(&key))
    }

    fn upsert(&mut self, path: &str, change: Change) {
        let tree = change == Change::DeleteTree;
        match self.find(path, tree) {
            Ok(i) => self.entries[i].change = change,
            Err(i) => self.entries.insert(
                i,
                DeltaEntry {
                    path: String::from(path),
                    change,
                },
            ),
        }
    }

    pub fn set(&mut self, path: &str, fe: FileEntry) {
        self.upsert(path, Change::Set(fe));
    }

    pub fn delete(&mut self, path: &str) {
        self.upsert(path, Change::Delete);
    }

    /// `dir` and everything under it: earlier changes under it are dropped.
    pub fn delete_tree(&mut self, dir: &str) {
        self.entries.retain(|e| !under(&e.path, dir));
        self.upsert(dir, Change::DeleteTree);
    }

    /// What the delta says about the file at `path`: `Some(Some)` written,
    /// `Some(None)` deleted (or under a deleted tree), `None` untouched.
    pub fn lookup(&self, path: &str) -> Option<Option<FileEntry>> {
        if let Ok(i) = self.find(path, false) {
            return Some(match self.entries[i].change {
                Change::Set(fe) => Some(fe),
                _ => None,
            });
        }
        self.covered(path).then_some(None)
    }

    /// Whether a deleted tree covers `path`.
    pub fn covered(&self, path: &str) -> bool {
        let mut p = path;
        while let Some(i) = p.rfind('/') {
            p = &p[..i];
            if !p.is_empty() && self.find(p, true).is_ok() {
                return true;
            }
        }
        false
    }

    /// The node ids the delta names (pending: GC must keep them).
    pub fn set_ids(&self) -> impl Iterator<Item = ObjectId> + '_ {
        self.entries.iter().filter_map(|e| match e.change {
            Change::Set(fe) => Some(fe.id),
            _ => None,
        })
    }

    /// What the allocator holds for it.
    pub fn ram_bytes(&self) -> usize {
        self.entries.capacity() * size_of::<DeltaEntry>()
            + self.entries.iter().map(|e| e.path.capacity()).sum::<usize>()
    }
}

/// `path` is strictly inside directory `dir`.
pub fn under(path: &str, dir: &str) -> bool {
    path.len() > dir.len() + 1 && path.starts_with(dir) && path.as_bytes()[dir.len()] == b'/'
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fe(id: u64) -> FileEntry {
        FileEntry {
            id: ObjectId(id),
            size: 1,
        }
    }

    #[test]
    fn set_delete_tree_and_lookup() {
        let mut d = TreeDelta::default();
        d.set("/a/x", fe(1));
        d.set("/a/y", fe(2));
        d.set("/ab", fe(3));
        d.delete_tree("/a");
        assert_eq!(d.lookup("/a/x"), Some(None));
        assert_eq!(d.lookup("/ab"), Some(Some(fe(3))));
        d.set("/a/x", fe(4));
        assert_eq!(d.lookup("/a/x"), Some(Some(fe(4))));
        assert_eq!(d.lookup("/a/z/w"), Some(None));
        assert_eq!(d.lookup("/b"), None);
        d.set("/a", fe(5));
        assert_eq!(d.entries().len(), 4, "file /a and tree /a are two entries");
        assert_eq!(d.set_ids().count(), 3);
        assert!(under("/a/b", "/a") && !under("/ab", "/a") && !under("/a", "/a"));
    }
}
