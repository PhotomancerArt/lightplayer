//! The root record: the commit anchor (invariant I1).
//!
//! Payload (36 bytes, little-endian): seq u64 | cold dir id u64 | hot dir id
//! u64 | dictionary id u64 (0 = none) | next key id u32 (JSON-tree mode;
//! always 0 here). The record's id is the hash of the payload.

use alloc::vec::Vec;

use crate::object_id::{IdTag, ObjectId};

pub const ROOT_PAYLOAD_LEN: usize = 36;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RootRecord {
    pub seq: u64,
    /// The cold tree (`/` minus the hot files).
    pub cold_dir: ObjectId,
    /// The hot files (`…/.lp/panel.json`), flat, named by full path.
    pub hot_dir: ObjectId,
    /// The dictionary new chunks are coded against.
    pub dict: ObjectId,
    pub next_key_id: u32,
}

impl RootRecord {
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(ROOT_PAYLOAD_LEN);
        b.extend_from_slice(&self.seq.to_le_bytes());
        b.extend_from_slice(&self.cold_dir.0.to_le_bytes());
        b.extend_from_slice(&self.hot_dir.0.to_le_bytes());
        b.extend_from_slice(&self.dict.0.to_le_bytes());
        b.extend_from_slice(&self.next_key_id.to_le_bytes());
        b
    }

    pub fn decode(b: &[u8]) -> Option<Self> {
        if b.len() != ROOT_PAYLOAD_LEN {
            return None;
        }
        let u = |i: usize| {
            let mut x = [0u8; 8];
            x.copy_from_slice(&b[i..i + 8]);
            u64::from_le_bytes(x)
        };
        Some(RootRecord {
            seq: u(0),
            cold_dir: ObjectId(u(8)),
            hot_dir: ObjectId(u(16)),
            dict: ObjectId(u(24)),
            next_key_id: u32::from_le_bytes([b[32], b[33], b[34], b[35]]),
        })
    }

    pub fn id_of(payload: &[u8]) -> ObjectId {
        ObjectId::of(IdTag::Root, payload)
    }

    /// Everything the root names.
    pub fn refs(&self) -> [ObjectId; 3] {
        [self.cold_dir, self.hot_dir, self.dict]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let r = RootRecord {
            seq: 9,
            cold_dir: ObjectId(1),
            hot_dir: ObjectId(2),
            dict: ObjectId::NONE,
            next_key_id: 0,
        };
        assert_eq!(RootRecord::decode(&r.encode()), Some(r));
        assert_eq!(RootRecord::decode(&[0; 3]), None);
    }
}
