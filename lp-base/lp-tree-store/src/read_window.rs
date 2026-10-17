//! Reads through a window onto the flash, kept coherent with the store's
//! own programs and erases.
//!
//! On the C6 the cheapest way to read `lpfs` is not a flash read call (the
//! mask ROM's SPI1 routine, ~14 µs a call) but a load from the flash cache:
//! the partition mapped by the MMU, every 32-byte line a cache fill the first
//! time it is touched (the mount study's C7: mount 3.5–5x and file reads
//! 2.7–4x cheaper). A cache knows nothing of the SPI1 writes beside it, so a
//! line it holds goes on serving the bytes it was filled with after the
//! store programs or erases under it. [`WindowedFlash`] is the store's
//! [`Flash`] over such a window, and holds **one rule**: after every program
//! and every erase — returned `Ok` or not — it invalidates exactly the
//! range written, before the store can read again. Nothing else reads or
//! writes the partition (the store is single-threaded and owns it), so no
//! line can be refilled between the write and its invalidate.
//!
//! Why the rule is load-bearing: a stale line read back as erased, after a
//! program, makes a head's tail look blank to the resume check, a settle
//! believe the record it re-programs, and a verify-after-write miss its
//! record; a stale line after an erase makes the erase check retire a good
//! sector. The `mutants` feature's `ReadsWithoutInvalidate` breaks it, and
//! `lp-store-bench` runs its drivers over [`SimLineCache`] (which serves
//! stale lines as a real cache does) to show they catch it.
//!
//! The window's side ([`ReadWindow`]) is the platform's: the C6's MMU
//! window and the ROM's range invalidate (`fw-esp32c6`'s `tree_flash.rs`),
//! or the simulated cache here. Nothing about the format changes.

use crate::flash::Flash;

/// A cached view of a [`Flash`]'s bytes.
pub trait ReadWindow<F: Flash> {
    /// `buf` = the bytes at `addr`, as the window serves them (`flash` is
    /// the part underneath, for a window that reads through it on a miss).
    fn read(&mut self, flash: &mut F, addr: u32, buf: &mut [u8]) -> Result<(), F::Error>;

    /// `[addr, addr + len)` was just programmed or erased: whatever the
    /// window holds of it is stale.
    fn invalidate(&mut self, addr: u32, len: u32);
}

/// No window: every read goes to the flash.
impl<F: Flash, W: ReadWindow<F>> ReadWindow<F> for Option<W> {
    fn read(&mut self, flash: &mut F, addr: u32, buf: &mut [u8]) -> Result<(), F::Error> {
        match self {
            Some(w) => w.read(flash, addr, buf),
            None => flash.read(addr, buf),
        }
    }

    fn invalidate(&mut self, addr: u32, len: u32) {
        if let Some(w) = self {
            w.invalidate(addr, len);
        }
    }
}

/// The store's [`Flash`]: reads through `window`, writes to `flash`, and
/// every write invalidates what it wrote before the next read.
pub struct WindowedFlash<F, W> {
    flash: F,
    window: W,
}

impl<F: Flash, W: ReadWindow<F>> WindowedFlash<F, W> {
    pub fn new(flash: F, window: W) -> Self {
        Self { flash, window }
    }

    /// The part underneath.
    pub fn flash(&self) -> &F {
        &self.flash
    }

    pub fn flash_mut(&mut self) -> &mut F {
        &mut self.flash
    }

    pub fn window(&self) -> &W {
        &self.window
    }

    pub fn into_parts(self) -> (F, W) {
        (self.flash, self.window)
    }

    fn written(&mut self, addr: u32, len: u32) {
        if !mutant!(ReadsWithoutInvalidate) {
            self.window.invalidate(addr, len);
        }
    }
}

impl<F: Flash, W: ReadWindow<F>> Flash for WindowedFlash<F, W> {
    type Error = F::Error;

    fn sector_count(&self) -> u32 {
        self.flash.sector_count()
    }

    fn sector_size(&self) -> u32 {
        self.flash.sector_size()
    }

    fn read(&mut self, addr: u32, buf: &mut [u8]) -> Result<(), Self::Error> {
        self.window.read(&mut self.flash, addr, buf)
    }

    fn program(&mut self, addr: u32, data: &[u8]) -> Result<(), Self::Error> {
        // A program cut short has still changed cells: invalidate either way.
        let r = self.flash.program(addr, data);
        self.written(addr, data.len() as u32);
        r
    }

    fn erase_sector(&mut self, sector: u32) -> Result<(), Self::Error> {
        let r = self.flash.erase_sector(sector);
        let size = self.flash.sector_size();
        self.written(sector * size, size);
        r
    }

    fn crc32(&self, crc: u32, bytes: &[u8]) -> u32 {
        self.flash.crc32(crc, bytes)
    }
}

