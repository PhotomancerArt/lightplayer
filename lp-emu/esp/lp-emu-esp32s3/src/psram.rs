//! The octal PSRAM on `SPI_CS1`: the chip's bytes and its mode registers.
//!
//! The canonical board (Seeed XIAO ESP32-S3, an ESP32-S3R8) and the desk's
//! FS3 (the XIAO ESP32-S3 Plus) both carry **8 MiB of octal PSRAM** in the
//! package, on the memory SPI bus beside the flash: flash on `CS0`, PSRAM on
//! `CS1`. Nothing in the product uses it yet; this model exists so an image
//! that brings it up boots here, and so one that never touches it runs
//! exactly as it did before the chip was modelled.
//!
//! # Who reaches the chip, and how
//!
//! Two paths, and this file is the state both of them share.
//!
//! **Its mode registers, through `SPI1`.** esp-hal's octal driver
//! (`third_party/esp-hal/src/psram/esp32s3.rs`, `octal_spi_impl`) reads and
//! writes them with the mask ROM's `esp_rom_opiflash_exec_cmd`
//! (`0x4004_e058`), which drives `SPI1`'s user engine with `misc` selecting
//! `CS1` (`esp_rom_spi_cmd_start` `4004e0e2..f7`: `misc[1:0] = ~cs_mask`).
//! The two commands it sends are the driver's own constants:
//! `OPI_PSRAM_REG_READ = 0x4040` and `OPI_PSRAM_REG_WRITE = 0xC0C0`, 16-bit
//! commands with a 32-bit address naming the register. [`crate::periph::spi1`]
//! decodes the transaction; [`Psram::read_registers`] and
//! [`Psram::write_registers`] answer it.
//!
//! **Its bytes, through the cache.** After the registers, the driver maps the
//! chip into the data bus with `Cache_Dbus_MMU_Set(MMU_ACCESS_SPIRAM, …)`
//! (`map_psram`): MMU entries with bit 15 set, page number = PSRAM page.
//! [`crate::cache::fill`] serves those pages out of [`Psram::page`] and
//! writes them back with [`Psram::page_mut`]. The SPI0 cache-side command
//! set-up (`config_psram_spi_phases`) is accept-and-remember: the bytes that
//! move do not depend on it on this machine.
//!
//! # The mode registers, and where each value comes from
//!
//! A register read at address `a` for `n` bytes answers `MR[a], MR[a+1], …`:
//! esp-hal reads 16 bits at `0` into `mr0` then `mr1` and decodes the vendor
//! id out of the second byte, and 16 bits at `2` into `mr2`/`mr3`
//! (`psram_mode_reg`). Which bit means what is the driver's
//! `OpiPsramModeReg` accessors; the driver acts on two fields and prints the
//! rest:
//!
//! - **MR1 bits 4:0, the vendor id**, must read `0x0D` (`OCT_PSRAM_VENDOR_ID`,
//!   "AP") or the driver gives up on octal mode;
//! - **MR2 bits 2:0, the density**, give the size (`0x0` 2 MB, `0x1` 4 MB,
//!   `0x3` 8 MB, `0x5` 16 MB, `0x7` 32 MB — the driver's table).
//!
//! The values this chip holds at reset are [`RESET_MODE_REGISTERS`], with
//! their provenance there. A **write** is honoured for `MR0` only — the one
//! register the driver writes (`init_psram_mode_reg`: read MR0, set latency
//! type, read latency and drive strength, write it back) — and refused with a
//! trace line for any other, because nothing here says which of the others
//! the part lets a host change.
//!
//! # Absent
//!
//! A board with no PSRAM is `len == 0` ([`Psram::absent`]). Nothing drives
//! the data lines then, and a read answers `0xff` on every byte: both of the
//! driver's not-found checks are written against exactly that (`dev_id ==
//! 0xffffff` on the quad path; a vendor id of `0x1F` is not `0x0D` on the
//! octal one), so it reports no chip and maps nothing.
//!
//! # What is not modelled
//!
//! Timing (the read/write latency the registers configure, refresh, the
//! DQS strobe), burst wrap, ECC, the PSRAM's own data commands over `SPI1`
//! (`0x0000` sync read, `0x8080` sync write — the driver sends neither; a
//! `SPI1` transaction carrying one is refused), and the quad-SPI PSRAM
//! command set (`0x9F` read id, `0x66`/`0x99` reset, …), which only a quad
//! part answers. A quad command to this octal part is refused, and reads
//! `0xff` — the driver's auto mode tries octal first and never sends one to
//! a part that answered it.

