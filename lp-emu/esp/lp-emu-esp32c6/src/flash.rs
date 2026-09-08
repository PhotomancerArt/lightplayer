//! The SPI NOR flash chip on the other side of SPI1.
//!
//! A byte image plus the three things a NOR flash actually does, and nothing
//! else: **read** returns bytes, **program** can only turn ones into zeros,
//! and **erase** is the only way back to `0xff`. Modelling program as a plain
//! copy would hide the single most common flash bug there is (writing a
//! sector twice without erasing it), so it is an `&=`, and a test says so.
//!
//! # Identity
//!
//! Two consumers ask this chip how big it is, by two different routes, and
//! they have to agree:
//!
//! - `esp_storage`'s `get_flash_size()` drives SPI1's `flash_rdid` bit
//!   directly and decodes the JEDEC id's capacity byte
//!   (`third_party/esp-storage/src/hardware.rs`).
//! - the mask ROM's `SPI_read_data` refuses any read past
//!   `rom_spiflash_legacy_data->chip_size` (`_esp_rom_spiflash_read` →
//!   `SPI_read_data`, `40024100`: `lw a5,4(a0); bltu a5,a4 → return 1`).
//!
//! So [`FlashImage::jedec_id`] and [`crate::loader::seed_rom_flash_chip`]
//! are derived from the same [`FlashImage::len`], and a test pins that the
//! capacity byte and the chip-size word describe the same number of bytes.
//! The manufacturer and memory-type bytes are the ROM's own default chip
//! (`rom_default_spiflash_legacy_data` at `0x4087_fa08` is
//! `device_id = 0x0015_40ef`: Winbond `0xef`, type `0x40`, capacity `0x15` =
//! 2 MiB), so the only thing this model changes about the ROM's idea of the
//! part is its size — which is what the second-stage bootloader changes on
//! silicon, from the flash-size field of the image header.
//!
//! # Persistence
//!
//! [`FlashBacking`] says where the bytes came from and whether they go back:
//! `--flash <file>` is read-write (the board's flash, surviving a run),
//! `--flash-copy <file>` reads once and never writes (a scratch copy of a
//! known image), and the default is a blank chip that lives and dies with
//! the process.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// The flash size the C6 boards ship with, and the size
/// `lp-fw/fw-esp32c6/partitions.csv` fills exactly (`lpfs` ends at
/// `0x310000 + 0xF0000 = 0x400000`).
pub const DEFAULT_FLASH_LEN: u32 = 4 * 1024 * 1024;

/// A 4 KiB flash sector — the erase granule, and littlefs's block size
/// (`fw-esp32c6/src/flash_storage.rs`: `BLOCK_SIZE = 4096`).
pub const SECTOR_LEN: u32 = 4096;

/// A 64 KiB block — the coarse erase granule and the cache MMU's page size.
pub const BLOCK_LEN: u32 = 64 * 1024;

/// A 256-byte page — the most a single page-program may touch.
pub const PAGE_LEN: u32 = 256;

/// The `factory` partition's offset (`partitions.csv`), where a flashed app
/// image starts.
pub const FACTORY_OFFSET: u32 = 0x0001_0000;

/// The `lpfs` partition (`partitions.csv`, and hardcoded in
/// `fw-esp32c6/src/flash_storage.rs` as `LPFS_PARTITION_OFFSET`).
pub const LPFS_OFFSET: u32 = 0x0031_0000;
pub const LPFS_LEN: u32 = 0x000F_0000;

/// Where a flash image's bytes come from, and whether they go back.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum FlashBacking {
    /// A blank chip that lives and dies with the process.
    #[default]
    Blank,
    /// `--flash <file>`: read at start, written back at [`FlashImage::flush`].
    /// A file that does not exist is created blank.
    File(PathBuf),
    /// `--flash-copy <file>`: read at start, never written.
    Copy(PathBuf),
}

/// The chip's bytes.
#[derive(Debug)]
pub struct FlashImage {
    bytes: Vec<u8>,
    backing: FlashBacking,
    /// Set by every [`program`](Self::program) and [`erase`](Self::erase);
    /// cleared by [`flush`](Self::flush). What makes a run that never wrote
    /// leave the file's mtime alone.
    dirty: bool,
    /// Every 64 KiB block a write has touched since
    /// [`take_written_blocks`](Self::take_written_blocks) was last called.
    /// The cache-fill path reads it: a flash write under a mapped page has
    /// to reach the window (see [`crate::cache`]).
    written_blocks: Vec<u32>,
    /// Counters for the run summary and for the gate that compares this
    /// machine's flash traffic with esp-emu's 14,169 decoded commands.
    pub reads: u64,
    pub programs: u64,
    pub sector_erases: u64,
    pub block_erases: u64,
    pub write_enables: u64,
    pub status_reads: u64,
}

