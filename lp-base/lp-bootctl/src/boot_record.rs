//! The boot record: which core the split-link loader boots.
//!
//! A split-link image (see `docs/adr/` once written, and the loader crate
//! `lp-fw/fw-esp32c6-loader`) keeps two records in two flash sectors. Each
//! names a core image by flash offset and length, with a sequence number;
//! the loader boots the newest valid one — unless that one is a **trial**
//! that already failed, in which case it boots the other.
//!
//! # Layout (little-endian, at the start of a 4 KiB sector)
//!
//! ```text
//!  0  magic      u32   RECORD_MAGIC
//!  4  version    u16   BOOT_RECORD_VERSION
//!  6  flags      u16   bit 0: TRIAL (a core that has not proven itself yet)
//!  8  seq        u32   higher wins
//! 12  core_off   u32   flash offset of the core's ESP image
//! 16  core_len   u32   its length in bytes
//! 20  build      u32   [`build_hash`] of the core's build id
//! 24  crc        u32   CRC-32 of bytes 0..24
//! 28  attempted  u32   0xFFFF_FFFF until the trial core first runs, then 0
//! 32  confirmed  u32   0xFFFF_FFFF until the trial core proves itself, then 0
//! ```
//!
//! `build` is what lets a rolled-back core refuse the build that just
//! failed on it, instead of taking it again every time it is offered.
//!
//! # Two kinds of write
//!
//! The record (bytes 0..28) is written once into an **erased** sector; its
//! magic and CRC make a torn write decode as "no record". The two marks
//! after it are outside the CRC on purpose: the firmware programs each one
//! later **without an erase** (NOR flash takes 1 → 0 in place), so marking
//! never puts the record itself at risk. A torn mark program leaves some
//! bits cleared and some not; anything other than all-ones reads as set,
//! which errs towards "attempted" (roll back) and "confirmed" (keep) — the
//! two safe directions.

use crate::crc32::crc32;

/// `"LPBR"`, little-endian.
pub const RECORD_MAGIC: u32 = u32::from_le_bytes(*b"LPBR");
/// Bumped on any change to the layout above.
pub const BOOT_RECORD_VERSION: u16 = 1;
/// Bytes covered by the CRC, plus the CRC.
pub const BOOT_RECORD_LEN: usize = 28;
/// Offset of the `attempted` mark within the sector.
pub const ATTEMPTED_MARK_OFFSET: u32 = 28;
/// Offset of the `confirmed` mark within the sector.
pub const CONFIRMED_MARK_OFFSET: u32 = 32;
/// Bytes the loader reads: the record and both marks.
pub const BOOT_RECORD_READ_LEN: usize = 36;

const FLAG_TRIAL: u16 = 1;

/// One decoded record.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BootRecord {
    pub seq: u32,
    pub core_off: u32,
    pub core_len: u32,
    /// [`build_hash`] of the core's build id.
    pub build: u32,
    /// Written for a core that has never run: it boots on trial.
    pub trial: bool,
}

/// The 32-bit name a record gives a build: CRC-32 of its build id.
pub fn build_hash(build_id: &[u8]) -> u32 {
    crc32(build_id)
}

/// The marks after a record.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BootMarks {
    pub attempted: bool,
    pub confirmed: bool,
}

/// What one record sector holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BootSlot {
    pub record: BootRecord,
    pub marks: BootMarks,
}

impl BootRecord {
    /// The 28 record bytes, CRC included. Write them into an erased sector;
    /// the marks after them stay erased.
    pub fn encode(&self) -> [u8; BOOT_RECORD_LEN] {
        let mut out = [0u8; BOOT_RECORD_LEN];
        out[0..4].copy_from_slice(&RECORD_MAGIC.to_le_bytes());
        out[4..6].copy_from_slice(&BOOT_RECORD_VERSION.to_le_bytes());
        let flags = if self.trial { FLAG_TRIAL } else { 0 };
        out[6..8].copy_from_slice(&flags.to_le_bytes());
        out[8..12].copy_from_slice(&self.seq.to_le_bytes());
        out[12..16].copy_from_slice(&self.core_off.to_le_bytes());
        out[16..20].copy_from_slice(&self.core_len.to_le_bytes());
        out[20..24].copy_from_slice(&self.build.to_le_bytes());
        let crc = crc32(&out[0..24]);
        out[24..28].copy_from_slice(&crc.to_le_bytes());
        out
    }

