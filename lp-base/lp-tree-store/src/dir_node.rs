//! A directory node's bytes: sorted entries `name → (kind, size, id)`.
//!
//! Layout (little-endian): count u16, then per entry kind u8 (1 file, 2 dir)
//! | name length u16 | name (UTF-8) | logical size u32 | id u64. Entries are
//! sorted by (name bytes, kind). A directory too big for one record is a
//! `Multi` with the dir flag over stored `Blob` chunks of these bytes.

use alloc::string::String;
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
    pub name: String,
    pub kind: EntryKind,
    /// Logical bytes for a file; 0 for a directory.
    pub size: u32,
    pub id: ObjectId,
}

/// Encode entries (sorted here).
pub fn encode_dir(entries: &mut [DirEntry]) -> Vec<u8> {
    heap_sort_by(entries, |a, b| {
        (a.name.as_bytes(), a.kind) < (b.name.as_bytes(), b.kind)
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
        out.extend_from_slice(e.name.as_bytes());
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
        let name = String::from_utf8(take(&mut p, name_len)?.to_vec()).ok()?;
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
    use alloc::string::ToString;
    use alloc::vec;

    #[test]
    fn round_trip_sorted() {
        let mut e = vec![
            DirEntry {
                name: "b".to_string(),
                kind: EntryKind::File,
                size: 3,
                id: ObjectId(5),
            },
            DirEntry {
                name: "a".to_string(),
                kind: EntryKind::Dir,
                size: 0,
                id: ObjectId(6),
            },
        ];
        let bytes = encode_dir(&mut e);
        let back = decode_dir(&bytes).unwrap();
        assert_eq!(back[0].name, "a");
        assert_eq!(back, e);
        assert_eq!(decode_dir(&bytes[..bytes.len() - 1]), None);
        assert_eq!(decode_dir(&[]), None);
        assert_eq!(decode_dir(&[0, 0]), Some(vec![]));
    }
}
