//! The SPI-NOR flash chip on the other side of a flash controller, and the
//! command engine that walks its transactions.
//!
//! Two halves, and they are what every Espressif part boots off:
//!
//! - **the chip** ([`FlashImage`]): a byte image plus the three things a NOR
//!   flash actually does. **Read** returns bytes, **program** can only turn
//!   ones into zeros, and **erase** is the only way back to `0xff`. Modelling
//!   program as a plain copy would hide the single most common flash bug
//!   there is (writing a sector twice without erasing it), so it is an `&=`,
//!   and a test says so.
//! - **the engine** ([`FlashEngine`]): the flash side of a controller's
//!   command word — the WIP/WEL status latch, the SPI-NOR command set the
//!   `usr` engine carries in its command phase, and the operations
//!   themselves against the chip.
//!
//! **Behaviour only.** Not one register offset, not one trigger-bit number,
//! not one reset value, and not one part's default size: the chip's view
//! decodes its own `cmd`, `user`, `user1`, `user2`, `addr` and length
//! registers, gathers its own data buffer, and hands the engine a
//! [`FlashOp`]. What comes back is a [`FlashOutcome`] the view places where
//! its own registers live — never "the engine wrote `w0`", because the
//! engine does not know where `w0` is.
//!
//! # Why this is an engine
//!
//! Every chip in this emulator boots off SPI-NOR, and the chip model carries
//! file-backed persistence (three distinct policies) that a second view would
//! have to re-derive exactly for its transcripts to mean anything. The
//! controller's trigger bits happen to agree across two generations of the
//! part; that is a coincidence of one IP, not a contract, and the bit
//! numbers stay in the views that read them.
//!
//! # Identity
//!
//! Two consumers ask this chip how big it is, by two different routes, and
//! they have to agree:
//!
//! - `esp_storage`'s `get_flash_size()` drives the controller's `flash_rdid`
//!   trigger directly and decodes the JEDEC id's capacity byte
//!   (`third_party/esp-storage/src/hardware.rs`).
//! - the mask ROM's `SPI_read_data` refuses any read past
//!   `rom_spiflash_legacy_data->chip_size` (on the C6, `_esp_rom_spiflash_read`
//!   → `SPI_read_data`, `40024100`: `lw a5,4(a0); bltu a5,a4 → return 1`).
//!
//! So [`FlashImage::jedec_id`] and whatever the chip crate seeds the ROM's
//! chip-size word from are derived from the same [`FlashImage::len`], and a
//! test pins that the capacity byte and the chip-size word describe the same
//! number of bytes. The manufacturer and memory-type bytes are the ROM's own
//! default chip (the C6's `rom_default_spiflash_legacy_data` at
//! `0x4087_fa08` is `device_id = 0x0015_40ef`: Winbond `0xef`, type `0x40`,
//! capacity `0x15` = 2 MiB), so the only thing this model changes about the
//! ROM's idea of the part is its size — which is what the second-stage
//! bootloader changes on silicon, from the flash-size field of the image
//! header. Those two bytes are the same structure in the classic ESP32's ROM;
//! if a part ever turns out to default to a different manufacturer, this is
//! where a parameter goes.
//!
//! # Persistence
//!
//! [`FlashBacking`] says where the bytes came from and whether they go back:
//! `--flash <file>` is read-write (the board's flash, surviving a run),
//! `--flash-copy <file>` reads once and never writes (a scratch copy of a
//! known image), and the default is a blank chip that lives and dies with
//! the process.

use alloc::format;
use alloc::vec;
use alloc::vec::Vec;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::periph::BusCx;

/// A 4 KiB flash sector — the erase granule, and littlefs's block size.
pub const SECTOR_LEN: u32 = 4096;

/// A 64 KiB block — the coarse erase granule, and the page size of the cache
/// MMUs that map flash into an instruction window.
pub const BLOCK_LEN: u32 = 64 * 1024;

/// A 256-byte page — the most a single page-program may touch.
pub const PAGE_LEN: u32 = 256;

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
    /// to reach the window.
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

