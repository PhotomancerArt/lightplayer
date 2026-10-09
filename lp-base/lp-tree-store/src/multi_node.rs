//! A multi-part node: anything over one record (Yona's rule) is an ordered
//! list of children that each fit. Level 0 children are `Blob` chunks;
//! level L > 0 children are level L−1 `Multi` records.
//!
//! Payload (little-endian): flags+level u8 (bit 7 = the node's bytes are a
//! directory; bits 0–6 = level) | total logical length u32 | count u16 |
//! child ids u64 × count. Its id is `H(Multi ++ payload)`: a Merkle hash of
//! its children, so an append rewrites only the right spine and a full
//! inner multi keeps its id.

use alloc::vec::Vec;

use crate::object_id::ObjectId;

pub const MULTI_HEADER_LEN: usize = 7;
const DIR_FLAG: u8 = 0x80;

/// A multi payload's fixed fields; children are read with [`multi_child`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MultiHead {
    pub level: u8,
    /// The node's bytes are a directory (FORMAT.md "Dir").
    pub dir: bool,
    pub total_len: u32,
    pub count: usize,
}

/// Parse (never panics): `None` unless the length matches a count ≥ 1.
pub fn parse_multi(b: &[u8]) -> Option<MultiHead> {
    if b.len() < MULTI_HEADER_LEN {
        return None;
    }
    let count = usize::from(u16::from_le_bytes([b[5], b[6]]));
    if b.len() != MULTI_HEADER_LEN + 8 * count || count == 0 {
        return None;
    }
    Some(MultiHead {
        level: b[0] & !DIR_FLAG,
        dir: b[0] & DIR_FLAG != 0,
        total_len: u32::from_le_bytes([b[1], b[2], b[3], b[4]]),
        count,
    })
}

/// Child `i` of a parsed payload.
pub fn multi_child(b: &[u8], i: usize) -> ObjectId {
    let o = MULTI_HEADER_LEN + 8 * i;
    let mut x = [0u8; 8];
    x.copy_from_slice(&b[o..o + 8]);
    ObjectId(u64::from_le_bytes(x))
}

pub fn encode_multi(level: u8, dir: bool, total_len: u32, children: &[ObjectId]) -> Vec<u8> {
    let mut b = Vec::with_capacity(MULTI_HEADER_LEN + 8 * children.len());
    b.push(level | if dir { DIR_FLAG } else { 0 });
    b.extend_from_slice(&total_len.to_le_bytes());
    b.extend_from_slice(&(children.len() as u16).to_le_bytes());
    for c in children {
        b.extend_from_slice(&c.0.to_le_bytes());
    }
    b
}

/// Children that fit one record of `max_payload` payload bytes.
pub fn multi_fanout(max_payload: usize) -> usize {
    (max_payload.saturating_sub(MULTI_HEADER_LEN) / 8).clamp(2, u16::MAX as usize)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let p = encode_multi(1, true, 9000, &[ObjectId(1), ObjectId(2)]);
        let h = parse_multi(&p).unwrap();
        assert_eq!(
            h,
            MultiHead {
                level: 1,
                dir: true,
                total_len: 9000,
                count: 2
            }
        );
        assert_eq!(multi_child(&p, 1), ObjectId(2));
        assert_eq!(parse_multi(&[0; 7]), None);
        assert_eq!(parse_multi(&p[..p.len() - 1]), None);
        assert_eq!(multi_fanout(1008), 125);
    }
}