use std::sync::{Arc, Mutex};

use crate::cache::PAGE_LEN;

/// The XIAO ESP32-S3's PSRAM: **8 MiB** (an ESP32-S3R8 — "R8", 8 MB of
/// octal PSRAM in the package; Seeed's product page and Espressif's part
/// numbering). The machine's default; `--psram-len` overrides, `0` for a
/// board without.
pub const DEFAULT_PSRAM_LEN: u32 = 8 * 1024 * 1024;

/// The sizes the density field can say, and its code for each — esp-hal's
/// own decode table (`psram_init`, `match mode_reg.density()`), inverted.
pub const DENSITIES: &[(u32, u8)] = &[
    (2 * 1024 * 1024, 0x0),
    (4 * 1024 * 1024, 0x1),
    (8 * 1024 * 1024, 0x3),
    (16 * 1024 * 1024, 0x5),
    (32 * 1024 * 1024, 0x7),
];

/// A PSRAM size as a command line spells it: `0` or `none` for a board
/// without, `2mb` … `32mb`, or a byte count (decimal or `0x…`). Refuses a
/// size the density field cannot say.
pub fn parse_size(text: &str) -> Result<u32, String> {
    let t = text.trim().to_ascii_lowercase();
    let len = if t == "none" {
        0
    } else if let Some(mb) = t.strip_suffix("mb").or_else(|| t.strip_suffix("mib")) {
        mb.parse::<u32>()
            .ok()
            .and_then(|m| m.checked_mul(1024 * 1024))
            .ok_or_else(|| format!("PSRAM size `{text}`: not a size"))?
    } else if let Some(hex) = t.strip_prefix("0x") {
        u32::from_str_radix(hex, 16).map_err(|_| format!("PSRAM size `{text}`: not a size"))?
    } else {
        t.parse::<u32>()
            .map_err(|_| format!("PSRAM size `{text}`: not a size"))?
    };
    Psram::new(len).map(|p| p.len())
}

/// `OPI_PSRAM_REG_READ` — esp-hal `octal_spi_impl`.
pub const CMD_REG_READ: u16 = 0x4040;
/// `OPI_PSRAM_REG_WRITE` — esp-hal `octal_spi_impl`.
pub const CMD_REG_WRITE: u16 = 0xC0C0;

/// The mode-register file is addressed 0..=8 (`MR0`…`MR4`, `MR8`).
pub const MODE_REGISTERS: usize = 9;