/// A host model of the C6's flash cache over the store's reads: 32 KiB,
/// 4-way set-associative, 32-byte lines, LRU, holding the bytes each line
/// was filled with — so a line not invalidated after a write serves its old
/// bytes, as silicon does. Counts the lines reads touch and the fills they
/// cause (what C7's cost model prices). Data only: the code the C6 runs
/// from the same cache would only evict these lines sooner.
#[cfg(any(test, feature = "nor-sim"))]
pub struct SimLineCache {
    /// Per set, up to four `(line, bytes)`, most recently used first.
    sets: alloc::vec::Vec<alloc::vec::Vec<(u32, [u8; LINE])>>,
    /// Lines reads touched, and fills (misses).
    pub lines: u64,
    pub fills: u64,
}

#[cfg(any(test, feature = "nor-sim"))]
const LINE: usize = 32;
#[cfg(any(test, feature = "nor-sim"))]
const SETS: usize = 32 * 1024 / LINE / WAYS;
#[cfg(any(test, feature = "nor-sim"))]
const WAYS: usize = 4;

#[cfg(any(test, feature = "nor-sim"))]
impl Default for SimLineCache {
    fn default() -> Self {
        Self {
            sets: alloc::vec![alloc::vec::Vec::new(); SETS],
            lines: 0,
            fills: 0,
        }
    }
}

#[cfg(any(test, feature = "nor-sim"))]
impl<F: Flash> ReadWindow<F> for SimLineCache {
    fn read(&mut self, flash: &mut F, addr: u32, buf: &mut [u8]) -> Result<(), F::Error> {
        let mut at = addr;
        let mut out = 0;
        while out < buf.len() {
            let line = at / LINE as u32;
            let skip = (at % LINE as u32) as usize;
            let take = (LINE - skip).min(buf.len() - out);
            let set = &mut self.sets[line as usize % SETS];
            self.lines += 1;
            let bytes = match set.iter().position(|e| e.0 == line) {
                Some(i) => {
                    let e = set.remove(i);
                    set.insert(0, e);
                    set[0].1
                }
                None => {
                    let mut b = [0u8; LINE];
                    flash.read(line * LINE as u32, &mut b)?;
                    self.fills += 1;
                    set.truncate(WAYS - 1);
                    set.insert(0, (line, b));
                    b
                }
            };
            buf[out..out + take].copy_from_slice(&bytes[skip..skip + take]);
            out += take;
            at += take as u32;
        }
        Ok(())
    }

    fn invalidate(&mut self, addr: u32, len: u32) {
        let first = addr / LINE as u32;
        let last = (addr + len.max(1) - 1) / LINE as u32;
        for set in &mut self.sets {
            set.retain(|e| e.0 < first || e.0 > last);
        }
    }
}

#[cfg(test)]
mod tests {
    use lp_nor_sim::{NorFlashSim, NorGeometry};

    use super::*;

    #[test]
    fn a_line_serves_its_old_bytes_until_invalidated() {
        let mut f = WindowedFlash::new(
            NorFlashSim::new(NorGeometry::c6(4)),
            SimLineCache::default(),
        );
        let mut b = [0u8; 8];
        f.read(100, &mut b).unwrap();
        assert_eq!(b, [0xFF; 8]);
        // Behind the window's back: the line still says erased.
        f.flash_mut().program(100, &[1, 2, 3]).unwrap();
        f.read(100, &mut b).unwrap();
        assert_eq!(b, [0xFF; 8]);
        // Through it: invalidated, read fresh.
        Flash::program(&mut f, 104, &[9]).unwrap();
        f.read(100, &mut b).unwrap();
        assert_eq!(b[..5], [1, 2, 3, 0xFF, 9]);
        assert_eq!(f.window().fills, 2);
        Flash::erase_sector(&mut f, 0).unwrap();
        f.read(100, &mut b).unwrap();
        assert_eq!(b, [0xFF; 8]);
    }

    #[test]
    fn reads_across_lines_and_sets() {
        let mut f = WindowedFlash::new(
            NorFlashSim::new(NorGeometry::c6(32)),
            SimLineCache::default(),
        );
        let data: alloc::vec::Vec<u8> = (0..9000u32).map(|i| (i * 7) as u8).collect();
        Flash::program(&mut f, 1000, &data).unwrap();
        let mut back = alloc::vec![0u8; 9000];
        f.read(1000, &mut back).unwrap();
        assert_eq!(back, data);
        // Twice the cache's size apart: the same set, evicted by then.
        let mut b = [0u8; 4];
        f.read(1000 + 64 * 1024, &mut b).unwrap();
        f.read(1000, &mut b).unwrap();
        assert_eq!(b, data[..4]);
    }
}