/// A flash image several owners hold: the flash controller executes commands
/// against it and the cache fill reads through it.
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
    /// and get a board with an empty chip" is the loop a second-boot gate
    /// runs in, and making the user pre-create a 4 MiB file of `0xff` would
    /// be a step with no meaning.
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
    /// `0x0016_40ef` for a 4 MiB part. The manufacturer and type are the
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
    /// census stays a census of what the *guest* asked the controller to do.
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
    /// (the command engine) has already decided which command this is.
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

    /// Place bytes without going through a flash command — what a loader
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

// ---- the command engine -------------------------------------------------

/// SPI NOR command bytes a controller's generic `usr` engine may carry in
/// its command phase. Only the ones a mask ROM actually sends are named;
/// anything else is [`FlashOp::Unknown`] and the view refuses it out loud.
///
/// These are the part's command set, not a SoC's register map, so they live
/// with the chip.
pub mod op {
    pub const READ: u8 = 0x03;
    pub const FAST_READ: u8 = 0x0b;
    pub const DUAL_OUT: u8 = 0x3b;
    pub const QUAD_OUT: u8 = 0x6b;
    pub const DUAL_IO: u8 = 0xbb;
    pub const QUAD_IO: u8 = 0xeb;
    pub const PAGE_PROGRAM: u8 = 0x02;
    pub const READ_STATUS: u8 = 0x05;
    pub const READ_STATUS_HIGH: u8 = 0x35;
    pub const WRITE_ENABLE: u8 = 0x06;
    pub const WRITE_DISABLE: u8 = 0x04;
    pub const RDID: u8 = 0x9f;
    pub const SECTOR_ERASE: u8 = 0x20;
    pub const BLOCK_ERASE_64K: u8 = 0xd8;

    /// The read commands, all of which mean "give me this many bits from
    /// `addr`". The mode/dummy differences are wire-level and move no
    /// different bytes.
    pub const READS: &[u8] = &[READ, FAST_READ, DUAL_OUT, QUAD_OUT, DUAL_IO, QUAD_IO];
}

/// Status-register bits, as a mask ROM reads them. **The flash part's status
/// register, not the SoC's**: WIP and WEL are the SPI NOR command set's own,
/// which is why they are here and not in a view.
///
/// Write-in-progress. Always 0 here: operations complete inside the write.
pub const SR_WIP: u16 = 1 << 0;
/// Write-enable latch. A ROM's `_SPI_write_enable` loops until this is set.
pub const SR_WEL: u16 = 1 << 1;

/// The 64-byte data buffer a transfer moves through, as bytes.
///
/// The controller's own register file holds it as words; the *count* of
/// words and the offset they start at are the view's business, and the
/// engine only ever sees the gathered bytes.
pub const BUFFER_LEN: usize = 64;

/// One flash transaction, decoded. The view reads its own `cmd`, `user`,
/// `user1`, `user2`, `addr` and length registers, checks its own phases, and
/// says which of these it wants; the engine runs it against the chip.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum FlashOp {
    /// Read the JEDEC id.
    ReadId,
    /// Read the status register. Counts as a status poll in the census.
    ReadStatus,
    /// Write the status register's low half. WIP stays 0 and the write
    /// consumes WEL, as it does on the part.
    WriteStatus(u16),
    /// Set the write-enable latch, and count it.
    WriteEnable,
    /// Clear the write-enable latch.
    WriteDisable,
    /// Erase the granule `addr` falls in. `len` is the granule.
    Erase { addr: u32, len: u32 },
    /// Erase the whole chip.
    EraseChip,
    /// Program `len` bytes of the buffer at `addr`.
    Program { addr: u32, len: u32 },
    /// Read `len` bytes at `addr` into the buffer.
    Read { addr: u32, len: u32 },
    /// A command byte this model does not know. The engine does **not**
    /// guess: it does nothing and says so, and the view writes the refusal
    /// with its own block name.
    Unknown(u8),
}