/// The mode registers at power-on, by address. `MR5`–`MR7` are read by
/// nothing and stay 0.
///
/// **Evidence: FS3 (`fixture-s3`, D8:3B:DA:47:29:70, an ESP32-S3 with 8 MB
/// octal PSRAM), 2026-10-22**, `fw-esp32s3 --features test_psram`: esp-hal's
/// own `print_psram_info`, which decodes the registers its driver read during
/// bring-up —
///
/// ```text
/// vendor id    : 0d (AP)              MR1[4:0]
/// dev id       : 02 (generation 3)    MR2[4:3]
/// density      : 03 (64 Mbit)         MR2[2:0]
/// good-die     : 01 (Pass)            MR2[7]
/// Latency      : 01 (Fixed)           MR0[5]    ┐ after the driver's
/// Readlatency  : 02 (10 cycles@Fixed) MR0[4:2]  │ own MR0 write
/// DriveStrength: 00 (1/1)             MR0[1:0]  ┘
/// VCC          : 01 (3V)              MR3[6]
/// SRF          : 01 (Fast Refresh)    MR3[5]
/// BurstType    : 01 (Hybrid Wrap)     MR8[2]
/// BurstLen     : 01 (32 Byte)         MR8[1:0]
/// ```
///
/// | reg | value | what is observed |
/// |---|---|---|
/// | MR0 | `0x00` | ⚠️ nothing at power-on: the driver writes its three fields before it reads them back, so the capture is of its own write. 0 is a placeholder the driver overwrites |
/// | MR1 | `0x0D` | the vendor id; bits 7:5 unobserved, 0 |
/// | MR2 | `0x90` \| density | good-die 1, generation-3 dev id `0b10`; bits 6:5 unobserved, 0. The density is the chip's size ([`DENSITIES`]), `0x3` for 8 MiB — the captured value |
/// | MR3 | `0x60` | VCC and SRF set; bits 4:0 and 7 unobserved, 0 |
/// | MR4 | `0x00` | ⚠️ not printed by the driver: unobserved |
/// | MR8 | `0x05` | burst type 1, burst length 1; bits 7:3 unobserved, 0 |
///
/// ⚠️ A raw read of the registers **after** the driver finished (the probe's
/// first capture tried one, through the same ROM call) answered
/// `05 60 05 60 05 05` for MR0–MR4, MR8 — alternating values that agree with
/// none of the decode above, so `SPI1` was no longer set up for that read
/// once the driver had moved the bus to high speed. It is not evidence of
/// anything here, and the probe no longer makes it.
pub const RESET_MODE_REGISTERS: [u8; MODE_REGISTERS] =
    [0x00, 0x0D, 0x90, 0x60, 0x00, 0, 0, 0, 0x05];

/// The chip, shared between `SPI1` (the registers), the cache fill (the
/// pages) and the machine (the snapshot and the run report).
pub type PsramHandle = Arc<Mutex<Psram>>;

/// What the chip was asked, for the run report.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PsramCensus {
    pub register_reads: u64,
    pub register_writes: u64,
    /// Transactions this model refused: an unknown command, a write to a
    /// register other than MR0.
    pub refused: u64,
    /// Pages the cache filled from the chip, and wrote back to it.
    pub pages_filled: u64,
    pub pages_written_back: u64,
}

impl PsramCensus {
    /// Did anything reach the chip at all? A run whose image never touched
    /// PSRAM reports nothing about it.
    pub fn touched(&self) -> bool {
        *self != Self::default()
    }
}

/// The octal PSRAM. See the module docs.
#[derive(Clone)]
pub struct Psram {
    /// The chip's bytes; empty for a board without PSRAM. **Zero at power
    /// on**: silicon's power-on contents are undefined, and zero is the
    /// deterministic choice, not a claim.
    bytes: Vec<u8>,
    mode: [u8; MODE_REGISTERS],
    census: PsramCensus,
    /// Has anything ever been written into `bytes`? Lets a snapshot of a
    /// chip nobody wrote carry nothing.
    written: bool,
}

impl core::fmt::Debug for Psram {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Psram")
            .field("len", &self.bytes.len())
            .field("mode", &self.mode)
            .field("census", &self.census)
            .finish()
    }
}

impl Psram {
    /// A chip of `len` bytes — one of [`DENSITIES`]' sizes, or 0 for none.
    pub fn new(len: u32) -> Result<Self, String> {
        if len == 0 {
            return Ok(Self::absent());
        }
        let Some(&(_, density)) = DENSITIES.iter().find(|(size, _)| *size == len) else {
            return Err(format!(
                "a PSRAM of {len} bytes cannot say its size: the density field \
                 (esp-hal's decode) names 2, 4, 8, 16 or 32 MiB, or 0 for none"
            ));
        };
        let mut mode = RESET_MODE_REGISTERS;
        mode[2] = (mode[2] & !0x07) | density;
        Ok(Self {
            bytes: vec![0; len as usize],
            mode,
            census: PsramCensus::default(),
            written: false,
        })
    }

