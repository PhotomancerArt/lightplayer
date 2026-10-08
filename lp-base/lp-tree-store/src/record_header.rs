//! The record header (FORMAT.md "Record").
//!
//! Layout (16 bytes, little-endian): kind u8 | codec u8 | payload length u16 |
//! id u64 | CRC-32 over the first 12 header bytes and the payload. A header
//! of all `0xFF` is the end of a sector's records; anything else that fails
//! to parse or to check *closes* the sector (nothing after it is read).

use lp_crc32::Crc32;

use crate::object_id::ObjectId;
use crate::record_kind::{ChunkCodec, RecordKind};

pub const RECORD_HEADER_LEN: u32 = 16;

/// A parsed record header (CRC not yet checked against the payload).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RecordHeader {
    pub kind: RecordKind,
    pub codec: ChunkCodec,
    pub len: u16,
    pub id: ObjectId,
    pub crc: u32,
}

/// What 16 header bytes say.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HeaderRead {
    /// All `0xFF`: no record here.
    End,
    /// Not a record: the sector is closed at this offset.
    Bad,
    Record(RecordHeader),
}

impl RecordHeader {
    pub fn parse(b: &[u8; RECORD_HEADER_LEN as usize]) -> HeaderRead {
        if b.iter().all(|&x| x == 0xFF) {
            return HeaderRead::End;
        }
        let (Some(kind), Some(codec)) = (RecordKind::from_u8(b[0]), ChunkCodec::from_u8(b[1]))
        else {
            return HeaderRead::Bad;
        };
        if kind != RecordKind::Blob && codec != ChunkCodec::Stored {
            return HeaderRead::Bad;
        }
        let mut id = [0u8; 8];
        id.copy_from_slice(&b[4..12]);
        let id = ObjectId(u64::from_le_bytes(id));
        if id.is_none() {
            return HeaderRead::Bad;
        }
        HeaderRead::Record(RecordHeader {
            kind,
            codec,
            len: u16::from_le_bytes([b[2], b[3]]),
            id,
            crc: u32::from_le_bytes([b[12], b[13], b[14], b[15]]),
        })
    }

    /// Header + payload bytes.
    pub fn total_len(&self) -> u32 {
        RECORD_HEADER_LEN + u32::from(self.len)
    }

    /// Whether `payload` belongs to the header bytes `raw`.
    pub fn crc_ok(raw: &[u8; RECORD_HEADER_LEN as usize], payload: &[u8]) -> bool {
        let mut c = Crc32::new();
        c.update(&raw[..12]);
        c.update(payload);
        c.finish() == u32::from_le_bytes([raw[12], raw[13], raw[14], raw[15]])
    }
}

/// The header bytes of a record whose payload is the concatenation of
/// `parts` (at most `u16::MAX` bytes; callers keep it under `record_max`).
pub fn encode_header(
    kind: RecordKind,
    codec: ChunkCodec,
    id: ObjectId,
    parts: &[&[u8]],
) -> [u8; RECORD_HEADER_LEN as usize] {
    let len: usize = parts.iter().map(|p| p.len()).sum();
    debug_assert!(len <= u16::MAX as usize);
    let mut h = [0u8; RECORD_HEADER_LEN as usize];
    h[0] = kind.to_u8();
    h[1] = codec.to_u8();
    h[2..4].copy_from_slice(&(len as u16).to_le_bytes());
    h[4..12].copy_from_slice(&id.0.to_le_bytes());
    let mut c = Crc32::new();
    c.update(&h[..12]);
    for p in parts {
        c.update(p);
    }
    h[12..16].copy_from_slice(&c.finish().to_le_bytes());
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_crc() {
        let h = encode_header(
            RecordKind::Blob,
            ChunkCodec::Deflate,
            ObjectId(42),
            &[b"hel", b"lo"],
        );
        let HeaderRead::Record(r) = RecordHeader::parse(&h) else {
            panic!("not a record")
        };
        assert_eq!(
            (r.kind, r.codec, r.len, r.id),
            (RecordKind::Blob, ChunkCodec::Deflate, 5, ObjectId(42))
        );
        assert!(RecordHeader::crc_ok(&h, b"hello"));
        assert!(!RecordHeader::crc_ok(&h, b"hellp"));
        assert_eq!(RecordHeader::parse(&[0xFF; 16]), HeaderRead::End);
        assert_eq!(RecordHeader::parse(&[0; 16]), HeaderRead::Bad);
        let dir = encode_header(RecordKind::Dir, ChunkCodec::Deflate, ObjectId(1), &[]);
        assert_eq!(RecordHeader::parse(&dir), HeaderRead::Bad, "only blobs code");
    }
}