/// What the engine gives back. The view puts it where its own registers
/// live — never "the engine wrote `w0`", because the engine does not know
/// where `w0` is.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum FlashOutcome {
    /// Nothing to place.
    Nothing,
    /// Put this word in the first buffer word.
    Word(u32),
    /// The 64-byte buffer was filled; scatter it back into the block's own
    /// data words.
    Buffer,
    /// The status register's current low half.
    Status(u16),
    /// The engine does not know this command; the view refuses it with its
    /// own block name and trace line.
    Unknown,
}

/// The flash side of a controller: the chip, and the status latch a mask ROM
/// spins on.
#[derive(Clone, Debug)]
pub struct FlashEngine {
    flash: FlashHandle,
    /// The chip's status register. Bits above WEL are whatever a status
    /// write last put there — the part remembers block-protect bits and a
    /// ROM's `_esp_rom_spiflash_unlock` writes them, so a model that dropped
    /// them would make the unlock unobservable.
    status: u16,
}

impl FlashEngine {
    /// An engine over `flash`, with a clear status register: nothing is in
    /// progress and the write-enable latch is down.
    pub fn new(flash: FlashHandle) -> Self {
        Self { flash, status: 0 }
    }

    pub fn status(&self) -> u16 {
        self.status
    }

    pub fn flash(&self) -> &FlashHandle {
        &self.flash
    }

    /// The flash byte address an address phase carries.
    ///
    /// A ROM writes the plain byte address into the controller's `addr`
    /// register, and the address-phase bit length says how many bits go on
    /// the wire — 24 for a plain read, 28 for QIO's four mode bits. The bits
    /// above 24 are therefore mode, not address, and a NOR part addressed
    /// this way is at most 16 MiB, so the address is the low 24 bits.
    /// Anything above them is reported once rather than silently folded.
    pub fn flash_addr(raw: u32, name: &str, cx: &mut BusCx<'_>) -> u32 {
        if raw & 0xff00_0000 != 0 {
            cx.trace.note(&format!(
                "cyc={} pc={:#010x} {name} addr {raw:#010x} has bits above 24; \
                 this chip is 16 MiB at most, using {:#010x}",
                cx.now,
                cx.pc,
                raw & 0x00ff_ffff
            ));
        }
        raw & 0x00ff_ffff
    }

    /// Run one decoded transaction against the chip.
    ///
    /// `buffer` is the block's 64-byte data buffer: the view has already
    /// gathered it for a [`FlashOp::Program`], and the engine fills it for a
    /// [`FlashOp::Read`] (padding with `0xff`, which is what an erased part
    /// puts on the wire). `name` is the view's own block name, so a
    /// diagnostic says which block moved — or refused to move — the bytes.
    pub fn execute(
        &mut self,
        op: FlashOp,
        buffer: &mut [u8; BUFFER_LEN],
        name: &str,
        cx: &mut BusCx<'_>,
    ) -> FlashOutcome {
        match op {
            FlashOp::ReadId => {
                let id = self.flash.lock().unwrap().jedec_id();
                FlashOutcome::Word(id)
            }
            FlashOp::ReadStatus => {
                self.flash.lock().unwrap().status_reads += 1;
                FlashOutcome::Status(self.status)
            }
            FlashOp::WriteStatus(value) => {
                self.status = value & !SR_WIP;
                self.status &= !SR_WEL;
                FlashOutcome::Nothing
            }
            FlashOp::WriteEnable => {
                self.flash.lock().unwrap().write_enables += 1;
                self.status |= SR_WEL;
                FlashOutcome::Nothing
            }
            FlashOp::WriteDisable => {
                self.status &= !SR_WEL;
                FlashOutcome::Nothing
            }
            FlashOp::Erase { addr, len } => {
                self.erase(addr, len, name, cx);
                FlashOutcome::Nothing
            }
            FlashOp::EraseChip => {
                self.flash.lock().unwrap().erase_chip();
                self.status &= !SR_WEL;
                FlashOutcome::Nothing
            }
            FlashOp::Program { addr, len } => {
                self.program(addr, len, buffer, name, cx);
                FlashOutcome::Nothing
            }
            FlashOp::Read { addr, len } => {
                self.read_into_buffer(addr, len, buffer, name, cx);
                FlashOutcome::Buffer
            }
            FlashOp::Unknown(_) => FlashOutcome::Unknown,
        }
    }

