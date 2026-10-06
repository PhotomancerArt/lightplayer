//! The boot record: which core the split-link loader boots.
//!
//! A split-link image (the loader crate `lp-fw/fw-esp32c6-loader`, and
//! `docs/adr/2026-10-04-c6-split-link-firmware-loader-and-boot-records.md`)
//! keeps two records in two flash sectors. Each names a core image by flash
//! offset and length, with a sequence number; the loader boots the newest
//! valid one — unless that one is a **trial** that has failed, in which case
//! it boots the other ([`crate::choose`]).
//!
//! # Layout, version 1 (little-endian, at the start of a 4 KiB sector)
//!
//! ```text
//!  0  magic       u32   RECORD_MAGIC ("LPBR")
//!  4  version     u16   BOOT_RECORD_VERSION (1)
//!  6  flags       u16   bit 0: TRIAL (a core that has not proven itself yet)
//!  8  seq         u32   higher wins
//! 12  core_off    u32   flash offset of the core's ESP image
//! 16  core_len    u32   its length in bytes (core.bin's exact length)
//! 20  build       u32   [`build_hash`] of the core's build id
//! 24  crc         u32   CRC-32 (IEEE) of bytes 0..24
//! ---- marks: outside the CRC, each programmed in place (1 → 0, no erase)
//! 28  attempted   u32   a trial core first ran (before its bring-up)
//! 32  confirmed   u32   the trial core's link came up
//! 36  started     u32   the trial core finished its bring-up (radios, links)
//! 40  cold_tally  u32   one bit cleared, lowest first, per counted cold retry
//! ```
//!
//! `build` is what lets a rolled-back core refuse the build that just failed
//! on it, instead of taking it again every time it is offered.
//!
//! # Two kinds of write
//!
//! The record (bytes 0..28) is written once into an **erased** sector; its
//! magic and CRC make a torn write decode as "no record". The marks after it
//! are outside the CRC on purpose: the firmware programs each one later
//! **without an erase** (NOR flash takes 1 → 0 in place), so marking never
//! puts the record itself at risk.
//!
//! A torn mark program leaves some of its bits cleared and some not. Each
//! mark is read so that a torn program errs in the safe direction:
//!
//! - `attempted`, `confirmed`, `started`: any bit cleared reads as **set**.
//!   A torn `attempted` reads "attempted" (the trial is accountable); a torn
//!   `confirmed` reads "confirmed" (a core that got that far keeps
//!   booting); a torn `started` reads "started", so the cold cap stops
//!   counting — which errs towards retrying a build rather than refusing it.
//! - `cold_tally`: the count is the number of cleared bits, and one program
//!   clears exactly one more, so a torn program either counted or did not —
//!   it can only count up, never down, and never by more than one.
//!
//! Bytes after 44 are never read by the loader.

use lp_crc32::crc32;

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
/// Offset of the `started` mark within the sector.
pub const STARTED_MARK_OFFSET: u32 = 36;
/// Offset of the cold-retry tally within the sector.
pub const COLD_TALLY_OFFSET: u32 = 40;
/// Bytes the loader reads: the record and all four marks.
pub const BOOT_RECORD_READ_LEN: usize = 44;
/// Counted cold retries after which the loader rolls an unstarted trial
/// back (see [`crate::choose`]).
pub const COLD_RETRY_CAP: u32 = 3;

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

/// The 32-bit name a record gives a build: CRC-32 (IEEE) of its build id's
/// text, `"<version>+<commit>"`, without the zero padding the engine header
/// stores it with.
pub fn build_hash(build_id: &[u8]) -> u32 {
    crc32(build_id)
}

/// The marks after a record.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BootMarks {
    pub attempted: bool,
    pub confirmed: bool,
    pub started: bool,
    /// The raw tally word: all-ones is a count of zero.
    pub cold_tally: u32,
}

impl Default for BootMarks {
    fn default() -> Self {
        Self {
            attempted: false,
            confirmed: false,
            started: false,
            cold_tally: u32::MAX,
        }
    }
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
    /// The marks in a sector's first [`BOOT_RECORD_READ_LEN`] bytes. A mark
    /// beyond the bytes given reads as erased.
    pub fn decode(bytes: &[u8]) -> Self {
        let word = |at: u32| {
            bytes
                .get(at as usize..at as usize + 4)
                .map_or(u32::MAX, |w| u32::from_le_bytes([w[0], w[1], w[2], w[3]]))
        };
        Self {
            attempted: word(ATTEMPTED_MARK_OFFSET) != u32::MAX,
            confirmed: word(CONFIRMED_MARK_OFFSET) != u32::MAX,
            started: word(STARTED_MARK_OFFSET) != u32::MAX,
            cold_tally: word(COLD_TALLY_OFFSET),
        }
    }

    /// Counted cold retries: the tally's cleared bits.
    pub fn cold_retries(&self) -> u32 {
        self.cold_tally.count_zeros()
    }

