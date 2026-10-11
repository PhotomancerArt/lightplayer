//! The tree store's [`lp_tree_store::Flash`] over any `embedded-storage`
//! NOR part that only takes word-aligned work (plan
//! `2026-10-08-2339-tree-store-firmware-and-emulator`, D10).
//!
//! The store writes records at **any byte offset** and reads them back the
//! same way. The C6's esp-storage (built with `panic-unaligned-buffer`,
//! without `bytewise-read`) refuses an offset or length that is not a
//! multiple of 4 (`NotAligned`) and **panics** on a buffer whose address is
//! not. So every call goes through a word-aligned bounce buffer on the
//! stack ([`BOUNCE`] bytes):
//!
//! - **read**: the aligned span around the request is read into the bounce
//!   buffer, a chunk at a time, and the wanted bytes copied out;
//! - **program**: the head and tail are padded to word boundaries with
//!   `0xFF` (programming a 1 changes no NOR cell, and a torn program only
//!   ever clears bits toward what it was programming) and the bounce buffer
//!   is written with `NorFlash::write` — never `embedded_storage::Storage::
//!   write`, which is read-erase-write of a whole sector and would be fatal
//!   for a log store;
//! - **erase**: one sector, already aligned.
//!
//! So on the device every program reaches the ROM word-aligned and a
//! multiple of 4 bytes long (M4's owed "unaligned program" sitting should
//! know: the store's unaligned records never reach the ROM as such).
//!
//! The store's addresses start at 0 for its first sector; this adds the
//! partition's offset. Any error from the part is passed up as is: to the
//! store it means power was lost, and the store is dropped.
//!
//! Generic over the part so it runs on the host against `lp-nor-sim` (the
//! tests below); `fw-esp32c6`'s `tree_flash.rs` instantiates it over
//! `esp_storage::FlashStorage`. Chip-free, as this crate must be.

use embedded_storage::nor_flash::NorFlash;

/// Bytes of the stack bounce buffer (a multiple of the word).
pub const BOUNCE: usize = 256;

const WORD: u32 = 4;

/// The store's flash: `sector_count` sectors of `sector_size` bytes at
/// `offset` on `part`, every call word-aligned.
pub struct AlignedNorFlash<N: NorFlash> {
    part: N,
    offset: u32,
    sector_size: u32,
    sector_count: u32,
}

impl<N: NorFlash> AlignedNorFlash<N> {
    /// `offset` and `len` must be multiples of `sector_size`; `sector_size`
    /// of the part's erase size.
    pub fn new(part: N, offset: u32, len: u32, sector_size: u32) -> Self {
        debug_assert!(offset.is_multiple_of(sector_size) && len.is_multiple_of(sector_size));
        debug_assert!(sector_size.is_multiple_of(N::ERASE_SIZE as u32));
        Self {
            part,
            offset,
            sector_size,
            sector_count: len / sector_size,
        }
    }

    /// The part underneath (a probe elsewhere on the same chip).
    pub fn part_mut(&mut self) -> &mut N {
        &mut self.part
    }

    /// The partition's first byte on the part.
    pub fn offset(&self) -> u32 {
        self.offset
    }
}

/// A word-aligned scratch buffer: its address is a multiple of 4.
#[repr(C, align(4))]
struct Bounce([u8; BOUNCE]);

impl<N: NorFlash> lp_tree_store::Flash for AlignedNorFlash<N> {
    type Error = N::Error;

    fn sector_count(&self) -> u32 {
        self.sector_count
    }

    fn sector_size(&self) -> u32 {
        self.sector_size
    }

    fn read(&mut self, addr: u32, buf: &mut [u8]) -> Result<(), Self::Error> {
        let mut bounce = Bounce([0; BOUNCE]);
        let mut at = self.offset + addr;
        let mut out = buf;
        while !out.is_empty() {
            let base = at & !(WORD - 1);
            let skip = (at - base) as usize;
            let want = out.len().min(BOUNCE - skip);
            let span = (skip + want).next_multiple_of(WORD as usize);
            self.part.read(base, &mut bounce.0[..span])?;
            out[..want].copy_from_slice(&bounce.0[skip..skip + want]);
            out = &mut out[want..];
            at += want as u32;
        }
        Ok(())
    }

