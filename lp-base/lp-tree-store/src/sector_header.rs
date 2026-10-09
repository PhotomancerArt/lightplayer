//! The sector header (FORMAT.md "Sector"): programmed right after a
//! completed, verified erase, it is the only thing that makes a sector
//! trusted.
//!
//! Layout (24 bytes, little-endian): magic `"LTS1"` u32 | format version u16 |
//! head kind u8 | log2 of the sector size u8 | compat flags u16 | incompat
//! flags u16 | sector seq u32 | erase count u32 | CRC-32 of the first 20
//! bytes. Before a sector is erased its header is *killed* (programmed to
//! all zeros), so a torn erase can never leave an old, valid header in front
//! of weak bits.

use lp_crc32::crc32;

pub const SECTOR_MAGIC: u32 = 0x3153_544C; // "LTS1" read little-endian
/// The on-flash format version (FORMAT.md "Versioning"). 1 was the race
/// prototype and 2 the first v1 layout (neither fielded; this code reads
/// neither).
pub const FORMAT_VERSION: u16 = 3;
pub const SECTOR_HEADER_LEN: u32 = 24;
/// What a header is programmed to before its sector is erased.
pub const KILLED_SECTOR_HEADER: [u8; SECTOR_HEADER_LEN as usize] = [0; SECTOR_HEADER_LEN as usize];
/// Compat flags this version knows (none): a sector with any other compat
/// bit is read, never appended to.
pub const COMPAT_KNOWN: u16 = 0;
/// Incompat flags this version knows (none): any other incompat bit refuses
/// the mount.
pub const INCOMPAT_KNOWN: u16 = 0;

/// Which write head a sector was opened for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HeadKind {
    /// File content, directories, GC copies.
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

/// What this writer puts in a header (its flags are always zero and its
/// sector size is the flash's).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SectorHeader {
    /// Global open order: larger = opened later.
    pub seq: u32,
    pub erase_count: u32,
    pub kind: HeadKind,
}

/// What 24 header bytes say on a flash of a given sector size.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SectorRead {
    /// Not a header of this version (erased, killed, torn, another version):
    /// nothing in the sector is read.
    Untrusted,
    /// A header to read the sector by. `appendable` is false when it carries
    /// a compat flag this version does not know.
    Trusted {
        header: SectorHeader,
        appendable: bool,
    },
    /// A good header this code must not read past: an unknown incompat flag,
    /// an unknown head kind, or another sector size. The mount is refused.
    Unsupported(&'static str),
}

impl SectorHeader {
    pub fn encode(&self, sector_size: u32) -> [u8; SECTOR_HEADER_LEN as usize] {
        let mut b = [0u8; SECTOR_HEADER_LEN as usize];
        b[0..4].copy_from_slice(&SECTOR_MAGIC.to_le_bytes());
        b[4..6].copy_from_slice(&FORMAT_VERSION.to_le_bytes());
        b[6] = self.kind.index() as u8;
        b[7] = sector_size.trailing_zeros() as u8;
        // 8..12: compat and incompat flags, none.
        b[12..16].copy_from_slice(&self.seq.to_le_bytes());
        b[16..20].copy_from_slice(&self.erase_count.to_le_bytes());
        let crc = crc32(&b[..20]);
        b[20..24].copy_from_slice(&crc.to_le_bytes());
        b
    }

