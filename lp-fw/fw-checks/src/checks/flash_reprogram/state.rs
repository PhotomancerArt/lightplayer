//! What one boot tells the next: an append-only log of 64-byte entries in
//! the state sector.
//!
//! An entry is written before the scan-done line, so no cut ever lands in
//! one; the CRC is there for a boot the host stopped by hand.

use super::{MAX_GROUPS, MAX_KEEP, MAX_TORN, SECTOR_SIZE, WRITE_SIZE};

/// Bytes in one entry.
pub const ENTRY_SIZE: usize = 64;

/// Entries in the state sector.
pub const ENTRIES: usize = SECTOR_SIZE / ENTRY_SIZE;

const MAGIC: u32 = 0x4652_5031; // "FRP1"
const NONE: u32 = u32::MAX;

/// What the loop after a boot is doing, and what that boot left to re-check.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct State {
    /// Boots since the region was fresh (the first boot's entry is 0).
    pub seq: u32,
    /// The generation the work loop programs the region with.
    pub generation: u32,
    /// Sector indices the loop skips: they hold the words to re-check.
    pub keep: [Option<u32>; MAX_KEEP],
    /// The generation the re-check words were programmed with.
    pub pending_generation: u32,
    /// The re-programmed words to re-check, absolute addresses.
    pub pending: [Option<u32>; MAX_TORN],
    /// The writes re-programmed: (first byte, bytes re-programmed from it).
    pub groups: [Option<(u32, u32)>; MAX_GROUPS],
}

impl State {
    pub fn keep_list(&self) -> impl Iterator<Item = u32> + '_ {
        self.keep.iter().flatten().copied()
    }

    pub fn pending_list(&self) -> impl Iterator<Item = u32> + '_ {
        self.pending.iter().flatten().copied()
    }

    pub fn group_list(&self) -> impl Iterator<Item = (u32, u32)> + '_ {
        self.groups.iter().flatten().copied()
    }

    pub fn encode(&self) -> [u8; ENTRY_SIZE] {
        let mut w = [NONE; ENTRY_SIZE / 4];
        w[0] = MAGIC;
        w[1] = self.seq;
        w[2] = self.generation;
        for (i, k) in self.keep.iter().enumerate() {
            w[3 + i] = k.unwrap_or(NONE);
        }
        w[5] = self.pending_generation;
        for (i, p) in self.pending.iter().enumerate() {
            w[6 + i] = p.unwrap_or(NONE);
        }
        // A write is 256-aligned and its span is 4..=256 bytes: the span's
        // words less one in the low byte.
        for (i, g) in self.groups.iter().enumerate() {
            w[12 + i] = g.map_or(NONE, |(at, span)| at | (span / 4 - 1));
        }
        let mut out = [0u8; ENTRY_SIZE];
        for (i, v) in w.iter().enumerate().take(15) {
            out[4 * i..4 * i + 4].copy_from_slice(&v.to_le_bytes());
        }
        let crc = crc32(&out[..60]);
        out[60..].copy_from_slice(&crc.to_le_bytes());
        out
    }

    pub fn decode(b: &[u8]) -> Option<Self> {
        let w = |i: usize| u32::from_le_bytes([b[4 * i], b[4 * i + 1], b[4 * i + 2], b[4 * i + 3]]);
        if b.len() < ENTRY_SIZE || w(0) != MAGIC || w(15) != crc32(&b[..60]) {
            return None;
        }
        let opt = |v: u32| (v != NONE).then_some(v);
        let mut s = State {
            seq: w(1),
            generation: w(2),
            pending_generation: w(5),
            ..State::default()
        };
        for i in 0..MAX_KEEP {
            s.keep[i] = opt(w(3 + i));
        }
        for i in 0..MAX_TORN {
            s.pending[i] = opt(w(6 + i));
        }
        let mask = WRITE_SIZE as u32 - 1;
        for i in 0..MAX_GROUPS {
            s.groups[i] = opt(w(12 + i)).map(|g| (g & !mask, ((g & mask) + 1) * 4));
        }
        Some(s)
    }
}

/// The newest entry in a state sector, and the slot the next goes in
/// (`None`: the sector is full and must be erased first).
pub fn scan(sector: &[u8]) -> (Option<State>, Option<usize>) {
    let mut latest = None;
    for slot in 0..ENTRIES {
        let b = &sector[slot * ENTRY_SIZE..(slot + 1) * ENTRY_SIZE];
        if b.iter().all(|&x| x == 0xFF) {
            return (latest, Some(slot));
        }
        if let Some(s) = State::decode(b) {
            latest = Some(s);
        }
    }
    (latest, None)
}

/// CRC-32 (IEEE, reflected), bitwise: 60 bytes a boot.
fn crc32(data: &[u8]) -> u32 {
    let mut c = !0u32;
    for &b in data {
        c ^= u32::from(b);
        for _ in 0..8 {
            c = if c & 1 != 0 {
                (c >> 1) ^ 0xEDB8_8320
            } else {
                c >> 1
            };
        }
    }
    !c
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_entry_round_trips_and_the_newest_wins() {
        let s = State {
            seq: 3,
            generation: 9,
            keep: [Some(40), None],
            pending_generation: 8,
            pending: [Some(0x37_8a14), Some(0x37_8a18), None, None, None, None],
            groups: [Some((0x37_8a00, 28)), Some((0x37_9f00, 256)), None],
        };
        assert_eq!(State::decode(&s.encode()), Some(s));
        let mut sector = [0xFFu8; SECTOR_SIZE];
        assert_eq!(scan(&sector), (None, Some(0)));
        sector[..64].copy_from_slice(&State { seq: 2, ..s }.encode());
        sector[64..128].copy_from_slice(&s.encode());
        assert_eq!(scan(&sector), (Some(s), Some(2)));
        // A damaged entry is skipped, not trusted.
        sector[64 + 9] ^= 1;
        assert_eq!(scan(&sector).0.unwrap().seq, 2);
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    }
}
