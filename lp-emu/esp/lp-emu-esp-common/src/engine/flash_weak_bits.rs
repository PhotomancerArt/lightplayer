//! Weak bits: what a torn erase leaves in a NOR cell that a read cannot
//! settle.
//!
//! A cell an erase was lifting when power went may sit between its two
//! states. On the part it reads as a different value from one read to the
//! next, until the sector is erased in full. `lp-nor-sim` models it as one
//! mask byte per cell byte (a set bit is weak) read as
//! `(stored & !mask) | (random & mask)`, and so does this, per 4 KiB sector,
//! beside the [`FlashImage`](super::spi_flash::FlashImage)'s bytes and inside
//! the same lock.
//!
//! The lifecycle, which the tests pin:
//!
//! - **Set** only by a torn erase whose shape leaves them
//!   ([`flash_cut`](super::flash_cut)): calibrated *erasing* and *reads
//!   `0xFF` with weak bits*, and the guessed models' torn-erase shapes.
//! - **Read** seeded: one [`SimRng`] seeded from the cut's seed, drawn once
//!   per weak byte read, so a run is a function of `(seed, address, read
//!   sequence)` and nothing else. No clock, no OS randomness.
//! - **Solidified** by a program: a bit programmed to 0 is a stable 0 (the
//!   mask bit clears); a 1 left alone stays weak.
//! - **Cleared** by an erase of the sector, and by nothing else. A power
//!   cycle keeps them: the flash survives it.
//! - **In process only** (plan Q8). A `File`-backed flush writes the stored
//!   cells and warns once that the weak bits did not go with them; there is
//!   no sidecar file.

use alloc::collections::BTreeMap;
use alloc::vec;
use alloc::vec::Vec;

use lp_nor_sim::SimRng;

use super::spi_flash::SECTOR_LEN;

/// Mixed into a cut's seed for the weak-read stream, so the reads and the
/// tear that made the weak bits draw from different sequences.
const WEAK_READ_SALT: u64 = 0x4EAD_4EAD_4EAD_4EAD;

/// Every weak bit on the chip, by sector.
#[derive(Clone, Debug)]
pub struct FlashWeakBits {
    /// Sector index → one mask byte per cell byte of that sector.
    masks: BTreeMap<u32, Vec<u8>>,
    rng: SimRng,
    /// Set once a flush has said the weak bits stay behind.
    flush_warned: bool,
}

impl Default for FlashWeakBits {
    fn default() -> Self {
        Self {
            masks: BTreeMap::new(),
            rng: SimRng::new(WEAK_READ_SALT),
            flush_warned: false,
        }
    }
}

impl FlashWeakBits {
    /// No weak bit anywhere: the fast path every read takes on a chip no
    /// cut ever tore.
    pub fn is_empty(&self) -> bool {
        self.masks.is_empty()
    }

    /// How many weak bits the chip holds.
    pub fn count(&self) -> u64 {
        self.masks
            .values()
            .map(|m| m.iter().map(|b| b.count_ones() as u64).sum::<u64>())
            .sum()
    }

    /// The sectors holding at least one weak bit, ascending.
    pub fn sectors(&self) -> Vec<u32> {
        self.masks.keys().copied().collect()
    }

    /// The mask byte at `addr` (0 when the cell is stable).
    pub fn mask_at(&self, addr: u32) -> u8 {
        let (sector, off) = split(addr);
        self.masks.get(&sector).map(|m| m[off]).unwrap_or(0)
    }

    /// Restart the read stream from a cut's `seed`.
    pub(crate) fn reseed(&mut self, seed: u64) {
        self.rng = SimRng::new(seed ^ WEAK_READ_SALT);
    }

    /// OR `mask` into the masks from `addr` on (a torn erase's new weak
    /// bits; the old ones stay, as they do in `lp-nor-sim`). All-zero sectors
    /// are not stored.
    pub(crate) fn add(&mut self, addr: u32, mask: &[u8]) {
        for (i, &m) in mask.iter().enumerate() {
            if m == 0 {
                continue;
            }
            let (sector, off) = split(addr + i as u32);
            self.masks
                .entry(sector)
                .or_insert_with(|| vec![0; SECTOR_LEN as usize])[off] |= m;
        }
    }

    /// A program of `data` at `addr` landed (fully or torn) and the cells
    /// now read `cells`: a bit that is 0 there is solid now.
    pub(crate) fn solidify(&mut self, addr: u32, cells: &[u8]) {
        if self.masks.is_empty() {
            return;
        }
        for (i, &c) in cells.iter().enumerate() {
            let (sector, off) = split(addr + i as u32);
            if let Some(mask) = self.masks.get_mut(&sector) {
                mask[off] &= c;
            }
        }
        self.drop_empty();
    }

