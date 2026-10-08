//! The root record: the commit anchor (invariant I1), and the one place the
//! retired-sector list is persisted.
//!
//! Payload (little-endian): seq u64 | cold dir id u64 | hot dir id u64 |
//! retired count u16 | retired sector u16 × count (ascending). The record's
//! id is `H(Root ++ payload)`.

use alloc::vec::Vec;

use crate::object_id::ObjectId;

pub const ROOT_FIXED_LEN: usize = 26;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RootRecord {
    pub seq: u64,
    /// The cold tree (`/` minus the hot files).
    pub cold_dir: ObjectId,
    /// The hot files (`…/.lp/panel.json`), flat, named by full path.
    pub hot_dir: ObjectId,
    /// Sectors that failed verification and are never allocated again.
    pub retired: Vec<u16>,
}

impl RootRecord {
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(ROOT_FIXED_LEN + 2 * self.retired.len());
        b.extend_from_slice(&self.seq.to_le_bytes());
        b.extend_from_slice(&self.cold_dir.0.to_le_bytes());
        b.extend_from_slice(&self.hot_dir.0.to_le_bytes());
        b.extend_from_slice(&(self.retired.len() as u16).to_le_bytes());
        for s in &self.retired {
            b.extend_from_slice(&s.to_le_bytes());
        }
        b
    }

    pub fn decode(b: &[u8]) -> Option<Self> {
        if b.len() < ROOT_FIXED_LEN {
            return None;
        }
        let u = |i: usize| {
            let mut x = [0u8; 8];
            x.copy_from_slice(&b[i..i + 8]);
            u64::from_le_bytes(x)
        };
        let n = usize::from(u16::from_le_bytes([b[24], b[25]]));
        if b.len() != ROOT_FIXED_LEN + 2 * n {
            return None;
        }
        let retired: Vec<u16> = b[ROOT_FIXED_LEN..]
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        if retired.windows(2).any(|w| w[0] >= w[1]) {
            return None;
        }
        let (cold_dir, hot_dir) = (ObjectId(u(8)), ObjectId(u(16)));
        if cold_dir.is_none() || hot_dir.is_none() {
            return None;
        }
        Some(RootRecord {
            seq: u(0),
            cold_dir,
            hot_dir,
            retired,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[test]
    fn round_trip() {
        let r = RootRecord {
            seq: 9,
            cold_dir: ObjectId(1),
            hot_dir: ObjectId(2),
            retired: vec![3, 17],
        };
        assert_eq!(RootRecord::decode(&r.encode()), Some(r.clone()));
        let mut e = r.encode();
        e.pop();
        assert_eq!(RootRecord::decode(&e), None);
        assert_eq!(RootRecord::decode(&[0; 3]), None);
        let unsorted = RootRecord {
            retired: vec![5, 5],
            ..r
        };
        assert_eq!(RootRecord::decode(&unsorted.encode()), None);
    }
}
