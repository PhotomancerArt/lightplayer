//! A file and a directory never share a name (FORMAT.md "Files and
//! paths"). The format can hold both — directory entries sort by (name,
//! kind) — but no host filesystem, archive or littlefs can, so a writer
//! that made one would leave every tool that reads the store (extract,
//! Studio's backup, a desktop folder) a pair it cannot represent, forever.
//! `LpFsTree` asks this before it creates a file.

use alloc::string::String;

use crate::flash::Flash;
use crate::object_hasher::ObjectHasher;
use crate::tree_store::{Res, TreeStore};

impl<F: Flash, H: ObjectHasher> TreeStore<F, H> {
    /// Whether a new file at `path` (a valid path) would share a name with
    /// a directory: one of its ancestors is a file, or a file lies under
    /// `path` itself. Reads the working tree with a transaction's changes
    /// laid over it, as every lookup does.
    pub fn name_clash(&mut self, path: &str) -> Res<bool, F> {
        if !crate::tree_store::valid_path(path) {
            return Ok(false);
        }
        for (i, _) in path.match_indices('/').skip(1) {
            if self.walk_file(&path[..i])?.is_some() {
                return Ok(true);
            }
        }
        let mut under = String::from(path);
        under.push('/');
        Ok(!self.list_files(&under)?.is_empty())
    }
}