/// A flash image several owners hold: SPI1 executes commands against it and
/// the cache fill reads through it.
pub type FlashHandle = Arc<Mutex<FlashImage>>;

impl FlashImage {
    /// A blank chip of `len` bytes — erased flash is all ones.
    pub fn blank(len: u32) -> Self {
        Self {
            bytes: vec![0xff; len as usize],
            backing: FlashBacking::Blank,
            dirty: false,
            written_blocks: Vec::new(),
            reads: 0,
            programs: 0,
            sector_erases: 0,
            block_erases: 0,
            write_enables: 0,
            status_reads: 0,
        }
    }

    /// Open `backing`, padding a short file with `0xff` up to `len` and
    /// refusing one that is longer.
    ///
    /// A missing `File` is created blank at `len`: "point `--flash` at a path
    /// and get a board with an empty chip" is the loop this milestone's
    /// second-boot gate runs in, and making the user pre-create a 4 MiB file
    /// of `0xff` would be a step with no meaning.
    pub fn open(backing: FlashBacking, len: u32) -> io::Result<Self> {
        let mut image = Self::blank(len);
        let path: Option<&Path> = match &backing {
            FlashBacking::Blank => None,
            FlashBacking::File(p) | FlashBacking::Copy(p) => Some(p.as_path()),
        };
        if let Some(path) = path
            && path.exists()
        {
            let bytes = std::fs::read(path)?;
            if bytes.len() > len as usize {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "{} is {} bytes, larger than the {len}-byte chip this machine models",
                        path.display(),
                        bytes.len()
                    ),
                ));
            }
            image.bytes[..bytes.len()].copy_from_slice(&bytes);
        }
        image.backing = backing;
        Ok(image)
    }

    pub fn len(&self) -> u32 {
        self.bytes.len() as u32
    }

    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    pub fn backing(&self) -> &FlashBacking {
        &self.backing
    }

    pub fn dirty(&self) -> bool {
        self.dirty
    }

    /// The JEDEC id `flash_rdid` returns, in the layout esp-storage decodes:
    /// byte 0 manufacturer, byte 1 memory type, byte 2 capacity as
    /// `log2(bytes)`.
    ///
    /// `0x0016_40ef` for the 4 MiB part. The manufacturer and type are the
    /// ROM's own default chip; the capacity is this image's real size, so a
    /// `--flash` of another size is reported honestly rather than pretended
    /// about.
    pub fn jedec_id(&self) -> u32 {
        let capacity = u32::from(self.len().trailing_zeros() as u8);
        0x0000_40ef | (capacity << 16)
    }

    /// `len` bytes at `addr`, or `None` if the range leaves the chip.
    pub fn read(&mut self, addr: u32, len: u32) -> Option<&[u8]> {
        let end = addr.checked_add(len)?;
        if end > self.len() {
            return None;
        }
        self.reads += 1;
        Some(&self.bytes[addr as usize..end as usize])
    }

    /// Read without counting it — what the cache fill uses, so the command
    /// census stays a census of what the *guest* asked SPI1 to do.
    pub fn peek(&self, addr: u32, len: u32) -> Option<&[u8]> {
        let end = addr.checked_add(len)?;
        if end > self.len() {
            return None;
        }
        Some(&self.bytes[addr as usize..end as usize])
    }

    /// Page-program: `data` is ANDed into the image, because a NOR flash
    /// cell can only be driven from 1 to 0. Programming over unerased bytes
    /// therefore corrupts them here exactly as it does on the part.
    ///
    /// Returns `false` if the range leaves the chip.
    pub fn program(&mut self, addr: u32, data: &[u8]) -> bool {
        let Some(end) = addr.checked_add(data.len() as u32) else {
            return false;
        };
        if end > self.len() {
            return false;
        }
        for (i, byte) in data.iter().enumerate() {
            self.bytes[addr as usize + i] &= byte;
        }
        self.programs += 1;
        self.note_write(addr, data.len() as u32);
        true
    }

    /// Erase `len` bytes at `addr` back to `0xff`. `addr` must be a multiple
    /// of `len` and `len` one of the granules the part supports; the caller
    /// (SPI1) has already decided which command this is.
    pub fn erase(&mut self, addr: u32, len: u32) -> bool {
        let Some(end) = addr.checked_add(len) else {
            return false;
        };
        if end > self.len() || !addr.is_multiple_of(len) {
            return false;
        }
        self.bytes[addr as usize..end as usize].fill(0xff);
        match len {
            SECTOR_LEN => self.sector_erases += 1,
            BLOCK_LEN => self.block_erases += 1,
            _ => {}
        }
        self.note_write(addr, len);
        true
    }

    /// Erase the whole chip.
    pub fn erase_chip(&mut self) {
        self.bytes.fill(0xff);
        let len = self.len();
        self.note_write(0, len);
    }

    fn note_write(&mut self, addr: u32, len: u32) {
        self.dirty = true;
        let first = addr / BLOCK_LEN;
        let last = (addr + len.saturating_sub(1)) / BLOCK_LEN;
        for block in first..=last {
            if !self.written_blocks.contains(&block) {
                self.written_blocks.push(block);
            }
        }
    }

    /// The 64 KiB blocks written since the last call, and clear the list.
    pub fn take_written_blocks(&mut self) -> Vec<u32> {
        core::mem::take(&mut self.written_blocks)
    }

    /// Write the image back if the backing says to and anything changed.
    /// Returns `true` if a file was written.
    pub fn flush(&mut self) -> io::Result<bool> {
        let FlashBacking::File(path) = &self.backing else {
            return Ok(false);
        };
        if !self.dirty && path.exists() {
            return Ok(false);
        }
        std::fs::write(path, &self.bytes)?;
        self.dirty = false;
        Ok(true)
    }

    /// Place bytes without going through a flash command — what the loader
    /// does when it stages the app image, which on a board is what the
    /// flasher did before the board was ever powered on. Not a program:
    /// the bytes replace, they are not ANDed, because a flasher erases
    /// first.
    pub fn stage(&mut self, addr: u32, data: &[u8]) -> bool {
        let Some(end) = addr.checked_add(data.len() as u32) else {
            return false;
        };
        if end > self.len() {
            return false;
        }
        self.bytes[addr as usize..end as usize].copy_from_slice(data);
        self.note_write(addr, data.len() as u32);
        true
    }

    /// The command census, for the run summary.
    pub fn command_census(&self) -> FlashCensus {
        FlashCensus {
            reads: self.reads,
            programs: self.programs,
            sector_erases: self.sector_erases,
            block_erases: self.block_erases,
            write_enables: self.write_enables,
            status_reads: self.status_reads,
        }
    }
}

