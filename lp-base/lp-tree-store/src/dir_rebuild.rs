//! Flushing the delta: path-copy every directory a change touches, bottom
//! up, into new `Dir` records (pending until a root names them), and the
//! hot directory. Unchanged directories keep their ids (content addressing:
//! re-encoding the same entries is the same record, and is not written
//! again). An emptied directory disappears from its parent (directories are
//! implicit); the root's cold directory always exists.

use alloc::string::String;
use alloc::vec::Vec;

use crate::dir_node::{DirEntry, EntryKind, encode_dir};
use crate::flash::Flash;
use crate::gc_mark::MarkRole;
use crate::node_read::read_dir;
use crate::object_hasher::ObjectHasher;
use crate::object_id::ObjectId;
use crate::sector_header::HeadKind;
use crate::tree_delta::{Change, DeltaEntry, under};
use crate::tree_store::{Res, TreeStore, WorkDirs, is_hot};

impl<F: Flash, H: ObjectHasher> TreeStore<F, H> {
    /// Write the delta's directories as pending records and adopt them as
    /// the working tree; the delta is empty after. On an error the delta and
    /// the working tree are as they were.
    ///
    /// Each directory makes room for exactly its own records before it is
    /// written (`write_dir_bytes`), so GC may run between two directories:
    /// the delta's file ids and every directory written so far are
    /// *in flight* and marked live meanwhile.
    pub(crate) fn flush(&mut self) -> Res<(), F> {
        if self.delta.is_empty() {
            return Ok(());
        }
        let changes = self.delta.take();
        self.inflight = changes
            .iter()
            .filter_map(|c| match c.change {
                Change::Set(fe) => Some((fe.id, MarkRole::Node)),
                _ => None,
            })
            .collect();
        let r = self.rebuild_all(&changes);
        self.inflight = Vec::new();
        match r {
            Ok(work) => {
                self.work = work;
                Ok(())
            }
            Err(e) => {
                self.delta.restore(changes);
                Err(e)
            }
        }
    }

    fn rebuild_all(&mut self, changes: &[DeltaEntry]) -> Res<WorkDirs, F> {
        let cold = match self.rebuild_dir(Some(self.work.cold), "", changes, false)? {
            Some(id) => id,
            None => self.write_dir_bytes(HeadKind::Cold, &[0, 0])?,
        };
        let hot = self.rebuild_hot(changes)?;
        Ok(WorkDirs { cold, hot })
    }

    /// Directory `dir` (`""` = the root) after `changes` (all under it,
    /// sorted). `reset` = it was deleted earlier in the delta: start empty.
    fn rebuild_dir(
        &mut self,
        old: Option<ObjectId>,
        dir: &str,
        changes: &[DeltaEntry],
        reset: bool,
    ) -> Res<Option<ObjectId>, F> {
        let mut entries = match (reset, old) {
            (false, Some(id)) => read_dir(&mut self.log, id)?,
            _ => Vec::new(),
        };
        let base = dir.len() + 1;
        let mut i = 0;
        while i < changes.len() {
            let e = &changes[i];
            let rel = &e.path[base..];
            let Some(k) = rel.find('/') else {
                match e.change {
                    Change::Set(fe) if !is_hot(&e.path) => {
                        remove(&mut entries, rel, EntryKind::File);
                        entries.push(DirEntry {
                            name: String::from(rel),
                            kind: EntryKind::File,
                            size: fe.size,
                            id: fe.id,
                        });
                    }
                    Change::Delete if !is_hot(&e.path) => {
                        remove(&mut entries, rel, EntryKind::File)
                    }
                    Change::DeleteTree => remove(&mut entries, rel, EntryKind::Dir),
                    _ => {}
                }
                i += 1;
                continue;
            };
            let name = &rel[..k];
            let sub_dir = &e.path[..base + k];
            let mut j = i;
            while j < changes.len() && under(&changes[j].path, sub_dir) {
                j += 1;
            }
            let child_reset = reset
                || changes[..i]
                    .iter()
                    .any(|c| c.path == sub_dir && c.change == Change::DeleteTree);
            let old_child = if child_reset {
                None
            } else {
                entries
                    .iter()
                    .find(|d| d.kind == EntryKind::Dir && d.name == name)
                    .map(|d| d.id)
            };
            let new_child = self.rebuild_dir(old_child, sub_dir, &changes[i..j], child_reset)?;
            remove(&mut entries, name, EntryKind::Dir);
            if let Some(id) = new_child {
                entries.push(DirEntry {
                    name: String::from(name),
                    kind: EntryKind::Dir,
                    size: 0,
                    id,
                });
            }
            i = j;
        }
        if entries.is_empty() && !dir.is_empty() {
            return Ok(None);
        }
        let bytes = encode_dir(&mut entries);
        self.log
            .note(bytes.capacity() + entries.capacity() * core::mem::size_of::<DirEntry>());
        Ok(Some(self.write_dir_bytes(HeadKind::Cold, &bytes)?))
    }

    /// The hot directory after `changes` (unchanged = not rewritten).
    fn rebuild_hot(&mut self, changes: &[DeltaEntry]) -> Res<ObjectId, F> {
        let trees: Vec<&str> = changes
            .iter()
            .filter(|c| c.change == Change::DeleteTree)
            .map(|c| c.path.as_str())
            .collect();
        let mut entries = read_dir(&mut self.log, self.work.hot)?;
        let before = entries.len();
        entries.retain(|e| !trees.iter().any(|t| under(&e.name, t)));
        let mut changed = entries.len() != before;
        for c in changes.iter().filter(|c| is_hot(&c.path)) {
            changed = true;
            remove(&mut entries, &c.path, EntryKind::File);
            if let Change::Set(fe) = c.change {
                entries.push(DirEntry {
                    name: c.path.clone(),
                    kind: EntryKind::File,
                    size: fe.size,
                    id: fe.id,
                });
            }
        }
        if !changed {
            return Ok(self.work.hot);
        }
        let bytes = encode_dir(&mut entries);
        self.log
            .note(bytes.capacity() + entries.capacity() * core::mem::size_of::<DirEntry>());
        self.write_dir_bytes(HeadKind::Hot, &bytes)
    }
}

fn remove(entries: &mut Vec<DirEntry>, name: &str, kind: EntryKind) {
    entries.retain(|e| !(e.kind == kind && e.name == name));
}