    fn read_into_buffer(
        &mut self,
        addr: u32,
        len: u32,
        buffer: &mut [u8; BUFFER_LEN],
        name: &str,
        cx: &mut BusCx<'_>,
    ) {
        let bytes = {
            let mut flash = self.flash.lock().unwrap();
            match flash.read(addr, len) {
                Some(slice) => slice.to_vec(),
                None => {
                    let chip = flash.len();
                    drop(flash);
                    cx.trace.note(&format!(
                        "cyc={} pc={:#010x} {name} read {len} bytes at {addr:#010x} leaves the \
                         {chip:#x}-byte chip; the buffer reads 0xff",
                        cx.now, cx.pc
                    ));
                    vec![0xff; len as usize]
                }
            }
        };
        // Short of the buffer's length the rest reads erased, which is what
        // the view's own scatter did when it was handed a short slice.
        for (slot, at) in buffer.iter_mut().zip(0usize..) {
            *slot = bytes.get(at).copied().unwrap_or(0xff);
        }
    }

    fn program(
        &mut self,
        addr: u32,
        len: u32,
        buffer: &[u8; BUFFER_LEN],
        name: &str,
        cx: &mut BusCx<'_>,
    ) {
        if self.status & SR_WEL == 0 {
            cx.trace.note(&format!(
                "cyc={} pc={:#010x} {name} page program at {addr:#010x} with WEL clear; \
                 the part would ignore it, and so does this",
                cx.now, cx.pc
            ));
            return;
        }
        let len = (len as usize).min(BUFFER_LEN);
        // A page program may not cross a 256-byte page: the part wraps
        // within the page instead, which is a bug the caller wants to see.
        if (addr % PAGE_LEN) as usize + len > PAGE_LEN as usize {
            cx.trace.note(&format!(
                "cyc={} pc={:#010x} {name} page program of {len} bytes at {addr:#010x} crosses a \
                 256-byte page boundary; the part would wrap, this model programs straight \
                 through",
                cx.now, cx.pc
            ));
        }
        if !self.flash.lock().unwrap().program(addr, &buffer[..len]) {
            cx.trace.note(&format!(
                "cyc={} pc={:#010x} {name} page program of {len} bytes at {addr:#010x} leaves \
                 the chip; nothing was written",
                cx.now, cx.pc
            ));
        }
        self.status &= !SR_WEL;
    }

    fn erase(&mut self, addr: u32, len: u32, name: &str, cx: &mut BusCx<'_>) {
        if self.status & SR_WEL == 0 {
            cx.trace.note(&format!(
                "cyc={} pc={:#010x} {name} erase at {addr:#010x} with WEL clear; ignored",
                cx.now, cx.pc
            ));
            return;
        }
        // The part erases the granule the address falls in.
        let aligned = addr & !(len - 1);
        if !self.flash.lock().unwrap().erase(aligned, len) {
            cx.trace.note(&format!(
                "cyc={} pc={:#010x} {name} erase of {len} bytes at {aligned:#010x} leaves the \
                 chip; nothing was erased",
                cx.now, cx.pc
            ));
        }
        self.status &= !SR_WEL;
    }