/// What the guest asked the flash to do over a run. The spike report's §8
/// inventory has esp-emu's figures for the same walk (14,169 decoded `USR`
/// commands: 13,528 reads, 634 page programs, 29 sector erases, 664
/// write-enables), which is what this is comparable to.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FlashCensus {
    pub reads: u64,
    pub programs: u64,
    pub sector_erases: u64,
    pub block_erases: u64,
    pub write_enables: u64,
    pub status_reads: u64,
}

impl FlashCensus {
    /// Reads + programs + erases + write-enables — the four kinds esp-emu's
    /// inventory counted. Status polls are excluded because its trace did
    /// not decode them as `USR` commands.
    pub fn commands(&self) -> u64 {
        self.reads + self.programs + self.sector_erases + self.block_erases + self.write_enables
    }
}

impl std::fmt::Display for FlashCensus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} commands ({} reads, {} page programs, {} sector erases, {} block erases, \
             {} write-enables; {} status polls)",
            self.commands(),
            self.reads,
            self.programs,
            self.sector_erases,
            self.block_erases,
            self.write_enables,
            self.status_reads
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_blank_chip_is_all_ones_and_knows_its_own_capacity() {
        let f = FlashImage::blank(DEFAULT_FLASH_LEN);
        assert_eq!(f.len(), 0x0040_0000);
        assert!(f.peek(0, 16).unwrap().iter().all(|b| *b == 0xff));
        // esp-storage decodes byte 2 as the capacity exponent: 0x16 = 4 MiB.
        assert_eq!(f.jedec_id(), 0x0016_40ef);
        let [manufacturer, memory_type, capacity, _] = f.jedec_id().to_le_bytes();
        assert_eq!((manufacturer, memory_type, capacity), (0xef, 0x40, 0x16));
        assert_eq!(1u32 << capacity, f.len());
        // The ROM's own default part, for comparison: 2 MiB, capacity 0x15.
        assert_eq!(FlashImage::blank(2 * 1024 * 1024).jedec_id(), 0x0015_40ef);
    }

    #[test]
    fn a_program_can_only_clear_bits_and_an_erase_is_the_way_back() {
        let mut f = FlashImage::blank(SECTOR_LEN * 2);
        assert!(f.program(0, &[0xf0, 0x0f]));
        assert_eq!(f.peek(0, 2).unwrap(), &[0xf0, 0x0f]);
        // Programming again without erasing ANDs: this is the bug the model
        // must reproduce, not smooth over.
        assert!(f.program(0, &[0x3c, 0x3c]));
        assert_eq!(f.peek(0, 2).unwrap(), &[0x30, 0x0c]);
        assert!(f.erase(0, SECTOR_LEN));
        assert_eq!(f.peek(0, 2).unwrap(), &[0xff, 0xff]);
        // And an erase must be granule-aligned.
        assert!(!f.erase(1, SECTOR_LEN));
        assert_eq!(f.sector_erases, 1);
        assert_eq!(f.programs, 2);
    }

    #[test]
    fn a_range_that_leaves_the_chip_is_refused_rather_than_wrapped() {
        let mut f = FlashImage::blank(SECTOR_LEN);
        assert!(f.read(SECTOR_LEN - 4, 4).is_some());
        assert!(f.read(SECTOR_LEN - 4, 8).is_none());
        assert!(f.read(u32::MAX, 4).is_none());
        assert!(!f.program(SECTOR_LEN - 1, &[0, 0]));
        assert!(!f.erase(SECTOR_LEN, SECTOR_LEN));
    }

    #[test]
    fn writes_are_reported_by_the_sixty_four_kib_block_the_cache_pages_by() {
        let mut f = FlashImage::blank(DEFAULT_FLASH_LEN);
        assert!(f.take_written_blocks().is_empty());
        assert!(f.program(BLOCK_LEN - 2, &[0, 0, 0, 0]));
        assert_eq!(f.take_written_blocks(), vec![0, 1]);
        assert!(f.take_written_blocks().is_empty());
        assert!(f.erase(LPFS_OFFSET, SECTOR_LEN));
        assert_eq!(f.take_written_blocks(), vec![LPFS_OFFSET / BLOCK_LEN]);
    }

    #[test]
    fn a_file_backing_round_trips_and_a_copy_never_writes() {
        let dir = std::env::temp_dir().join(format!("lp-emu-flash-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("flash.bin");
        let _ = std::fs::remove_file(&path);

        let mut f = FlashImage::open(FlashBacking::File(path.clone()), SECTOR_LEN * 4).unwrap();
        assert!(f.program(0x10, b"hello"));
        assert!(f.flush().unwrap());
        assert_eq!(std::fs::metadata(&path).unwrap().len(), 4 * 4096);

        let mut second =
            FlashImage::open(FlashBacking::File(path.clone()), SECTOR_LEN * 4).unwrap();
        assert_eq!(second.peek(0x10, 5).unwrap(), b"hello");
        assert!(second.program(0x20, b"world"));
        // A copy backing reads the same bytes and refuses to write them back.
        let mut copy = FlashImage::open(FlashBacking::Copy(path.clone()), SECTOR_LEN * 4).unwrap();
        assert_eq!(copy.peek(0x10, 5).unwrap(), b"hello");
        assert!(copy.program(0x30, b"scratch"));
        assert!(!copy.flush().unwrap());
        let on_disk = std::fs::read(&path).unwrap();
        assert_eq!(
            &on_disk[0x30..0x37],
            &[0xff; 7],
            "the copy stayed in memory"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_file_longer_than_the_modeled_chip_is_an_error_not_a_truncation() {
        let dir = std::env::temp_dir().join(format!("lp-emu-flash-big-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("big.bin");
        std::fs::write(&path, vec![0u8; 8193]).unwrap();
        let err = FlashImage::open(FlashBacking::File(path), 8192).unwrap_err();
        assert!(err.to_string().contains("larger than"), "{err}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_census_sums_the_four_kinds_and_leaves_status_polls_out() {
        // The spike report §8's esp-emu figures for the `examples/basic`
        // walk, entered verbatim. Note what the arithmetic says: the report
        // gives the total as **14,169** and the breakdown as 13,528 reads +
        // 634 page programs + 29 sector erases + 664 write-enables, which
        // sums to 14,855. The report's total and its own breakdown disagree
        // by 686; neither number is ours to correct, so the census reports
        // both halves and the gate compares the breakdown, not the total.
        let census = FlashCensus {
            reads: 13_528,
            programs: 634,
            sector_erases: 29,
            block_erases: 0,
            write_enables: 664,
            status_reads: 9_000,
        };
        assert_eq!(census.commands(), 14_855);
        assert_eq!(census.commands() - 14_169, 686);
    }
}
