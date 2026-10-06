//! The engine header: the first bytes of the engine, which the core reads to
//! decide whether there is an engine it may enter, and where.
//!
//! # Layout, version 1 (little-endian, at the engine's first byte)
//!
//! ```text
//!  0  magic       u32       ENGINE_MAGIC ("LPEH")
//!  4  version     u16       ENGINE_HEADER_VERSION (1)
//!  6  header_len  u16       ENGINE_HEADER_LEN (88)
//!  8  entry       u32       the address the core calls (`fn(CoreBoot)` in the firmware)
//! 12  len         u32       engine.bin's length, header included (packager-patched)
//! 16  build_id    [u8; 64]  "<version>+<commit>", zero-padded
//! 80  crc         u32       CRC-32 (IEEE) of bytes 0..80 (packager-patched)
//! 84  commit      u32       ENGINE_COMMITTED exactly; anything else is uncommitted
//! ```
//!
//! The firmware links the header with `len` and `crc` as zero placeholders —
//! an entry pointer has no value at compile time to checksum — and the
//! packager fills both after the link ([`patch`]). The flashed image carries
//! the header committed.
//!
//! # The commit discipline
//!
//! An over-the-air writer writes the header's sector with the commit word
//! **erased**, then programs the commit word in a separate operation (NOR:
//! 1 → 0). So a torn header write fails its CRC (or its magic), and a torn
//! commit program leaves a word that is not exactly [`ENGINE_COMMITTED`]:
//! neither is ever valid, and a core never enters a half-written engine.

use lp_crc32::crc32;

/// `"LPEH"`, little-endian.
pub const ENGINE_MAGIC: u32 = u32::from_le_bytes(*b"LPEH");
pub const ENGINE_HEADER_VERSION: u16 = 1;
/// Bytes in the header, commit word included.
pub const ENGINE_HEADER_LEN: usize = 88;
/// Length of the build id field.
pub const ENGINE_BUILD_ID_LEN: usize = 64;
/// `"LPOK"`, little-endian: the one committed value.
pub const ENGINE_COMMITTED: u32 = u32::from_le_bytes(*b"LPOK");

pub const ENGINE_ENTRY_OFFSET: usize = 8;
pub const ENGINE_LEN_OFFSET: usize = 12;
pub const ENGINE_BUILD_ID_OFFSET: usize = 16;
pub const ENGINE_CRC_OFFSET: usize = 80;
pub const ENGINE_COMMIT_OFFSET: usize = 84;

/// A decoded, committed header.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EngineHeader {
    pub entry: u32,
    pub len: u32,
    pub build_id: [u8; ENGINE_BUILD_ID_LEN],
}

/// Why a header is not one the core may enter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EngineHeaderError {
    /// Fewer bytes than a header.
    Short,
    /// No header here (erased flash, or something else).
    NoHeader,
    /// A header of another version, or another size.
    Version,
    /// A torn or corrupt header: the CRC does not match.
    Torn,
    /// The header is whole but its commit word is not exactly committed.
    NotCommitted,
    /// A different build's engine.
    WrongBuild,
    /// Its length is shorter than a header or does not fit the room.
    DoesNotFit,
}

impl EngineHeaderError {
    /// Words for a log line.
    pub const fn describe(self) -> &'static str {
        match self {
            Self::Short => "too short",
            Self::NoHeader => "no engine header",
            Self::Version => "an engine header of another version",
            Self::Torn => "engine header torn",
            Self::NotCommitted => "engine not committed",
            Self::WrongBuild => "engine of another build",
            Self::DoesNotFit => "engine does not fit",
        }
    }
}

/// Why the packager cannot patch an engine.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PatchError {
    /// The engine does not start with an unpatched v1 header.
    NotAHeader,
    /// The header's build id is not the one expected.
    WrongBuild,
    /// The engine is longer than a `u32` can say.
    TooLong,
    /// The linked header is not committed.
    NotCommitted,
}

