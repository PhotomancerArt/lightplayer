//! Walking the directories on flash (the working tree), with the delta laid
//! over them: one file by path (a collided path-table row, or a write
//! confirming a row), and every file under a prefix (listings, deletes).

use alloc::string::String;
use alloc::vec::Vec;

use crate::dir_node::EntryKind;
use crate::flash::Flash;
use crate::heap_sort::heap_sort_by;
use crate::node_read::read_dir;
use crate::object_hasher::ObjectHasher;
use crate::object_id::ObjectId;
use crate::tree_delta::{Change, FileEntry};
use crate::tree_store::{Res, TreeStore, is_hot};

impl<F: Flash, H: ObjectHasher> TreeStore<F, H> {
    /// The file at `path` by walking.
    pub(crate) fn walk_file(&mut self, path: &str) -> Res<Option<FileEntry>, F> {
        if let Some(r) = self.delta.lookup(path) {
            return Ok(r);
        }
        if is_hot(path) {
            let entries = read_dir(&mut self.log, self.work.hot)?;
            return Ok(entries
                .into_iter()
                .find(|e| e.kind == EntryKind::File && e.name == path)
                .map(|e| FileEntry {
                    id: e.id,
                    size: e.size,
                }));
        }
        let mut dir = self.work.cold;
        let mut parts = path[1..].split('/').peekable();
        while let Some(name) = parts.next() {
            let last = parts.peek().is_none();
            let want = if last { EntryKind::File } else { EntryKind::Dir };
            let entries = read_dir(&mut self.log, dir)?;
            let Some(e) = entries
                .into_iter()
                .find(|e| e.kind == want && e.name == name)
            else {
                return Ok(None);
            };
            if last {
                return Ok(Some(FileEntry {
                    id: e.id,
                    size: e.size,
                }));
            }
            dir = e.id;
        }
        Ok(None)
    }

    /// Every file path starting with `prefix`, sorted (the caller's list).
    pub(crate) fn list_files(&mut self, prefix: &str) -> Res<Vec<String>, F> {
        let mut out = Vec::new();
        let mut path = String::new();
        self.list_cold(self.work.cold, &mut path, prefix, 0, &mut out)?;
        for e in read_dir(&mut self.log, self.work.hot)? {
            if e.name.starts_with(prefix) {
                out.push(e.name);
            }
        }
        // The delta over it: deleted paths out, written paths in.
        let delta = &self.delta;
        out.retain(|p| delta.lookup(p).is_none_or(|r| r.is_some()));
        for e in delta.entries() {
            if matches!(e.change, Change::Set(_)) && e.path.starts_with(prefix) {
                out.push(e.path.clone());
            }
        }
        heap_sort_by(&mut out, |a, b| a.as_bytes() < b.as_bytes());
        out.dedup();
        Ok(out)
    }

    fn list_cold(
        &mut self,
        id: ObjectId,
        path: &mut String,
        prefix: &str,
        depth: usize,
        out: &mut Vec<String>,
    ) -> Res<(), F> {
        for e in read_dir(&mut self.log, id)? {
            let len = path.len();
            path.push('/');
            path.push_str(&e.name);
            match e.kind {
                EntryKind::File if path.starts_with(prefix) => out.push(path.clone()),
                EntryKind::Dir if depth < crate::tree_store::MAX_DEPTH && may_hold(path, prefix) => {
                    self.list_cold(e.id, path, prefix, depth + 1, out)?;
                }
                _ => {}
            }
            path.truncate(len);
        }
        Ok(())
    }
}

/// Whether directory `dir` can hold a path starting with `prefix`.
fn may_hold(dir: &str, prefix: &str) -> bool {
    // `dir/` starts with `prefix`, or `prefix` starts with `dir/`.
    let d = dir.as_bytes();
    let p = prefix.as_bytes();
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
        assert!(may_hold("/projects", "/"));
        assert!(may_hold("/projects", "/proj"));
        assert!(may_hold("/projects", "/projects/a/"));
        assert!(may_hold("/projects", "/projects"));
        assert!(!may_hold("/projects", "/projectsX"));
        assert!(!may_hold("/hardware", "/projects/"));
    }
}
