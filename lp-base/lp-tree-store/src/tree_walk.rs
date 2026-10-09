//! Walking the directories on flash (the working tree), with the delta laid
//! over them: one file by path (every lookup, and a write that needs the
//! file it replaces), and every file under a prefix (listings). Paths are
//! handled as bytes.

use alloc::string::String;
use alloc::vec::Vec;

use crate::dir_node::{DirEntry, EntryKind};
use crate::flash::Flash;
use crate::heap_sort::sort_strings;
use crate::node_read::read_dir;
use crate::object_hasher::ObjectHasher;
use crate::object_id::ObjectId;
use crate::store_error::StoreError;
use crate::tree_delta::{Change, FileEntry};
use crate::tree_store::{Res, TreeStore, is_hot};

impl<F: Flash, H: ObjectHasher> TreeStore<F, H> {
    /// The file at `path` (a valid path) by walking.
    pub(crate) fn walk_file(&mut self, path: &str) -> Res<Option<FileEntry>, F> {
        if let Some(r) = self.delta.lookup(path) {
            return Ok(r);
        }
        let file = |e: DirEntry| FileEntry {
            id: e.id,
            size: e.size,
        };
        if is_hot(path) {
            let entries = read_dir(&mut self.log, self.work.hot)?;
            return Ok(find(entries, path.as_bytes(), EntryKind::File).map(file));
        }
        let mut dir = self.work.cold;
        let mut rest = &path.as_bytes()[1..];
        loop {
            let slash = rest.iter().position(|&c| c == b'/');
            let name = &rest[..slash.unwrap_or(rest.len())];
            let want = if slash.is_some() {
                EntryKind::Dir
            } else {
                EntryKind::File
            };
            let entries = read_dir(&mut self.log, dir)?;
            let Some(e) = find(entries, name, want) else {
                return Ok(None);
            };
            let Some(k) = slash else {
                return Ok(Some(file(e)));
            };
            dir = e.id;
            rest = &rest[k + 1..];
        }
    }

    /// Every file path starting with `prefix`, sorted (the caller's list).
    pub(crate) fn list_files(&mut self, prefix: &str) -> Res<Vec<String>, F> {
        let prefix = prefix.as_bytes();
        let mut out: Vec<Vec<u8>> = Vec::new();
        let mut path = Vec::new();
        self.list_cold(self.work.cold, &mut path, prefix, 0, &mut out)?;
        for e in read_dir(&mut self.log, self.work.hot)? {
            if e.name.starts_with(prefix) {
                out.push(e.name);
            }
        }
        // Names leave the store as strings here (UTF-8 is a writer's rule);
        // the delta over them: deleted paths out, written paths in.
        let mut names = Vec::with_capacity(out.len());
        for p in out {
            let p = String::from_utf8(p).map_err(|_| StoreError::Corrupt("dir entry name"))?;
            if self.delta.lookup(&p).is_none_or(|r| r.is_some()) {
                names.push(p);
            }
        }
        for e in self.delta.entries() {
            if matches!(e.change, Change::Set(_)) && e.path.as_bytes().starts_with(prefix) {
                names.push(e.path.clone());
            }
        }
        sort_strings(&mut names);
        names.dedup();
        Ok(names)
    }

    fn list_cold(
        &mut self,
        id: ObjectId,
        path: &mut Vec<u8>,
        prefix: &[u8],
        depth: usize,
        out: &mut Vec<Vec<u8>>,
    ) -> Res<(), F> {
        for e in read_dir(&mut self.log, id)? {
            let len = path.len();
            path.push(b'/');
            path.extend_from_slice(&e.name);
            match e.kind {
                EntryKind::File if path.starts_with(prefix) => out.push(path.clone()),
                EntryKind::Dir
                    if depth < crate::tree_store::MAX_DEPTH && may_hold(path, prefix) =>
                {
                    self.list_cold(e.id, path, prefix, depth + 1, out)?;
                }
                _ => {}
            }
            path.truncate(len);
        }
        Ok(())
    }
}

/// The entry named `name` of `kind`, taken out of `entries`.
fn find(entries: Vec<DirEntry>, name: &[u8], kind: EntryKind) -> Option<DirEntry> {
    entries
        .into_iter()
        .find(|e| e.kind == kind && e.name == name)
}

/// Whether directory `dir` can hold a path starting with `prefix`.
fn may_hold(d: &[u8], p: &[u8]) -> bool {
    // `dir/` starts with `prefix`, or `prefix` starts with `dir/`.
    let n = p.len().min(d.len());
    if d[..n] != p[..n] {
        return false;
    }
    p.len() <= d.len() || p[d.len()] == b'/'
}

#[cfg(test)]
mod tests {
    use super::may_hold;

    #[test]
    fn prefix_pruning() {
        assert!(may_hold(b"/projects", b"/"));
        assert!(may_hold(b"/projects", b"/proj"));
        assert!(may_hold(b"/projects", b"/projects/a/"));
        assert!(may_hold(b"/projects", b"/projects"));
        assert!(!may_hold(b"/projects", b"/projectsX"));
        assert!(!may_hold(b"/hardware", b"/projects/"));
    }
}