    /// An erase of `len` bytes at `addr` finished: its sectors are stable.
    pub(crate) fn clear(&mut self, addr: u32, len: u32) {
        if self.masks.is_empty() {
            return;
        }
        let first = addr / SECTOR_LEN;
        let last = (addr + len.saturating_sub(1)) / SECTOR_LEN;
        self.masks.retain(|s, _| *s < first || *s > last);
    }

    /// Every weak bit gone (a chip erase).
    pub(crate) fn clear_all(&mut self) {
        self.masks.clear();
    }

    /// Does any byte of `addr..addr + len` hold a weak bit?
    pub(crate) fn touches(&self, addr: u32, len: u32) -> bool {
        if self.masks.is_empty() || len == 0 {
            return false;
        }
        let first = addr / SECTOR_LEN;
        let last = (addr + len - 1) / SECTOR_LEN;
        self.masks.range(first..=last).next().is_some()
    }

    /// Read noise: every weak bit of `out` (the stored cells of
    /// `addr..addr + out.len()`) replaced by a fresh seeded value.
    pub(crate) fn apply(&mut self, addr: u32, out: &mut [u8]) {
        for (i, b) in out.iter_mut().enumerate() {
            let (sector, off) = split(addr + i as u32);
            if let Some(mask) = self.masks.get(&sector) {
                let m = mask[off];
                if m != 0 {
                    *b = (*b & !m) | (self.rng.next_u8() & m);
                }
            }
        }
    }

    /// A flush is writing the stored cells to a file: say once that the weak
    /// bits stay in this process. `true` when this call is the one that said
    /// it.
    pub(crate) fn warn_on_flush(&mut self) -> bool {
        if self.masks.is_empty() || self.flush_warned {
            return false;
        }
        self.flush_warned = true;
        log::warn!(
            "flash: {} weak bit(s) in {} sector(s) are not written to the flash file — they \
             live in this process only; the file holds the stored cells",
            self.count(),
            self.masks.len()
        );
        true
    }

    /// Has a flush said so yet?
    pub fn flush_warned(&self) -> bool {
        self.flush_warned
    }

    fn drop_empty(&mut self) {
        self.masks.retain(|_, m| m.iter().any(|&b| b != 0));
    }
}

fn split(addr: u32) -> (u32, usize) {
    (addr / SECTOR_LEN, (addr % SECTOR_LEN) as usize)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_mask_reads_seeded_noise_and_nothing_else_moves() {
        let mut weak = FlashWeakBits::default();
        weak.reseed(7);
        weak.add(SECTOR_LEN + 2, &[0x0F]);
        assert_eq!(weak.count(), 4);
        assert!(weak.touches(SECTOR_LEN, 4));
        assert!(!weak.touches(0, SECTOR_LEN));

        let read = |weak: &mut FlashWeakBits| {
            let mut out = [0xA0u8; 4];
            weak.apply(SECTOR_LEN, &mut out);
            out
        };
        let mut seen = Vec::new();
        for _ in 0..32 {
            let out = read(&mut weak);
            assert_eq!(out[0], 0xA0);
            assert_eq!(out[1], 0xA0);
            assert_eq!(out[3], 0xA0);
            assert_eq!(out[2] & 0xF0, 0xA0, "only the weak bits move");
            seen.push(out[2]);
        }
        seen.sort_unstable();
        seen.dedup();
        assert!(seen.len() > 1, "a weak cell reads differently: {seen:?}");
    }

    #[test]
    fn the_same_seed_reads_the_same_sequence() {
        let sequence = |seed: u64| {
            let mut weak = FlashWeakBits::default();
            weak.reseed(seed);
            weak.add(0, &[0xFF; 8]);
            (0..16)
                .map(|_| {
                    let mut out = [0u8; 8];
                    weak.apply(0, &mut out);
                    out
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(sequence(3), sequence(3));
        assert_ne!(sequence(3), sequence(4));
    }

    #[test]
    fn a_program_solidifies_its_zeros_and_an_erase_clears_the_sector() {
        let mut weak = FlashWeakBits::default();
        weak.add(0, &[0xFF, 0xFF]);
        weak.add(2 * SECTOR_LEN, &[0x01]);
        weak.solidify(0, &[0x0F, 0xFF]);
        assert_eq!(weak.mask_at(0), 0x0F, "the programmed zeros are solid");
        assert_eq!(weak.mask_at(1), 0xFF, "a 1 left alone stays weak");
        weak.clear(0, SECTOR_LEN);
        assert_eq!(weak.mask_at(0), 0);
        assert_eq!(weak.sectors(), vec![2]);
        weak.solidify(2 * SECTOR_LEN, &[0x00]);
        assert!(weak.is_empty(), "a mask with nothing left in it is dropped");
    }

    #[test]
    fn a_flush_warns_once_and_only_with_weak_bits() {
        let mut weak = FlashWeakBits::default();
        assert!(!weak.warn_on_flush(), "nothing to warn about");
        weak.add(0, &[0x80]);
        assert!(weak.warn_on_flush());
        assert!(!weak.warn_on_flush(), "once");
        assert!(weak.flush_warned());
    }
}
