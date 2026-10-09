//! Record types and chunk codecs, as their on-flash bytes.

/// What a record holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RecordKind {
    /// A leaf chunk (a whole small file, or one chunk of a bigger node).
    Blob,
    /// A multi-part node: ordered child ids (chunks, or lower Multi levels).
    Multi,
    /// A directory: sorted entries name → (kind, size, id).
    Dir,
    /// The commit anchor: seq, cold dir, hot dir, dictionary.
    Root,
    /// The store-local deflate dictionary, when it fits one record (else the
    /// dictionary node is a `Multi` of stored chunks).
    Dict,
}

impl RecordKind {
    pub fn to_u8(self) -> u8 {
        match self {
            RecordKind::Blob => 1,
            RecordKind::Multi => 2,
            RecordKind::Dir => 3,
            RecordKind::Root => 4,
            RecordKind::Dict => 5,
        }
    }

    pub fn from_u8(b: u8) -> Option<Self> {
        Some(match b {
            1 => RecordKind::Blob,
            2 => RecordKind::Multi,
            3 => RecordKind::Dir,
            4 => RecordKind::Root,
            5 => RecordKind::Dict,
            _ => return None,
        })
    }
}

/// How a `Blob` record's payload is coded (every other kind is `Stored`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ChunkCodec {
    /// Payload = the bytes.
    Stored,
    /// Payload = logical length (u16 LE) ++ raw deflate.
    Deflate,
    /// Payload = logical length (u16 LE) ++ dictionary id (u64 LE) ++ raw
    /// deflate against that dictionary.
    DeflateDict,
}

impl ChunkCodec {
    pub fn to_u8(self) -> u8 {
        match self {
            ChunkCodec::Stored => 0,
            ChunkCodec::Deflate => 1,
            ChunkCodec::DeflateDict => 2,
        }
    }

    pub fn from_u8(b: u8) -> Option<Self> {
        Some(match b {
            0 => ChunkCodec::Stored,
            1 => ChunkCodec::Deflate,
            2 => ChunkCodec::DeflateDict,
            _ => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_round_trip() {
        for k in [
            RecordKind::Blob,
            RecordKind::Multi,
            RecordKind::Dir,
            RecordKind::Root,
            RecordKind::Dict,
        ] {
            assert_eq!(RecordKind::from_u8(k.to_u8()), Some(k));
        }
        assert_eq!(RecordKind::from_u8(0xFF), None);
        for c in [
            ChunkCodec::Stored,
            ChunkCodec::Deflate,
            ChunkCodec::DeflateDict,
        ] {
            assert_eq!(ChunkCodec::from_u8(c.to_u8()), Some(c));
        }
    }
}