    pub fn decode(b: &[u8; SECTOR_HEADER_LEN as usize], sector_size: u32) -> SectorRead {
        let word = |i: usize| u32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]);
        let half = |i: usize| u16::from_le_bytes([b[i], b[i + 1]]);
        if !mutant!(NewerVersionUntrusted) && word(0) == SECTOR_MAGIC && half(4) > FORMAT_VERSION {
            // A newer writer's sector: refuse, never read it as blank (the
            // CRC is not checked: a newer layout may have moved it).
            return SectorRead::Unsupported("newer format");
        }
        if word(0) != SECTOR_MAGIC || half(4) != FORMAT_VERSION || word(20) != crc32(&b[..20]) {
            return SectorRead::Untrusted;
        }
        if half(10) & !INCOMPAT_KNOWN != 0 {
            return SectorRead::Unsupported("unknown incompat flag");
        }
        if u32::from(b[7]) != sector_size.trailing_zeros() {
            return SectorRead::Unsupported("another sector size");
        }
        let kind = match b[6] {
            0 => HeadKind::Cold,
            1 => HeadKind::Hot,
            _ => return SectorRead::Unsupported("unknown head kind"),
        };
        SectorRead::Trusted {
            header: SectorHeader {
                seq: word(12),
                erase_count: word(16),
                kind,
            },
            appendable: half(8) & !COMPAT_KNOWN == 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const S: u32 = 4096;

    /// `b` with its CRC recomputed (a header a newer writer could produce).
    fn resealed(mut b: [u8; 24]) -> [u8; 24] {
        let crc = crc32(&b[..20]);
        b[20..24].copy_from_slice(&crc.to_le_bytes());
        b
    }

    #[test]
    fn round_trip_and_rejects() {
        let h = SectorHeader {
            seq: 7,
            erase_count: 3,
            kind: HeadKind::Hot,
        };
        let b = h.encode(S);
        assert_eq!(b[7], 12);
        let trusted = SectorRead::Trusted {
            header: h,
            appendable: true,
        };
        assert_eq!(SectorHeader::decode(&b, S), trusted);
        assert_eq!(
            SectorHeader::decode(&KILLED_SECTOR_HEADER, S),
            SectorRead::Untrusted
        );
        assert_eq!(SectorHeader::decode(&[0xFF; 24], S), SectorRead::Untrusted);
        for i in 0..24 {
            let mut t = b;
            t[i] ^= 0x10;
            // Raising the version is a newer format, CRC or not.
            let want = if (4..6).contains(&i) {
                SectorRead::Unsupported("newer format")
            } else {
                SectorRead::Untrusted
            };
            assert_eq!(SectorHeader::decode(&t, S), want, "byte {i}");
        }
    }

    #[test]
    fn flags_kind_and_size_once_the_crc_is_good() {
        let b = SectorHeader {
            seq: 1,
            erase_count: 1,
            kind: HeadKind::Cold,
        }
        .encode(S);
        let mut compat = b;
        compat[9] = 0x80;
        assert!(matches!(
            SectorHeader::decode(&resealed(compat), S),
            SectorRead::Trusted {
                appendable: false,
                ..
            }
        ));
        let mut incompat = b;
        incompat[10] = 0x01;
        assert!(matches!(
            SectorHeader::decode(&resealed(incompat), S),
            SectorRead::Unsupported(_)
        ));
        let mut kind = b;
        kind[6] = 2;
        assert!(matches!(
            SectorHeader::decode(&resealed(kind), S),
            SectorRead::Unsupported(_)
        ));
        assert!(matches!(
            SectorHeader::decode(&b, 8192),
            SectorRead::Unsupported(_)
        ));
    }

    /// Magic + a newer version refuses, CRC or not; an older or never
    /// assigned lower version is only untrusted.
    #[test]
    fn a_newer_version_refuses_and_an_older_one_is_untrusted() {
        let b = SectorHeader {
            seq: 1,
            erase_count: 1,
            kind: HeadKind::Cold,
        }
        .encode(S);
        let newer = SectorRead::Unsupported("newer format");
        for v in [4u16, 5, 0x0100, 0xFFFF] {
            let mut t = b;
            t[4..6].copy_from_slice(&v.to_le_bytes());
            assert_eq!(SectorHeader::decode(&resealed(t), S), newer, "v{v}");
            assert_eq!(SectorHeader::decode(&t, S), newer, "v{v}, stale CRC");
            let mut odd = t;
            odd[6..24].fill(0xA5);
            assert_eq!(SectorHeader::decode(&odd, S), newer, "v{v}, other layout");
        }
        for v in [0u16, 1, 2] {
            let mut t = b;
            t[4..6].copy_from_slice(&v.to_le_bytes());
            assert_eq!(
                SectorHeader::decode(&resealed(t), S),
                SectorRead::Untrusted,
                "v{v}"
            );
        }
        let mut no_magic = b;
        no_magic[0] ^= 0x01;
        no_magic[4] = 4;
        assert_eq!(SectorHeader::decode(&no_magic, S), SectorRead::Untrusted);
    }
}