    /// The status latch, little-endian. The view writes its own register
    /// file's bytes around this, in whatever order its snapshot format
    /// already pinned.
    pub fn save_status(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.status.to_le_bytes());
    }

    /// Parse a [`save_status`](Self::save_status) chunk and say how many
    /// bytes it took; `None` when the blob is short, so that a truncated
    /// snapshot leaves the engine untouched rather than half-loaded.
    pub fn load_status(bytes: &[u8]) -> Option<(u16, usize)> {
        let raw: [u8; 2] = bytes.get(..2)?.try_into().ok()?;
        Some((u16::from_le_bytes(raw), 2))
    }

    /// Apply a parsed status latch.
    pub fn restore(&mut self, status: u16) {
        self.status = status;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::periph::Sandbox;

    /// A chip size the tests read at a glance. Not a part's default: that is
    /// a board fact and lives in the chip crate.
    const CHIP_LEN: u32 = 4 * 1024 * 1024;

    #[test]
    fn a_blank_chip_is_all_ones_and_knows_its_own_capacity() {
        let f = FlashImage::blank(CHIP_LEN);
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
    fn the_jedec_capacity_byte_describes_the_same_length_as_the_chip() {
        for len in [SECTOR_LEN * 2, BLOCK_LEN, 2 * 1024 * 1024, CHIP_LEN] {
            let f = FlashImage::blank(len);
            let [_, _, capacity, _] = f.jedec_id().to_le_bytes();
            assert_eq!(1u32 << capacity, f.len(), "capacity byte for {len:#x}");
        }
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
        let mut f = FlashImage::blank(CHIP_LEN);
        assert!(f.take_written_blocks().is_empty());
        assert!(f.program(BLOCK_LEN - 2, &[0, 0, 0, 0]));
        assert_eq!(f.take_written_blocks(), vec![0, 1]);
        assert!(f.take_written_blocks().is_empty());
        assert!(f.erase(0x0031_0000, SECTOR_LEN));
        assert_eq!(f.take_written_blocks(), vec![0x0031_0000 / BLOCK_LEN]);
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

    // ---- the command engine ---------------------------------------------

    fn rig(len: u32) -> (Sandbox, FlashEngine, FlashHandle) {
        let flash: FlashHandle = Arc::new(Mutex::new(FlashImage::blank(len)));
        (Sandbox::new(), FlashEngine::new(flash.clone()), flash)
    }

    #[test]
    fn the_user_op_walks_command_then_address_then_data() {
        let (mut sb, mut engine, flash) = rig(CHIP_LEN);
        flash.lock().unwrap().stage(0x1000, b"littlefs");
        let mut buffer = [0u8; BUFFER_LEN];
        let outcome = engine.execute(
            FlashOp::Read {
                addr: 0x1000,
                len: 8,
            },
            &mut buffer,
            "FLASH",
            &mut sb.cx(),
        );
        assert_eq!(outcome, FlashOutcome::Buffer);
        assert_eq!(&buffer[..8], b"littlefs");
        assert!(buffer[8..].iter().all(|b| *b == 0xff), "the rest is erased");
        assert_eq!(flash.lock().unwrap().reads, 1);
    }

    #[test]
    fn program_can_only_clear_bits() {
        let (mut sb, mut engine, flash) = rig(SECTOR_LEN * 2);
        let mut buffer = [0xffu8; BUFFER_LEN];
        buffer[..2].copy_from_slice(&[0xf0, 0x0f]);
        engine.execute(FlashOp::WriteEnable, &mut buffer, "FLASH", &mut sb.cx());
        engine.execute(
            FlashOp::Program { addr: 0, len: 2 },
            &mut buffer,
            "FLASH",
            &mut sb.cx(),
        );
        assert_eq!(flash.lock().unwrap().peek(0, 2).unwrap(), &[0xf0, 0x0f]);
        assert_eq!(engine.status() & SR_WEL, 0, "the program consumed WEL");

        // A second program without an erase leaves the AND of the two.
        buffer[..2].copy_from_slice(&[0x3c, 0x3c]);
        engine.execute(FlashOp::WriteEnable, &mut buffer, "FLASH", &mut sb.cx());
        engine.execute(
            FlashOp::Program { addr: 0, len: 2 },
            &mut buffer,
            "FLASH",
            &mut sb.cx(),
        );
        assert_eq!(flash.lock().unwrap().peek(0, 2).unwrap(), &[0x30, 0x0c]);
        assert_eq!(flash.lock().unwrap().write_enables, 2);
    }

    #[test]
    fn erase_is_the_only_way_back_to_ff() {
        let (mut sb, mut engine, flash) = rig(SECTOR_LEN * 2);
        flash.lock().unwrap().stage(0x800, b"stale");
        let mut buffer = [0u8; BUFFER_LEN];
        // Without the latch the part ignores the erase, and so does this.
        engine.execute(
            FlashOp::Erase {
                addr: 0x800,
                len: SECTOR_LEN,
            },
            &mut buffer,
            "FLASH",
            &mut sb.cx(),
        );
        assert_eq!(flash.lock().unwrap().peek(0x800, 5).unwrap(), b"stale");

        engine.execute(FlashOp::WriteEnable, &mut buffer, "FLASH", &mut sb.cx());
        // A byte inside the sector, not its base: the part aligns.
        engine.execute(
            FlashOp::Erase {
                addr: 0x800,
                len: SECTOR_LEN,
            },
            &mut buffer,
            "FLASH",
            &mut sb.cx(),
        );
        assert_eq!(flash.lock().unwrap().peek(0x800, 5).unwrap(), &[0xff; 5]);
        assert_eq!(flash.lock().unwrap().sector_erases, 1);
        assert_eq!(engine.status() & SR_WEL, 0, "the erase consumed WEL");
    }

    #[test]
    fn the_status_latch_answers_the_loop_the_rom_spins_in() {
        let (mut sb, mut engine, flash) = rig(SECTOR_LEN);
        let mut buffer = [0u8; BUFFER_LEN];
        let before = engine.execute(FlashOp::ReadStatus, &mut buffer, "FLASH", &mut sb.cx());
        engine.execute(FlashOp::WriteEnable, &mut buffer, "FLASH", &mut sb.cx());
        let after = engine.execute(FlashOp::ReadStatus, &mut buffer, "FLASH", &mut sb.cx());
        let id = engine.execute(FlashOp::ReadId, &mut buffer, "FLASH", &mut sb.cx());
        assert_eq!(before, FlashOutcome::Status(0));
        assert_eq!(after, FlashOutcome::Status(SR_WEL));
        assert_eq!(id, FlashOutcome::Word(flash.lock().unwrap().jedec_id()));
        assert_eq!(flash.lock().unwrap().status_reads, 2);
        // WIP never survives a status write: operations complete inside it.
        engine.execute(
            FlashOp::WriteStatus(0xfd),
            &mut buffer,
            "FLASH",
            &mut sb.cx(),
        );
        assert_eq!(engine.status() & SR_WIP, 0);
        assert_eq!(engine.status() & SR_WEL, 0);
        assert_eq!(engine.status(), 0xfc);
    }

    #[test]
    fn an_unknown_trigger_is_reported_not_guessed() {
        let (mut sb, mut engine, flash) = rig(SECTOR_LEN);
        let mut buffer = [0u8; BUFFER_LEN];
        let outcome = engine.execute(FlashOp::Unknown(0x77), &mut buffer, "FLASH", &mut sb.cx());
        assert_eq!(outcome, FlashOutcome::Unknown);
        assert_eq!(engine.status(), 0, "nothing was guessed at");
        assert_eq!(
            flash.lock().unwrap().command_census(),
            FlashCensus::default()
        );
    }

    #[test]
    fn the_engine_state_round_trips() {
        let (mut sb, mut engine, _flash) = rig(SECTOR_LEN);
        let mut buffer = [0u8; BUFFER_LEN];
        engine.execute(
            FlashOp::WriteStatus(0x00bc),
            &mut buffer,
            "FLASH",
            &mut sb.cx(),
        );
        let mut blob = Vec::new();
        engine.save_status(&mut blob);
        blob.extend_from_slice(b"the view's own bytes");

        let (status, n) = FlashEngine::load_status(&blob).expect("the status half");
        assert_eq!(n, 2);
        assert_eq!(&blob[n..], b"the view's own bytes");
        let (_, mut other, _) = rig(SECTOR_LEN);
        other.restore(status);
        assert_eq!(other.status(), engine.status());
        assert!(
            FlashEngine::load_status(&blob[..1]).is_none(),
            "a short blob loads nothing"
        );
    }
}