fn word(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

impl EngineHeader {
    /// The header at the start of `bytes`, if it is whole, of this version
    /// and committed. Validity against a build and a room is [`validate`].
    ///
    /// [`validate`]: EngineHeader::validate
    pub fn decode(bytes: &[u8]) -> Result<Self, EngineHeaderError> {
        let b = bytes
            .get(..ENGINE_HEADER_LEN)
            .ok_or(EngineHeaderError::Short)?;
        if word(b, 0) != ENGINE_MAGIC {
            return Err(EngineHeaderError::NoHeader);
        }
        if word(b, ENGINE_CRC_OFFSET) != crc32(&b[..ENGINE_CRC_OFFSET]) {
            return Err(EngineHeaderError::Torn);
        }
        if u16::from_le_bytes([b[4], b[5]]) != ENGINE_HEADER_VERSION
            || usize::from(u16::from_le_bytes([b[6], b[7]])) != ENGINE_HEADER_LEN
        {
            return Err(EngineHeaderError::Version);
        }
        if word(b, ENGINE_COMMIT_OFFSET) != ENGINE_COMMITTED {
            return Err(EngineHeaderError::NotCommitted);
        }
        let mut build_id = [0u8; ENGINE_BUILD_ID_LEN];
        build_id.copy_from_slice(
            &b[ENGINE_BUILD_ID_OFFSET..ENGINE_BUILD_ID_OFFSET + ENGINE_BUILD_ID_LEN],
        );
        Ok(Self {
            entry: word(b, ENGINE_ENTRY_OFFSET),
            len: word(b, ENGINE_LEN_OFFSET),
            build_id,
        })
    }

    /// Whether this header is the engine of `expected_build_id` and fits in
    /// `room` bytes.
    pub fn validate(
        &self,
        expected_build_id: &[u8; ENGINE_BUILD_ID_LEN],
        room: u32,
    ) -> Result<(), EngineHeaderError> {
        if &self.build_id != expected_build_id {
            return Err(EngineHeaderError::WrongBuild);
        }
        if (self.len as usize) < ENGINE_HEADER_LEN || self.len > room {
            return Err(EngineHeaderError::DoesNotFit);
        }
        Ok(())
    }
}

/// A whole committed header for these fields: what the packager leaves in
/// `engine.bin`, and what an over-the-air writer writes (commit last).
pub fn encode_fields(
    entry: u32,
    len: u32,
    build_id: &[u8; ENGINE_BUILD_ID_LEN],
) -> [u8; ENGINE_HEADER_LEN] {
    let mut out = [0u8; ENGINE_HEADER_LEN];
    out[0..4].copy_from_slice(&ENGINE_MAGIC.to_le_bytes());
    out[4..6].copy_from_slice(&ENGINE_HEADER_VERSION.to_le_bytes());
    out[6..8].copy_from_slice(&(ENGINE_HEADER_LEN as u16).to_le_bytes());
    out[ENGINE_ENTRY_OFFSET..ENGINE_ENTRY_OFFSET + 4].copy_from_slice(&entry.to_le_bytes());
    out[ENGINE_LEN_OFFSET..ENGINE_LEN_OFFSET + 4].copy_from_slice(&len.to_le_bytes());
    out[ENGINE_BUILD_ID_OFFSET..ENGINE_BUILD_ID_OFFSET + ENGINE_BUILD_ID_LEN]
        .copy_from_slice(build_id);
    let crc = crc32(&out[..ENGINE_CRC_OFFSET]);
    out[ENGINE_CRC_OFFSET..ENGINE_CRC_OFFSET + 4].copy_from_slice(&crc.to_le_bytes());
    out[ENGINE_COMMIT_OFFSET..ENGINE_COMMIT_OFFSET + 4]
        .copy_from_slice(&ENGINE_COMMITTED.to_le_bytes());
    out
}

/// The packager's step: fill `len` (the whole `engine`'s length) and `crc`
/// in the linked header at the start of `engine`, after checking it is a
/// committed v1 header for `expected_build_id`.
pub fn patch(
    engine: &mut [u8],
    expected_build_id: &[u8; ENGINE_BUILD_ID_LEN],
) -> Result<EngineHeader, PatchError> {
    let len = u32::try_from(engine.len()).map_err(|_| PatchError::TooLong)?;
    let b = engine
        .get_mut(..ENGINE_HEADER_LEN)
        .ok_or(PatchError::NotAHeader)?;
    if word(b, 0) != ENGINE_MAGIC
        || u16::from_le_bytes([b[4], b[5]]) != ENGINE_HEADER_VERSION
        || usize::from(u16::from_le_bytes([b[6], b[7]])) != ENGINE_HEADER_LEN
    {
        return Err(PatchError::NotAHeader);
    }
    if &b[ENGINE_BUILD_ID_OFFSET..ENGINE_BUILD_ID_OFFSET + ENGINE_BUILD_ID_LEN]
        != expected_build_id.as_slice()
    {
        return Err(PatchError::WrongBuild);
    }
    if word(b, ENGINE_COMMIT_OFFSET) != ENGINE_COMMITTED {
        return Err(PatchError::NotCommitted);
    }
    b[ENGINE_LEN_OFFSET..ENGINE_LEN_OFFSET + 4].copy_from_slice(&len.to_le_bytes());
    let crc = crc32(&b[..ENGINE_CRC_OFFSET]);
    b[ENGINE_CRC_OFFSET..ENGINE_CRC_OFFSET + 4].copy_from_slice(&crc.to_le_bytes());
    EngineHeader::decode(b).map_err(|_| PatchError::NotAHeader)
}

/// A build id, zero-padded to the field: `"<version>+<commit>"`.
pub const fn build_id_field(id: &[u8]) -> [u8; ENGINE_BUILD_ID_LEN] {
    let mut out = [0u8; ENGINE_BUILD_ID_LEN];
    let mut i = 0;
    while i < id.len() && i < ENGINE_BUILD_ID_LEN {
        out[i] = id[i];
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::vec::Vec;

    const ID: [u8; ENGINE_BUILD_ID_LEN] = build_id_field(b"2026.10.05-1+abc123456789");

    fn header() -> [u8; ENGINE_HEADER_LEN] {
        encode_fields(0x4240_0123, 1_830_000, &ID)
    }

    #[test]
    fn a_header_round_trips_and_validates() {
        let h = EngineHeader::decode(&header()).unwrap();
        assert_eq!(h.entry, 0x4240_0123);
        assert_eq!(h.len, 1_830_000);
        assert_eq!(h.validate(&ID, 2_000_000), Ok(()));
        assert_eq!(
            h.validate(&ID, 1_000_000),
            Err(EngineHeaderError::DoesNotFit)
        );
        let other = build_id_field(b"2026.10.05-2+abc123456789");
        assert_eq!(
            h.validate(&other, 2_000_000),
            Err(EngineHeaderError::WrongBuild)
        );
    }

    #[test]
    fn erased_flash_is_no_header() {
        assert_eq!(
            EngineHeader::decode(&[0xff; ENGINE_HEADER_LEN]),
            Err(EngineHeaderError::NoHeader)
        );
        assert_eq!(
            EngineHeader::decode(&[0; 10]),
            Err(EngineHeaderError::Short)
        );
    }

    #[test]
    fn any_torn_header_write_is_not_valid() {
        let full = header();
        // Written from erased: a program that stopped after `n` bytes.
        for n in 0..ENGINE_HEADER_LEN {
            let mut torn = [0xffu8; ENGINE_HEADER_LEN];
            torn[..n].copy_from_slice(&full[..n]);
            assert!(EngineHeader::decode(&torn).is_err(), "torn after {n} bytes");
        }
        // The over-the-air order: everything but the commit word, then the
        // word on its own.
        let mut uncommitted = full;
        uncommitted[ENGINE_COMMIT_OFFSET..].fill(0xff);
        assert_eq!(
            EngineHeader::decode(&uncommitted),
            Err(EngineHeaderError::NotCommitted)
        );
    }

    #[test]
    fn every_torn_commit_program_is_not_valid() {
        // Programming ENGINE_COMMITTED over 0xFFFF_FFFF clears its zero bits;
        // a torn program clears any subset of them. Only the full set is
        // committed.
        let to_clear = !ENGINE_COMMITTED;
        let bits: Vec<u32> = (0..32).filter(|b| to_clear & (1 << b) != 0).collect();
        let mut h = header();
        for subset in 0u32..(1 << bits.len()) {
            let mut w = u32::MAX;
            for (k, bit) in bits.iter().enumerate() {
                if subset & (1 << k) != 0 {
                    w &= !(1 << bit);
                }
            }
            h[ENGINE_COMMIT_OFFSET..].copy_from_slice(&w.to_le_bytes());
            let ok = EngineHeader::decode(&h).is_ok();
            assert_eq!(ok, w == ENGINE_COMMITTED, "commit word {w:#010x}");
        }
    }

    #[test]
    fn the_packager_fills_len_and_crc() {
        // What the firmware links: placeholders, committed.
        let mut linked = header();
        linked[ENGINE_LEN_OFFSET..ENGINE_LEN_OFFSET + 4].fill(0);
        linked[ENGINE_CRC_OFFSET..ENGINE_CRC_OFFSET + 4].fill(0);
        assert_eq!(EngineHeader::decode(&linked), Err(EngineHeaderError::Torn));
        let mut engine = linked.to_vec();
        engine.resize(5000, 0x5a);
        let h = patch(&mut engine, &ID).unwrap();
        assert_eq!(h.len, 5000);
        assert_eq!(EngineHeader::decode(&engine), Ok(h));
        let other = build_id_field(b"x+y");
        assert_eq!(patch(&mut engine, &other), Err(PatchError::WrongBuild));
        assert_eq!(patch(&mut [0xff; 100], &ID), Err(PatchError::NotAHeader));
    }

    #[test]
    fn a_build_id_is_zero_padded() {
        let f = build_id_field(b"v+c");
        assert_eq!(&f[..3], b"v+c");
        assert!(f[3..].iter().all(|b| *b == 0));
    }
}
