//! A directory node's bytes: sorted entries `name → (kind, size, id)`.
//!
//! Layout (little-endian): count u16, then per entry kind u8 (1 file, 2 dir)
//! | name length u16 | name (UTF-8) | logical size u32 | id u64.
//!
//! Names are bytes here: UTF-8 is a writer's rule (every name comes from a
//! `&str` path), and a reader checks it only where a name leaves the store
//! as a `String` (`list`). Entries are
//! sorted by (name bytes, kind). A directory too big for one record is a
//! `Multi` with the dir flag over stored `Blob` chunks of these bytes.

use alloc::vec::Vec;

use crate::heap_sort::heap_sort_by;
use crate::object_id::ObjectId;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum EntryKind {
    File,
    Dir,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DirEntry {
    pub name: Vec<u8>,
    pub kind: EntryKind,
    /// Logical bytes for a file; 0 for a directory.
    pub size: u32,
    pub id: ObjectId,
}

/// Encode entries (sorted here).
pub fn encode_dir(entries: &mut [DirEntry]) -> Vec<u8> {
    heap_sort_by(entries, |a, b| {
        (&a.name[..], a.kind) < (&b.name[..], b.kind)
    });
    let len = 2 + entries.iter().map(|e| 15 + e.name.len()).sum::<usize>();
    let mut out = Vec::with_capacity(len);
    out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    for e in entries.iter() {
        out.push(match e.kind {
            EntryKind::File => 1,
            EntryKind::Dir => 2,
        });
        out.extend_from_slice(&(e.name.len() as u16).to_le_bytes());
        out.extend_from_slice(&e.name);
        out.extend_from_slice(&e.size.to_le_bytes());
        out.extend_from_slice(&e.id.0.to_le_bytes());
    }
    out
}

/// Decode, or `None` for anything malformed (never panics).
pub fn decode_dir(b: &[u8]) -> Option<Vec<DirEntry>> {
    let mut p = 0usize;
    let take = |p: &mut usize, n: usize| -> Option<&[u8]> {
        let s = b.get(*p..p.checked_add(n)?)?;
        *p += n;
        Some(s)
    };
    let c = take(&mut p, 2)?;
    let count = u16::from_le_bytes([c[0], c[1]]);
    let mut out = Vec::with_capacity(usize::from(count).min(b.len() / 15));
    for _ in 0..count {
        let kind = match take(&mut p, 1)?[0] {
            1 => EntryKind::File,
            2 => EntryKind::Dir,
            _ => return None,
        };
        let l = take(&mut p, 2)?;
        let name_len = usize::from(u16::from_le_bytes([l[0], l[1]]));
        let name = take(&mut p, name_len)?.to_vec();
        let s = take(&mut p, 4)?;
        let size = u32::from_le_bytes([s[0], s[1], s[2], s[3]]);
        let mut id = [0u8; 8];
        id.copy_from_slice(take(&mut p, 8)?);
        let id = ObjectId(u64::from_le_bytes(id));
        if id.is_none() {
            return None;
        }
        out.push(DirEntry {
            name,
            kind,
            size,
            id,
        });
    }
    (p == b.len()).then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[test]
    fn round_trip_sorted() {
        let mut e = vec![
            DirEntry {
                name: b"b".to_vec(),
                kind: EntryKind::File,
                size: 3,
                id: ObjectId(5),
            },
            DirEntry {
                name: b"a".to_vec(),
                kind: EntryKind::Dir,
                size: 0,
                id: ObjectId(6),
            },
        ];
        let bytes = encode_dir(&mut e);
        let back = decode_dir(&bytes).unwrap();
        assert_eq!(back[0].name, b"a");
        assert_eq!(back, e);
        assert_eq!(decode_dir(&bytes[..bytes.len() - 1]), None);
        assert_eq!(decode_dir(&[]), None);
        assert_eq!(decode_dir(&[0, 0]), Some(vec![]));
        // Names are bytes: one that is not UTF-8 decodes (the writer's rule;
        // `list` refuses it where it would become a String).
        let mut odd = vec![DirEntry {
            name: vec![0xFF, b'x'],
            kind: EntryKind::File,
            size: 1,
            id: ObjectId(7),
        }];
        assert_eq!(decode_dir(&encode_dir(&mut odd)), Some(odd));
    }
}
