//! Content identity: 64 bits of SHA-256 over a one-byte tag and a record's
//! logical bytes (FORMAT.md "Ids").
//!
//! Not cryptography (spike U9): no byte-compare on dedup, no defence against
//! crafted collisions. The tag keeps a blob, a directory, a multi and a root
//! with equal bytes apart.

use crate::object_hasher::ObjectHasher;

/// A record's id. `0` is reserved as "none".
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ObjectId(pub u64);

/// What a hashed byte string is (the first byte hashed).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum IdTag {
    /// A `Blob` record: a whole small file, or one chunk of a node.
    Blob = 1,
    /// A one-record directory: its `Dir` payload.
    Dir = 2,
    /// A `Multi` record: its payload (child ids), so a multi's id is a
    /// Merkle hash of its children.
    Multi = 3,
    /// A `Root` record: its payload.
    Root = 4,
    // 5 was the RAM path table's path hash (never written; FORMAT.md "Ids").
}

impl ObjectId {
    pub const NONE: ObjectId = ObjectId(0);

    /// `tag ++ parts…`, first 8 digest bytes big-endian; a 0 becomes 1.
    pub fn of<H: ObjectHasher>(h: &mut H, tag: IdTag, parts: &[&[u8]]) -> Self {
        match hash64(h, tag, parts) {
            0 => ObjectId(1),
            v => ObjectId(v),
        }
    }

    pub fn is_none(self) -> bool {
        self.0 == 0
    }
}

fn hash64<H: ObjectHasher>(h: &mut H, tag: IdTag, parts: &[&[u8]]) -> u64 {
    let t = [tag as u8];
    let mut all: [&[u8]; 4] = [&[]; 4];
    all[0] = &t;
    for (slot, p) in all[1..].iter_mut().zip(parts) {
        *slot = p;
    }
    debug_assert!(parts.len() <= 3);
    let d = h.sha256(&all[..=parts.len().min(3)]);
    let mut b = [0u8; 8];
    b.copy_from_slice(&d[..8]);
    u64::from_be_bytes(b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SoftSha256;

    #[test]
    fn tags_separate_equal_bytes_and_parts_concatenate() {
        let h = &mut SoftSha256;
        assert_ne!(
            ObjectId::of(h, IdTag::Blob, &[b"x"]),
            ObjectId::of(h, IdTag::Dir, &[b"x"])
        );
        assert_eq!(
            ObjectId::of(h, IdTag::Blob, &[b"ab", b"c"]),
            ObjectId::of(h, IdTag::Blob, &[b"abc"])
        );
        assert!(!ObjectId::of(h, IdTag::Blob, &[]).is_none());
        // The documented vector (FORMAT.md): SHA-256(01 "hello")[..8], BE.
        assert_eq!(
            ObjectId::of(h, IdTag::Blob, &[b"hello"]).0,
            u64::from_be_bytes(h.sha256(&[&[1], b"hello"])[..8].try_into().unwrap())
        );
    }
}