    /// No chip: nothing drives the bus (module docs, "Absent").
    pub fn absent() -> Self {
        Self {
            bytes: Vec::new(),
            mode: [0xff; MODE_REGISTERS],
            census: PsramCensus::default(),
            written: false,
        }
    }

    pub fn handle(self) -> PsramHandle {
        Arc::new(Mutex::new(self))
    }

    pub fn len(&self) -> u32 {
        self.bytes.len() as u32
    }

    pub fn is_present(&self) -> bool {
        !self.bytes.is_empty()
    }

    pub fn census(&self) -> PsramCensus {
        self.census
    }

    pub fn mode_registers(&self) -> [u8; MODE_REGISTERS] {
        self.mode
    }

    /// A mode-register read: `MR[addr + k]` for each byte asked. `0xff` past
    /// the file and on an absent chip.
    pub fn read_registers(&mut self, addr: u32, out: &mut [u8]) {
        self.census.register_reads += 1;
        for (k, slot) in out.iter_mut().enumerate() {
            let at = addr as usize + k;
            *slot = if self.is_present() {
                self.mode.get(at).copied().unwrap_or(0xff)
            } else {
                0xff
            };
        }
    }

    /// A mode-register write. Returns the registers it refused, for the
    /// caller's trace line (module docs: MR0 only).
    pub fn write_registers(&mut self, addr: u32, data: &[u8]) -> Vec<usize> {
        self.census.register_writes += 1;
        if !self.is_present() {
            return Vec::new();
        }
        let mut refused = Vec::new();
        for (k, byte) in data.iter().enumerate() {
            let at = addr as usize + k;
            if at == 0 {
                self.mode[0] = *byte;
            } else if self.mode.get(at) != Some(byte) {
                // A write that leaves a register as it reads changes nothing
                // and is not refused: the driver's 16-bit MR0 write carries
                // MR1 along with the value it read there.
                refused.push(at);
            }
        }
        if !refused.is_empty() {
            self.census.refused += 1;
        }
        refused
    }

    /// A transaction this model does not answer.
    pub fn note_refused(&mut self) {
        self.census.refused += 1;
    }

    /// PSRAM page `page` (64 KiB, the MMU's page), or `None` past the chip.
    pub fn page(&self, page: u32) -> Option<&[u8]> {
        let lo = (page as usize).checked_mul(PAGE_LEN as usize)?;
        self.bytes.get(lo..lo + PAGE_LEN as usize)
    }

    /// The fill copied a page out of the chip.
    pub fn note_filled(&mut self) {
        self.census.pages_filled += 1;
    }

    /// Write a page back — the cache is giving up a window page that held it.
    pub fn write_back(&mut self, page: u32, bytes: &[u8]) -> bool {
        let lo = page as usize * PAGE_LEN as usize;
        let Some(dst) = self.bytes.get_mut(lo..lo + PAGE_LEN as usize) else {
            return false;
        };
        if dst != bytes {
            dst.copy_from_slice(bytes);
            self.written = true;
        }
        self.census.pages_written_back += 1;
        true
    }

    /// The chip's bytes, or `None` when nothing has ever written them — a
    /// snapshot of an untouched chip is nothing at all.
    pub fn contents(&self) -> Option<&[u8]> {
        self.written.then_some(&self.bytes[..])
    }

    /// Put bytes back (a snapshot restore, or a reset that keeps them).
    /// `None` is "never written": zero.
    pub fn restore_contents(&mut self, bytes: Option<&[u8]>) {
        match bytes {
            Some(b) if b.len() == self.bytes.len() => {
                self.bytes.copy_from_slice(b);
                self.written = true;
            }
            Some(b) => log::warn!(
                "Psram::restore_contents: {} bytes for a {}-byte chip; ignored",
                b.len(),
                self.bytes.len()
            ),
            None => self.clear(),
        }
    }

    /// The supply went away: the bytes are gone (zero, as at power on) and
    /// the mode registers are back at reset.
    pub fn power_off(&mut self) {
        self.clear();
        let len = self.len();
        if let Ok(fresh) = Self::new(len) {
            self.mode = fresh.mode;
        }
    }

