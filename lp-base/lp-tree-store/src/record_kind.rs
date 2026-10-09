//! Record types and chunk codecs, as their on-flash bytes (FORMAT.md).

/// What a record holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RecordKind {
    /// A leaf: a whole small file, or one chunk of a bigger node.
    Blob,
    /// A multi-part node: ordered child ids (chunks, or lower Multi levels).
    Multi,
    /// A directory that fits one record: sorted entries.
    Dir,
    /// The commit anchor.
    Root,
}

impl RecordKind {
    pub fn to_u8(self) -> u8 {
        match self {
            RecordKind::Blob => 1,
            RecordKind::Multi => 2,
            RecordKind::Dir => 3,
            RecordKind::Root => 4,
        }
    }

    /// `5` was the prototype's dictionary and is never written again.
    pub fn from_u8(b: u8) -> Option<Self> {
        Some(match b {
            1 => RecordKind::Blob,
            2 => RecordKind::Multi,
            3 => RecordKind::Dir,
            4 => RecordKind::Root,
            _ => return None,
        })
    }
}

/// How a `Blob` record's payload is coded (every other kind is `Stored`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ChunkCodec {
    /// Payload = the bytes.
    Stored,
    /// Payload = logical length (u16 LE) ++ raw deflate (RFC 1951), as the
    /// host sent it.
    Deflate,
}

impl ChunkCodec {
    pub fn to_u8(self) -> u8 {
        match self {
            ChunkCodec::Stored => 0,
            ChunkCodec::Deflate => 1,
        }
    }

    /// `2` was the prototype's deflate-with-dictionary and is never written.
    pub fn from_u8(b: u8) -> Option<Self> {
        Some(match b {
            0 => ChunkCodec::Stored,
            1 => ChunkCodec::Deflate,
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
        ] {
            assert_eq!(RecordKind::from_u8(k.to_u8()), Some(k));
        }
        assert_eq!(RecordKind::from_u8(5), None);
        assert_eq!(RecordKind::from_u8(0xFF), None);
        for c in [ChunkCodec::Stored, ChunkCodec::Deflate] {
            assert_eq!(ChunkCodec::from_u8(c.to_u8()), Some(c));
        }
        assert_eq!(ChunkCodec::from_u8(2), None);
    }
}
