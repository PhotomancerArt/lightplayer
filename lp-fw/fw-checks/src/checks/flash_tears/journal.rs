//! The two-copy journal that says which cycle was in flight.
//!
//! Before a cycle erases its sector it writes its number to both journal
//! copies. Each copy is a sector of 256 sixteen-byte slots; cycle `c` goes to
//! slot `(c + copy * 128) % 256` of each copy, and a copy is erased just
//! before its slot 0 is written. The two copies wrap half a sector apart, so
//! while one is being erased the other still holds the previous cycle — a cut
//! in a journal erase can never lose the in-flight number.
//!
//! An entry is `magic | cycle | !cycle | crc32(first 12 bytes)`, little
//! endian. A torn entry fails the check and is reported (it is a 16-byte
//! program tear, and a free sample of one), never trusted.

use super::SECTOR_SIZE;
use super::analysis::{ProgramTear, program_tear};

/// Bytes in one journal entry.
pub const ENTRY_SIZE: usize = 16;

/// Slots in one journal copy.
pub const SLOTS: u32 = (SECTOR_SIZE / ENTRY_SIZE) as u32;

/// `"FTJ1"`, little endian.
pub const MAGIC: u32 = 0x314A_5446;

/// The entry for `cycle`.
pub fn encode(cycle: u32) -> [u8; ENTRY_SIZE] {
    let mut e = [0u8; ENTRY_SIZE];
    e[0..4].copy_from_slice(&MAGIC.to_le_bytes());
    e[4..8].copy_from_slice(&cycle.to_le_bytes());
    e[8..12].copy_from_slice(&(!cycle).to_le_bytes());
    let crc = crc32(&e[..12]);
    e[12..16].copy_from_slice(&crc.to_le_bytes());
    e
}

/// What one slot holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Slot {
    /// All `0xFF`: never written since the copy was erased.
    Blank,
    /// A whole entry naming this cycle.
    Valid(u32),
    /// Neither: a torn write, or a torn erase's leftovers.
    Damaged,
}

/// Decode one slot.
pub fn decode(slot: &[u8]) -> Slot {
    debug_assert_eq!(slot.len(), ENTRY_SIZE);
    if slot.iter().all(|&b| b == 0xFF) {
        return Slot::Blank;
    }
    let word = |i: usize| u32::from_le_bytes([slot[i], slot[i + 1], slot[i + 2], slot[i + 3]]);
    if word(0) == MAGIC && word(8) == !word(4) && word(12) == crc32(&slot[..12]) {
        Slot::Valid(word(4))
    } else {
        Slot::Damaged
    }
}

/// The slot of `copy` that `cycle` is written to.
pub const fn slot_of(copy: u32, cycle: u32) -> u32 {
    (cycle.wrapping_add(copy * (SLOTS / 2))) % SLOTS
}

/// One journal copy, read.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CopyScan {
    /// Whole entries.
    pub valid: u32,
    /// Slots that are neither blank nor whole.
    pub damaged: u32,
    /// The highest cycle a whole entry names.
    pub latest: Option<u32>,
    /// The slot after `latest` — where the next entry was being written —
    /// when it is not blank: the shape of that torn 16-byte program.
    pub torn_next: Option<ProgramTear>,
}

/// Read one copy's bytes.
pub fn scan_copy(copy: u32, bytes: &[u8]) -> CopyScan {
    debug_assert_eq!(bytes.len(), SECTOR_SIZE);
    let mut scan = CopyScan::default();
    for slot in bytes.chunks(ENTRY_SIZE) {
        match decode(slot) {
            Slot::Blank => {}
            Slot::Valid(c) => {
                scan.valid += 1;
                scan.latest = Some(scan.latest.map_or(c, |l| l.max(c)));
            }
            Slot::Damaged => scan.damaged += 1,
        }
    }
    if let Some(latest) = scan.latest {
        let next = latest.wrapping_add(1);
        let at = slot_of(copy, next) as usize * ENTRY_SIZE;
        let actual = &bytes[at..at + ENTRY_SIZE];
        if decode(actual) == Slot::Damaged {
            scan.torn_next = Some(program_tear(actual, &encode(next)));
        }
    }
    scan
}

/// The in-flight cycle the copies agree on: the highest whole entry in either.
pub fn latest_of(copies: &[CopyScan]) -> Option<u32> {
    copies.iter().filter_map(|c| c.latest).max()
}

/// CRC-32 (IEEE 802.3, reflected, as zlib computes it), bitwise: twelve
/// bytes a cycle do not earn a table.
pub fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &b in data {
        crc ^= u32::from(b);
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checks::flash_tears::analysis::TearShape;

    #[test]
    fn crc32_matches_the_standard_check_value() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    }

    #[test]
    fn entries_round_trip_and_damage_is_caught() {
        assert_eq!(decode(&encode(77)), Slot::Valid(77));
        assert_eq!(decode(&[0xFF; ENTRY_SIZE]), Slot::Blank);
        let mut e = encode(77);
        e[15] |= 0x01;
        e[15] &= 0xFE;
        e[5] ^= 0x10;
        assert_eq!(decode(&e), Slot::Damaged);
    }

    #[test]
    fn the_two_copies_wrap_half_a_sector_apart() {
        assert_eq!(slot_of(0, 0), 0);
        assert_eq!(slot_of(1, 0), 128);
        assert_eq!(slot_of(1, 128), 0);
        assert_eq!(slot_of(0, 256), 0);
    }

    #[test]
    fn a_copy_names_its_latest_and_the_torn_slot_after_it() {
        let mut bytes = [0xFFu8; SECTOR_SIZE];
        for c in 0..5u32 {
            let at = slot_of(0, c) as usize * ENTRY_SIZE;
            bytes[at..at + ENTRY_SIZE].copy_from_slice(&encode(c));
        }
        // Cycle 5's entry: the first six bytes landed, then nothing.
        let at = slot_of(0, 5) as usize * ENTRY_SIZE;
        let e5 = encode(5);
        bytes[at..at + 6].copy_from_slice(&e5[..6]);
        let scan = scan_copy(0, &bytes);
        assert_eq!(scan.valid, 5);
        assert_eq!(scan.damaged, 1);
        assert_eq!(scan.latest, Some(4));
        let torn = scan.torn_next.expect("torn slot");
        assert_eq!(torn.prefix_bytes, 6);
        assert_eq!(torn.shape, TearShape::BytePrefix);
        assert_eq!(latest_of(&[scan, CopyScan::default()]), Some(4));
    }
}