    fn clear(&mut self) {
        if self.written {
            self.bytes.fill(0);
            self.written = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_density_field_says_the_size_the_driver_decodes() {
        for (len, code) in DENSITIES {
            let mut p = Psram::new(*len).unwrap();
            let mut mr = [0u8; 2];
            p.read_registers(2, &mut mr);
            assert_eq!(mr[0] & 0x07, *code, "{len}");
        }
        assert!(Psram::new(3 * 1024 * 1024).is_err());
    }

    #[test]
    fn a_register_read_answers_consecutive_registers_from_its_address() {
        let mut p = Psram::new(DEFAULT_PSRAM_LEN).unwrap();
        let mut mr01 = [0u8; 2];
        p.read_registers(0, &mut mr01);
        // esp-hal decodes the vendor id out of the SECOND byte of a 16-bit
        // read at 0: `mr1 & 0x1f == 0x0d`.
        assert_eq!(mr01[1] & 0x1f, 0x0d);
        let mut mr = [0u8; 9];
        p.read_registers(0, &mut mr);
        // FS3's decode (RESET_MODE_REGISTERS' docs), field by field.
        assert_eq!(mr[2] & 0x07, 0x3, "density: 64 Mbit");
        assert_eq!((mr[2] >> 3) & 0x3, 2, "dev id: generation 3");
        assert_eq!(mr[2] >> 7, 1, "good-die: pass");
        assert_eq!((mr[3] >> 6) & 1, 1, "VCC: 3V");
        assert_eq!((mr[3] >> 5) & 1, 1, "SRF: fast refresh");
        assert_eq!((mr[8] >> 2) & 1, 1, "burst type: hybrid wrap");
        assert_eq!(mr[8] & 0x3, 1, "burst length: 32 bytes");
    }

    #[test]
    fn only_mr0_takes_a_write() {
        let mut p = Psram::new(DEFAULT_PSRAM_LEN).unwrap();
        // `init_psram_mode_reg` writes 16 bits from `&mr0`: mr0 and mr1.
        // `init_psram_mode_reg` writes back the MR1 it read: no change.
        assert!(p.write_registers(0, &[0x28, 0x0d]).is_empty());
        let refused = p.write_registers(0, &[0x28, 0x00]);
        assert_eq!(refused, vec![1]);
        let mut mr01 = [0u8; 2];
        p.read_registers(0, &mut mr01);
        assert_eq!(mr01, [0x28, 0x0d], "MR1 kept its vendor id");
    }

    #[test]
    fn a_size_parses_the_way_a_command_line_spells_it() {
        assert_eq!(parse_size("8mb"), Ok(8 << 20));
        assert_eq!(parse_size("8MiB"), Ok(8 << 20));
        assert_eq!(parse_size("0"), Ok(0));
        assert_eq!(parse_size("none"), Ok(0));
        assert_eq!(parse_size("0x200000"), Ok(2 << 20));
        assert_eq!(parse_size("16777216"), Ok(16 << 20));
        assert!(parse_size("3mb").is_err());
        assert!(parse_size("lots").is_err());
    }

    #[test]
    fn an_absent_chip_reads_all_ones_and_maps_nothing() {
        let mut p = Psram::new(0).unwrap();
        assert!(!p.is_present());
        let mut out = [0u8; 3];
        p.read_registers(0, &mut out);
        assert_eq!(out, [0xff; 3]);
        assert!(p.page(0).is_none());
    }

    #[test]
    fn pages_write_back_and_a_power_cut_loses_them() {
        let mut p = Psram::new(2 * 1024 * 1024).unwrap();
        assert!(p.contents().is_none(), "nobody wrote it");
        let page = vec![0xa5; PAGE_LEN as usize];
        assert!(p.write_back(31, &page));
        assert!(!p.write_back(32, &page), "past a 2 MiB chip");
        assert_eq!(p.page(31).unwrap()[17], 0xa5);
        assert!(p.contents().is_some());
        p.power_off();
        assert_eq!(p.page(31).unwrap()[17], 0);
        assert!(p.contents().is_none());
    }
}
