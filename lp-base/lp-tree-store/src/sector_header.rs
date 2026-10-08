//! The sector header: programmed right after a completed erase, it is the
//! only thing that makes a sector trusted.
//!
//! Layout (20 bytes, little-endian): magic `"LTS1"` u32 | format version u16 |
//! head kind u8 | 0 u8 | sector seq u32 | erase count u32 | CRC-32 of the
//! first 16 bytes. Before a sector is erased its header is *killed*
//! (programmed to all zeros), so a torn erase can never leave an old, valid
//! header in front of weak bits.

use lp_crc32::crc32;

pub const SECTOR_MAGIC: u32 = 0x3153_544C; // "LTS1" read little-endian
pub const SECTOR_FORMAT_VERSION: u16 = 1;
pub const SECTOR_HEADER_LEN: u32 = 20;
/// What a header is programmed to before its sector is erased.
pub const KILLED_SECTOR_HEADER: [u8; SECTOR_HEADER_LEN as usize] = [0; SECTOR_HEADER_LEN as usize];

/// Which write head a sector was opened for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HeadKind {
    /// Pushes, directories, GC copies.
    Cold,
    /// `.lp/panel.json` files, the hot directory, roots.
    Hot,
}

impl HeadKind {
    pub const ALL: [HeadKind; 2] = [HeadKind::Cold, HeadKind::Hot];

    pub fn index(self) -> usize {
        match self {
            HeadKind::Cold => 0,
            HeadKind::Hot => 1,
        }
    }
}

/// A decoded, CRC-good sector header.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SectorHeader {
    /// Global open order: larger = opened later.
    pub seq: u32,
    pub erase_count: u32,
    pub kind: HeadKind,
}

impl SectorHeader {
    pub fn encode(&self) -> [u8; SECTOR_HEADER_LEN as usize] {
        let mut b = [0u8; SECTOR_HEADER_LEN as usize];
        b[0..4].copy_from_slice(&SECTOR_MAGIC.to_le_bytes());
        b[4..6].copy_from_slice(&SECTOR_FORMAT_VERSION.to_le_bytes());
        b[6] = self.kind.index() as u8;
        b[7] = 0;
        b[8..12].copy_from_slice(&self.seq.to_le_bytes());
        b[12..16].copy_from_slice(&self.erase_count.to_le_bytes());
        let crc = crc32(&b[..16]);
        b[16..20].copy_from_slice(&crc.to_le_bytes());
        b
    }

    pub fn decode(b: &[u8; SECTOR_HEADER_LEN as usize]) -> Option<Self> {
        let word = |i: usize| u32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]);
        if word(0) != SECTOR_MAGIC
            || u16::from_le_bytes([b[4], b[5]]) != SECTOR_FORMAT_VERSION
            || b[7] != 0
            || word(16) != crc32(&b[..16])
        {
            return None;
        }
        let kind = match b[6] {
            0 => HeadKind::Cold,
            1 => HeadKind::Hot,
            _ => return None,
        };
        Some(SectorHeader {
            seq: word(8),
            erase_count: word(12),
            kind,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_rejects() {
        let h = SectorHeader {
            seq: 7,
            erase_count: 3,
            kind: HeadKind::Hot,
        };
        let b = h.encode();
        assert_eq!(SectorHeader::decode(&b), Some(h));
        assert_eq!(SectorHeader::decode(&KILLED_SECTOR_HEADER), None);
        assert_eq!(SectorHeader::decode(&[0xFF; 20]), None);
        for i in 0..20 {
            let mut t = b;
            t[i] ^= 0x10;
            assert_eq!(SectorHeader::decode(&t), None, "byte {i}");
        }
    }
}
