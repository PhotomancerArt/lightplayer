//! A multi-part node: anything over `record_max` (Yona's rule) is an ordered
//! list of children that each fit. Level 0 children are `Blob` chunks; level
//! L > 0 children are level L−1 `Multi` records.
//!
//! Payload (little-endian): level u8 | total logical length u32 | count u16 |
//! child ids u64 × count.

use alloc::vec::Vec;

use crate::object_id::ObjectId;

pub const MULTI_HEADER_LEN: usize = 7;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MultiNode {
    pub level: u8,
    pub total_len: u32,
    pub children: Vec<ObjectId>,
}

impl MultiNode {
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(MULTI_HEADER_LEN + 8 * self.children.len());
        b.push(self.level);
        b.extend_from_slice(&self.total_len.to_le_bytes());
        b.extend_from_slice(&(self.children.len() as u16).to_le_bytes());
        for c in &self.children {
            b.extend_from_slice(&c.0.to_le_bytes());
        }
        b
    }

    pub fn decode(b: &[u8]) -> Option<Self> {
        if b.len() < MULTI_HEADER_LEN {
            return None;
        }
        let count = usize::from(u16::from_le_bytes([b[5], b[6]]));
        if b.len() != MULTI_HEADER_LEN + 8 * count || count == 0 {
            return None;
        }
        let children = b[MULTI_HEADER_LEN..]
            .chunks_exact(8)
            .map(|c| {
                let mut x = [0u8; 8];
                x.copy_from_slice(c);
                ObjectId(u64::from_le_bytes(x))
            })
            .collect();
        Some(MultiNode {
            level: b[0],
            total_len: u32::from_le_bytes([b[1], b[2], b[3], b[4]]),
            children,
        })
    }

    /// Children that fit one record of `max_payload` payload bytes.
    pub fn fanout(max_payload: usize) -> usize {
        (max_payload.saturating_sub(MULTI_HEADER_LEN) / 8).min(u16::MAX as usize)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[test]
    fn round_trip() {
        let m = MultiNode {
            level: 1,
            total_len: 9000,
            children: vec![ObjectId(1), ObjectId(2)],
        };
        assert_eq!(MultiNode::decode(&m.encode()), Some(m));
        assert_eq!(MultiNode::decode(&[0; 7]), None);
        assert_eq!(MultiNode::fanout(1008), 125);
    }
}