    /// The word to program at [`COLD_TALLY_OFFSET`] to count one more cold
    /// retry (the lowest still-set bit cleared), or `None` when the tally is
    /// full.
    pub fn next_cold_tally_word(&self) -> Option<u32> {
        (self.cold_tally != 0).then(|| self.cold_tally & (self.cold_tally - 1))
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

    /// Bootable as the last known good: not on trial, or a trial confirmed.
    pub fn proven(&self) -> bool {
        !self.record.trial || self.marks.confirmed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(trial: bool) -> BootRecord {
        BootRecord {
            seq: 3,
            core_off: 0x2c0000,
            core_len: 1_000_000,
            build: 0x1234,
            trial,
        }
    }

    fn sector_with(r: &BootRecord) -> [u8; BOOT_RECORD_READ_LEN] {
        let mut b = [0xffu8; BOOT_RECORD_READ_LEN];
        b[..BOOT_RECORD_LEN].copy_from_slice(&r.encode());
        b
    }

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
        assert_eq!(BootRecord::decode(&[0xff; BOOT_RECORD_READ_LEN]), None);
        assert_eq!(BootSlot::decode(&[0xff; BOOT_RECORD_READ_LEN]), None);
    }

    #[test]
    fn any_torn_record_write_is_no_record() {
        let full = record(true).encode();
        // A program that stopped after `n` bytes leaves the rest erased.
        for n in 0..BOOT_RECORD_LEN {
            let mut torn = [0xffu8; BOOT_RECORD_READ_LEN];
            torn[..n].copy_from_slice(&full[..n]);
            assert_eq!(BootRecord::decode(&torn), None, "torn after {n} bytes");
        }
    }

    #[test]
    fn a_future_version_is_no_record() {
        let mut b = record(false).encode();
        b[4] = 2;
        let crc = crc32(&b[0..24]);
        b[24..28].copy_from_slice(&crc.to_le_bytes());
        assert_eq!(BootRecord::decode(&b), None);
    }

    #[test]
    fn a_fresh_record_has_no_marks() {
        let slot = BootSlot::decode(&sector_with(&record(true))).unwrap();
        assert_eq!(slot.marks, BootMarks::default());
        assert_eq!(slot.marks.cold_retries(), 0);
    }

    #[test]
    fn every_torn_flag_mark_reads_as_set() {
        for offset in [
            ATTEMPTED_MARK_OFFSET,
            CONFIRMED_MARK_OFFSET,
            STARTED_MARK_OFFSET,
        ] {
            // Programming 0 over all-ones, stopped after any bit pattern.
            for bit in 0..32 {
                let mut b = sector_with(&record(true));
                let torn = !(1u32 << bit);
                b[offset as usize..offset as usize + 4].copy_from_slice(&torn.to_le_bytes());
                let m = BootMarks::decode(&b);
                let set = match offset {
                    ATTEMPTED_MARK_OFFSET => m.attempted,
                    CONFIRMED_MARK_OFFSET => m.confirmed,
                    _ => m.started,
                };
                assert!(set, "mark at {offset} with bit {bit} cleared");
                // The other marks are untouched by it.
                assert_eq!(m.cold_retries(), 0);
            }
        }
    }

    #[test]
    fn the_tally_counts_one_cleared_bit_per_program_lowest_first() {
        let mut m = BootMarks::default();
        for n in 1..=32 {
            let next = m.next_cold_tally_word().unwrap();
            assert_eq!(next, m.cold_tally & !(1 << (n - 1)), "lowest bit first");
            // NOR only clears: the programmed word ANDs into what is there.
            m.cold_tally &= next;
            assert_eq!(m.cold_retries(), n);
        }
        assert_eq!(m.next_cold_tally_word(), None, "a full tally");
    }

    #[test]
    fn a_torn_tally_program_counts_zero_or_one_never_down() {
        let before = BootMarks {
            cold_tally: 0xffff_fffc,
            ..BootMarks::default()
        };
        let next = before.next_cold_tally_word().unwrap();
        // The program changes one bit: torn, it either landed or did not.
        for landed in [before.cold_tally, next] {
            let after = BootMarks {
                cold_tally: landed,
                ..before
            };
            let d = after.cold_retries() - before.cold_retries();
            assert!(d <= 1);
        }
    }

    #[test]
    fn marks_read_past_the_bytes_given_are_erased() {
        let b = sector_with(&record(true));
        let m = BootMarks::decode(&b[..36]);
        assert!(!m.started);
        assert_eq!(m.cold_tally, u32::MAX);
    }

    #[test]
    fn proven_is_not_on_trial_or_confirmed() {
        let slot = |trial, confirmed| BootSlot {
            record: record(trial),
            marks: BootMarks {
                confirmed,
                ..BootMarks::default()
            },
        };
        assert!(slot(false, false).proven());
        assert!(slot(true, true).proven());
        assert!(!slot(true, false).proven());
    }

    #[test]
    fn the_marks_follow_the_record_in_order() {
        assert_eq!(ATTEMPTED_MARK_OFFSET as usize, BOOT_RECORD_LEN);
        assert_eq!(CONFIRMED_MARK_OFFSET, ATTEMPTED_MARK_OFFSET + 4);
        assert_eq!(STARTED_MARK_OFFSET, CONFIRMED_MARK_OFFSET + 4);
        assert_eq!(COLD_TALLY_OFFSET, STARTED_MARK_OFFSET + 4);
        assert_eq!(BOOT_RECORD_READ_LEN as u32, COLD_TALLY_OFFSET + 4);
    }
}
