//! Content identity: 64 bits of SHA-256 over a node's logical content.
//!
//! Not cryptography (spike U9): no byte-compare on dedup, no defence against
//! crafted collisions. A one-byte tag keeps a file, a directory, a chunk and
//! a dictionary with equal bytes apart.

use sha2::{Digest, Sha256};

/// A record's id. `0` is reserved as "none".
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ObjectId(pub u64);

/// What a hashed byte string is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum IdTag {
    File = 1,
    Dir = 2,
    Chunk = 3,
    MultiInner = 4,
    Dict = 5,
    Root = 6,
}

impl ObjectId {
    pub const NONE: ObjectId = ObjectId(0);

    pub fn of(tag: IdTag, bytes: &[u8]) -> Self {
        let mut h = Sha256::new();
        h.update([tag as u8]);
        h.update(bytes);
        let d = h.finalize();
        let mut b = [0u8; 8];
        b.copy_from_slice(&d[..8]);
        match u64::from_be_bytes(b) {
            0 => ObjectId(1),
            v => ObjectId(v),
        }
    }

    pub fn is_none(self) -> bool {
        self.0 == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tags_separate_equal_bytes() {
        assert_ne!(
            ObjectId::of(IdTag::File, b"x"),
            ObjectId::of(IdTag::Dir, b"x")
        );
        assert_eq!(
            ObjectId::of(IdTag::File, b"x"),
            ObjectId::of(IdTag::File, b"x")
        );
        assert!(!ObjectId::of(IdTag::File, b"").is_none());
    }
}