    fn program(&mut self, addr: u32, data: &[u8]) -> Result<(), Self::Error> {
        let mut bounce = Bounce([0; BOUNCE]);
        let mut at = self.offset + addr;
        let mut rest = data;
        while !rest.is_empty() {
            let base = at & !(WORD - 1);
            let skip = (at - base) as usize;
            let take = rest.len().min(BOUNCE - skip);
            let span = (skip + take).next_multiple_of(WORD as usize);
            let chunk = &mut bounce.0[..span];
            chunk.fill(0xFF);
            chunk[skip..skip + take].copy_from_slice(&rest[..take]);
            self.part.write(base, chunk)?;
            rest = &rest[take..];
            at += take as u32;
        }
        Ok(())
    }

    fn erase_sector(&mut self, sector: u32) -> Result<(), Self::Error> {
        let from = self.offset + sector * self.sector_size;
        self.part.erase(from, from + self.sector_size)
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use std::vec;
    use std::vec::Vec;

    use embedded_storage::nor_flash::{ErrorType, NorFlash, NorFlashErrorKind, ReadNorFlash};
    use lp_nor_sim::{NorFlashSim, NorGeometry};
    use lp_tree_store::{Flash, SoftSha256, StoreConfig, TreeStore};

    use super::*;

    #[test]
    fn odd_offsets_lengths_and_buffer_addresses_read_and_program_exactly() {
        let mut f = flash();
        // Every start in a word, every length up to past the bounce buffer,
        // from a buffer at every address in a word.
        let pattern: Vec<u8> = (0..700u32).map(|i| (i * 37 + 11) as u8).collect();
        for start in [0u32, 1, 2, 3, 4097, 8190] {
            for len in [1usize, 2, 3, 4, 5, 63, 255, 256, 257, 600] {
                for shift in 0..4 {
                    let base = 3 * 4096 + start;
                    f.erase_sector(3).unwrap();
                    f.erase_sector(4).unwrap();
                    f.erase_sector(5).unwrap();
                    let mut src = vec![0u8; len + shift];
                    src[shift..].copy_from_slice(&pattern[..len]);
                    f.program(base, &src[shift..]).unwrap();
                    let mut dst = vec![0u8; len + shift];
                    f.read(base, &mut dst[shift..]).unwrap();
                    assert_eq!(
                        &dst[shift..],
                        &pattern[..len],
                        "start {start} len {len} shift {shift}"
                    );
                    // The bytes either side stay erased: the padding was 0xFF.
                    let mut edge = [0u8; 2];
                    if base >= 3 * 4096 + 2 {
                        f.read(base - 2, &mut edge).unwrap();
                        assert_eq!(edge, [0xFF; 2]);
                    }
                    f.read(base + len as u32, &mut edge).unwrap();
                    assert_eq!(edge, [0xFF; 2]);
                }
            }
        }
    }

    #[test]
    fn a_program_beside_programmed_bytes_leaves_them_as_they_were() {
        let mut f = flash();
        f.program(10, &[0x12, 0x34, 0x56]).unwrap();
        f.program(13, &[0x78]).unwrap();
        f.program(7, &[0x9A, 0xBC, 0xDE]).unwrap();
        let mut got = [0u8; 8];
        f.read(7, &mut got).unwrap();
        assert_eq!(got, [0x9A, 0xBC, 0xDE, 0x12, 0x34, 0x56, 0x78, 0xFF]);
    }

    #[test]
    fn the_partition_offset_is_added() {
        let mut f = flash();
        f.program(0, b"lpts").unwrap();
        let mut raw = [0u8; 4];
        f.part.0.read(OFFSET, &mut raw).unwrap();
        assert_eq!(&raw, b"lpts");
        f.erase_sector(0).unwrap();
        f.part.0.read(OFFSET, &mut raw).unwrap();
        assert_eq!(raw, [0xFF; 4]);
        assert_eq!((f.sector_count(), f.sector_size()), (SECTORS, 4096));
    }

    /// The whole store over the shim: every call it makes reaches the part
    /// aligned (the strict part below refuses anything else the way
    /// esp-storage does), and its files read back across a remount.
    #[test]
    fn the_store_runs_on_a_part_that_refuses_unaligned_work() {
        let c = StoreConfig::default();
        let st = match TreeStore::format(flash(), SoftSha256, c.clone()) {
            Ok(st) => st,
            Err((e, ..)) => panic!("format: {e:?}"),
        };
        let mut st = st;
        let odd: Vec<u8> = (0..5_003u32).map(|i| (i % 251) as u8).collect();
        st.put("/projects/a/big.bin", &odd).unwrap();
        st.put("/x", b"q").unwrap();
        st.append("/x", b"rs").unwrap();
        st.put("/projects/a/.lp/panel.json", b"{\"v\":1}").unwrap();
        let f = st.into_flash();
        assert!(f.part.1 > 100, "{} calls", f.part.1);
        let mut st = match TreeStore::mount(f, SoftSha256, c) {
            Ok(st) => st,
            Err((e, ..)) => panic!("mount: {e:?}"),
        };
        assert_eq!(st.get("/projects/a/big.bin").unwrap().unwrap(), odd);
        assert_eq!(st.get("/x").unwrap().unwrap(), b"qrs");
        assert!(matches!(st.get("/nope"), Ok(None)));
    }

    // ---- helpers -----------------------------------------------------------

    const OFFSET: u32 = 8 * 4096;
    const SECTORS: u32 = 16;

    fn flash() -> AlignedNorFlash<Strict> {
        let mut sim = NorFlashSim::new(NorGeometry::c6(SECTORS + 8));
        // The shim's 0xFF padding lands on bytes a neighbouring record
        // already programmed: NOR leaves a programmed cell where it is when
        // asked for a 1, and the model, built to catch a writer asking for
        // 0 -> 1 on purpose, would panic on it. The tests below check that
        // those bytes read back unchanged instead.
        sim.set_panic_on_violation(false);
        let part = Strict(sim, 0);
        AlignedNorFlash::new(part, OFFSET, SECTORS * 4096, 4096)
    }

    /// `lp-nor-sim` behind esp-storage's rules: an offset, a length or a
    /// buffer address that is not a multiple of 4 is a test failure (the
    /// device would answer `NotAligned`, or panic). Counts the calls.
    struct Strict(NorFlashSim, u32);

    #[derive(Debug)]
    struct StrictError;

    impl embedded_storage::nor_flash::NorFlashError for StrictError {
        fn kind(&self) -> NorFlashErrorKind {
            NorFlashErrorKind::Other
        }
    }

    impl ErrorType for Strict {
        type Error = StrictError;
    }

    fn aligned(offset: u32, len: usize, ptr: *const u8) {
        assert!(offset.is_multiple_of(4), "offset {offset:#x}");
        assert!(len.is_multiple_of(4), "length {len}");
        assert!((ptr as usize).is_multiple_of(4), "buffer at {ptr:p}");
    }

    impl ReadNorFlash for Strict {
        const READ_SIZE: usize = 4;
        fn read(&mut self, offset: u32, bytes: &mut [u8]) -> Result<(), StrictError> {
            aligned(offset, bytes.len(), bytes.as_ptr());
            self.1 += 1;
            ReadNorFlash::read(&mut self.0, offset, bytes).map_err(|_| StrictError)
        }
        fn capacity(&self) -> usize {
            ReadNorFlash::capacity(&self.0)
        }
    }

    impl NorFlash for Strict {
        const WRITE_SIZE: usize = 4;
        const ERASE_SIZE: usize = 4096;
        fn erase(&mut self, from: u32, to: u32) -> Result<(), StrictError> {
            assert!(from.is_multiple_of(4096) && to.is_multiple_of(4096));
            self.1 += 1;
            NorFlash::erase(&mut self.0, from, to).map_err(|_| StrictError)
        }
        fn write(&mut self, offset: u32, bytes: &[u8]) -> Result<(), StrictError> {
            aligned(offset, bytes.len(), bytes.as_ptr());
            self.1 += 1;
            NorFlash::write(&mut self.0, offset, bytes).map_err(|_| StrictError)
        }
    }
}