    /// The record in `bytes` (at least [`BOOT_RECORD_LEN`]), or `None` for an
    /// erased, torn, foreign or future-version sector.
    pub fn decode(bytes: &[u8]) -> Option<Self> {
        let b = bytes.get(..BOOT_RECORD_LEN)?;
        let word = |at: usize| u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]]);
        if word(0) != RECORD_MAGIC || word(24) != crc32(&b[0..24]) {
            return None;
        }
        if u16::from_le_bytes([b[4], b[5]]) != BOOT_RECORD_VERSION {
            return None;
        }
        let flags = u16::from_le_bytes([b[6], b[7]]);
        Some(Self {
            seq: word(8),
            core_off: word(12),
            core_len: word(16),
            build: word(20),
            trial: flags & FLAG_TRIAL != 0,
        })
    }
}

impl BootMarks {
    /// The marks in a sector's first [`BOOT_RECORD_READ_LEN`] bytes.
    pub fn decode(bytes: &[u8]) -> Self {
        let set = |at: usize| {
            bytes
                .get(at..at + 4)
                .is_some_and(|w| w != [0xff, 0xff, 0xff, 0xff])
        };
        Self {
            attempted: set(ATTEMPTED_MARK_OFFSET as usize),
            confirmed: set(CONFIRMED_MARK_OFFSET as usize),
        }
    }
}

impl BootSlot {
    /// A sector's first [`BOOT_RECORD_READ_LEN`] bytes, if they hold a record.
    pub fn decode(bytes: &[u8]) -> Option<Self> {
        Some(Self {
            record: BootRecord::decode(bytes)?,
            marks: BootMarks::decode(bytes),
        })
    }

    /// A trial that ran and never proved itself: the loader will not boot it.
    pub fn failed(&self) -> bool {
        self.record.trial && self.marks.attempted && !self.marks.confirmed
    }

    /// Bootable as the last known good: not on trial, or a trial confirmed.
    pub fn proven(&self) -> bool {
        !self.record.trial || self.marks.confirmed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_record_round_trips() {
        let r = BootRecord {
            seq: 7,
            core_off: 0x18000,
            core_len: 1_150_816,
            build: 0x1234,
            trial: true,
        };
        assert_eq!(BootRecord::decode(&r.encode()), Some(r));
    }

    #[test]
    fn erased_flash_is_no_record() {
        assert_eq!(BootRecord::decode(&[0xff; 36]), None);
        assert_eq!(BootSlot::decode(&[0xff; 36]), None);
    }

    #[test]
    fn any_torn_record_write_is_no_record() {
        let full = BootRecord {
            seq: 3,
            core_off: 0x2c0000,
            core_len: 1_000_000,
            build: 0x1234,
            trial: true,
        }
        .encode();
        // A program that stopped after `n` bytes leaves the rest erased.
        for n in 0..BOOT_RECORD_LEN {
            let mut torn = [0xffu8; 36];
            torn[..n].copy_from_slice(&full[..n]);
            assert_eq!(BootRecord::decode(&torn), None, "torn after {n} bytes");
        }
    }

    #[test]
    fn a_future_version_is_no_record() {
        let mut b = BootRecord {
            seq: 1,
            core_off: 0x18000,
            core_len: 1,
            build: 0x1234,
            trial: false,
        }
        .encode();
        b[4] = 2;
        let crc = crc32(&b[0..24]);
        b[24..28].copy_from_slice(&crc.to_le_bytes());
        assert_eq!(BootRecord::decode(&b), None);
    }

    #[test]
    fn marks_read_as_set_once_any_bit_is_programmed() {
        let mut b = [0xffu8; 36];
        b[..28].copy_from_slice(
            &BootRecord {
                seq: 1,
                core_off: 0,
                core_len: 1,
                build: 0x1234,
                trial: true,
            }
            .encode(),
        );
        assert_eq!(BootMarks::decode(&b), BootMarks::default());
        b[29] = 0x7f; // a torn program of the attempted mark
        assert!(BootMarks::decode(&b).attempted);
        assert!(!BootMarks::decode(&b).confirmed);
    }

    #[test]
    fn a_trial_fails_only_when_attempted_and_unconfirmed() {
        let record = BootRecord {
            seq: 2,
            core_off: 0,
            core_len: 1,
            build: 0x1234,
            trial: true,
        };
        let slot = |attempted, confirmed| BootSlot {
            record,
            marks: BootMarks {
                attempted,
                confirmed,
            },
        };
        assert!(!slot(false, false).failed());
        assert!(slot(true, false).failed());
        assert!(!slot(true, true).failed());
        assert!(slot(true, true).proven());
        assert!(!slot(true, false).proven());
    }
}
