//! The path view: absolute file paths over a cold directory tree and a flat
//! hot directory, and the RAM map the store keeps of them.

use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::mem::size_of;

use crate::dir_node::{DirEntry, EntryKind};
use crate::object_id::ObjectId;

/// A committed file: its node id and logical size.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FileEntry {
    pub id: ObjectId,
    pub size: u32,
}

/// A path's state between commits.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WorkEntry {
    Committed(FileEntry),
    /// Put since the last commit: the bytes, held in RAM until commit.
    Staged(Vec<u8>),
}

/// The hot path: last two components are `.lp/panel.json`.
pub fn is_hot(path: &str) -> bool {
    path.ends_with("/.lp/panel.json")
}

/// Absolute, no trailing `/`, no empty component.
pub fn valid_path(path: &str) -> bool {
    path.len() > 1
        && path.starts_with('/')
        && !path.ends_with('/')
        && !path[1..].split('/').any(str::is_empty)
}

/// RAM for a committed map: path bytes + entry sizes.
pub fn tree_ram_bytes(map: &BTreeMap<String, WorkEntry>) -> usize {
    map.keys()
        .map(|k| k.len() + size_of::<String>() + size_of::<FileEntry>())
        .sum()
}

/// Build the cold tree from `files` — `(path relative to this directory,
/// entry)`, sorted by path — post-order: each directory's entries go to
/// `emit`, which returns its id. Paths sharing a `name/` prefix are
/// contiguous in sorted order, so each subdirectory is one run of the slice.
pub fn build_dirs<E>(
    files: &[(&str, FileEntry)],
    emit: &mut impl FnMut(Vec<DirEntry>) -> Result<ObjectId, E>,
) -> Result<ObjectId, E> {
    let mut entries = Vec::new();
    let mut i = 0;
    while i < files.len() {
        let (rel, fe) = files[i];
        let Some((head, _)) = rel.split_once('/') else {
            entries.push(DirEntry {
                name: rel.to_string(),
                kind: EntryKind::File,
                size: fe.size,
                id: fe.id,
            });
            i += 1;
            continue;
        };
        let mut sub = Vec::new();
        while i < files.len()
            && let Some(rest) = files[i]
                .0
                .strip_prefix(head)
                .and_then(|r| r.strip_prefix('/'))
        {
            sub.push((rest, files[i].1));
            i += 1;
        }
        let id = build_dirs(&sub, emit)?;
        entries.push(DirEntry {
            name: head.to_string(),
            kind: EntryKind::Dir,
            size: 0,
            id,
        });
    }
    emit(entries)
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[test]
    fn paths_and_hot() {
        assert!(valid_path("/a/b.json"));
        for p in ["", "/", "a", "/a/", "//a", "/a//b"] {
            assert!(!valid_path(p), "{p}");
        }
        assert!(is_hot("/projects/a/.lp/panel.json"));
        assert!(!is_hot("/projects/a/.lp/access.json"));
    }

    #[test]
    fn builds_nested_dirs() {
        let f = FileEntry {
            id: ObjectId(7),
            size: 1,
        };
        // Sorted as strings: "a-b" sorts between "a" and "a/".
        let files = [("a-b", f), ("a/x", f), ("a/y/z", f), ("top", f)];
        let mut seen = vec![];
        let root = build_dirs::<()>(&files, &mut |e| {
            seen.push(e.len());
            Ok(ObjectId(100 + seen.len() as u64))
        })
        .unwrap();
        // y (1), a (x + y = 2), root (a-b, a, top = 3).
        assert_eq!(seen, vec![1, 2, 3]);
        assert_eq!(root, ObjectId(103));
    }
}
