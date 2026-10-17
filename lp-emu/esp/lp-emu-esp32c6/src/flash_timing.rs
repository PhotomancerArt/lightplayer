//! The flash interface's time, from its registers (`t4`).
//!
//! What a transfer between the chip and its flash costs is a property of
//! four registers in three blocks, and this module is where they meet:
//!
//! - **the clock**: `PCR.mspi_clk_conf` and `PCR.sysclk_conf` say how fast
//!   the MSPI module clock runs, and a controller's own `clock` register
//!   divides it into the SPI clock (SCK);
//! - **the line widths**: `ctrl`'s `fread_*` / `fcmd_*` bits (reads) and
//!   `user`'s `fwrite_*` bits (writes) say how many data lines each phase of a
//!   transfer drives;
//! - **the phases**: `user`, `user1`, `user2` and the data lengths say how many
//!   bits each phase carries and how many dummy clocks sit between them.
//!
//! A transfer then costs `clocks / SCK` of real time, plus one fitted
//! per-transfer overhead ([`TRANSFER_OVERHEAD_CYCLES`]). Nothing here knows a
//! flash mode or a frequency by name: whatever the second-stage bootloader
//! left in the registers is what the time follows, so an image whose header
//! says QIO, or a bootloader that sets another clock, predicts its own times.
//!
//! # What the shipped bootloader leaves, and the fill it explains
//!
//! The image header espflash writes says DIO at "40 MHz". Run in this
//! emulator, the real mask ROM and the IDF second-stage bootloader espflash
//! bundles leave `PCR.mspi_clk_conf = 0x500` (the 480 MHz SPLL ÷ 6 = 80 MHz,
//! once the CPU runs from the PLL) and `clock = 0x8000_0000` (`clk_equ_sysclk`,
//! one division) in both controllers — **an 80 MHz SCK**, for a 40, 80 or
//! 20 MHz header alike (only the ROM's own load of the bootloader runs at the
//! header's divider). That is the configuration the `t3` calibration could not
//! name (`docs/reports/2026-09-08-esp32c6-t3-calibration.md`, "The fill's
//! arithmetic, and what refuted it"): a DIO 32-byte line fill is
//! 8 + 14 + 2 + 128 = 152 SCK, which at 80 MHz is 304 CPU cycles, and the
//! silicon fill measured 338. The 34 left over is [`TRANSFER_OVERHEAD_CYCLES`]
//! — fitted to that one measurement, and charged to every SPI1 transfer too.
//!
//! # Who writes, who reads
//!
//! [`FlashClocks`] is a shared cell, the same shape as PCR's clock lines: PCR
//! and SPI0 publish their words into it when they are written, SPI1 reads the
//! MSPI clock out of it, EXTMEM posts its range invalidates into it, and
//! `cache::CacheCost` (at `t4`) reads the line-fill cost and drains the
//! invalidates. None of them sees another peripheral.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use crate::memmap;

/// What a transfer costs over its wire time: chip select, the controller's
/// own latency and (for a fill) the CPU's restart. Fitted: the measured
/// 338-cycle DIO line fill less its 304 cycles of 80 MHz SCK.
pub const TRANSFER_OVERHEAD_CYCLES: u64 = 34;

/// The bytes one cache line fill moves.
pub const LINE_BYTES: u32 = 32;

/// The SPLL the MSPI's high-speed divider takes, on the C6.
pub const SPLL_HZ: u64 = 480_000_000;
/// The crystal, the MSPI's low-speed source at reset.
pub const XTAL_HZ: u64 = 40_000_000;
/// RC_FAST, the other low-speed source (nominal).
pub const RC_FAST_HZ: u64 = 17_500_000;

/// `PCR.mspi_clk_conf` and `PCR.sysclk_conf` at reset (the PAC's values).
pub const PCR_MSPI_CLK_CONF_RESET: u32 = 0x0000_0300;
pub const PCR_SYSCLK_CONF_RESET: u32 = 0x2800_0200;

/// SPI0's timing-relevant registers at reset (`regs::SPI0`'s resets).
pub const SPI0_CLOCK_RESET: u32 = 0x0003_0103;
pub const SPI0_CTRL_RESET: u32 = 0x802c_200c;
pub const SPI0_USER_RESET: u32 = 0x0000_0000;
pub const SPI0_USER1_RESET: u32 = 0x5c00_0047;
pub const SPI0_USER2_RESET: u32 = 0x7000_0000;

/// The SPI0 registers [`FlashClocks`] mirrors.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Spi0Word {
    Ctrl,
    Clock,
    User,
    User1,
    User2,
}

impl Spi0Word {
    /// The register at `off` in SPI0, if it is one this module reads.
    pub fn at(off: u32) -> Option<Self> {
        match off & !3 {
            0x008 => Some(Self::Ctrl),
            0x014 => Some(Self::Clock),
            0x018 => Some(Self::User),
            0x01c => Some(Self::User1),
            0x020 => Some(Self::User2),
            _ => None,
        }
    }

    pub const ALL: [Self; 5] = [
        Self::Ctrl,
        Self::Clock,
        Self::User,
        Self::User1,
        Self::User2,
    ];

    pub const fn offset(self) -> u32 {
        match self {
            Self::Ctrl => 0x008,
            Self::Clock => 0x014,
            Self::User => 0x018,
            Self::User1 => 0x01c,
            Self::User2 => 0x020,
        }
    }
}

/// The shared cell: the words the flash interface's time is made of.
#[derive(Debug)]
pub struct FlashClocks {
    mspi_clk_conf: AtomicU32,
    sysclk_conf: AtomicU32,
    spi0: [AtomicU32; 5],
    /// Bumped on every change that can move the line-fill cost, so a reader
    /// that cached one knows when to recompute it.
    generation: AtomicU32,
    /// Range invalidates EXTMEM performed, `(vaddr, size)`, not yet taken.
    invalidates: Mutex<Vec<(u32, u32)>>,
    invalidate_pending: AtomicBool,
}

impl Default for FlashClocks {
    fn default() -> Self {
        Self::new()
    }
}

impl FlashClocks {
    /// The words at reset.
    pub fn new() -> Self {
        Self {
            mspi_clk_conf: AtomicU32::new(PCR_MSPI_CLK_CONF_RESET),
            sysclk_conf: AtomicU32::new(PCR_SYSCLK_CONF_RESET),
            spi0: [
                AtomicU32::new(SPI0_CTRL_RESET),
                AtomicU32::new(SPI0_CLOCK_RESET),
                AtomicU32::new(SPI0_USER_RESET),
                AtomicU32::new(SPI0_USER1_RESET),
                AtomicU32::new(SPI0_USER2_RESET),
            ],
            generation: AtomicU32::new(0),
            invalidates: Mutex::new(Vec::new()),
            invalidate_pending: AtomicBool::new(false),
        }
    }

    // ---- writers ------------------------------------------------------

    /// PCR's two words.
    pub fn set_pcr(&self, mspi_clk_conf: u32, sysclk_conf: u32) {
        self.mspi_clk_conf.store(mspi_clk_conf, Ordering::Relaxed);
        self.sysclk_conf.store(sysclk_conf, Ordering::Relaxed);
        self.generation.fetch_add(1, Ordering::Release);
    }

    /// One of SPI0's words.
    pub fn set_spi0(&self, word: Spi0Word, value: u32) {
        self.spi0[word as usize].store(value, Ordering::Relaxed);
        self.generation.fetch_add(1, Ordering::Release);
    }

    /// EXTMEM performed a range invalidate of `[vaddr, vaddr + size)`.
    pub fn post_invalidate(&self, vaddr: u32, size: u32) {
        self.invalidates.lock().unwrap().push((vaddr, size));
        self.invalidate_pending.store(true, Ordering::Release);
    }

    // ---- readers ------------------------------------------------------

    pub fn generation(&self) -> u32 {
        self.generation.load(Ordering::Acquire)
    }

    pub fn spi0(&self, word: Spi0Word) -> u32 {
        self.spi0[word as usize].load(Ordering::Relaxed)
    }

    /// The MSPI module clock, Hz.
    pub fn mspi_hz(&self) -> u64 {
        mspi_hz(
            self.mspi_clk_conf.load(Ordering::Relaxed),
            self.sysclk_conf.load(Ordering::Relaxed),
        )
    }

    /// What one 32-byte cache line fill costs, in CPU cycles: SPI0's read
    /// transfer at its SCK, plus the overhead.
    pub fn line_fill_cycles(&self) -> u32 {
        let sck = sck_hz(self.mspi_hz(), self.spi0(Spi0Word::Clock));
        let phases = cache_fill_phases(
            self.spi0(Spi0Word::Ctrl),
            self.spi0(Spi0Word::User),
            self.spi0(Spi0Word::User1),
            self.spi0(Spi0Word::User2),
        );
        transfer_cycles(phases.clocks(), sck) as u32
    }

    /// Is there an invalidate waiting? One relaxed load: what the cache
    /// cost model asks on every access to the window.
    #[inline]
    pub fn invalidate_pending(&self) -> bool {
        self.invalidate_pending.load(Ordering::Acquire)
    }

    /// The invalidates posted since the last call.
    pub fn take_invalidates(&self) -> Vec<(u32, u32)> {
        let mut list = self.invalidates.lock().unwrap();
        self.invalidate_pending.store(false, Ordering::Release);
        core::mem::take(&mut *list)
    }
}

/// The MSPI module clock from `PCR.mspi_clk_conf` and `PCR.sysclk_conf`.
///
/// `sysclk_conf.soc_clk_sel` (bits 17:16) is the root: 1 = the PLL, whose
/// MSPI branch is the 480 MHz SPLL divided by `mspi_fast_hs_div_num + 1`
/// (bits 15:8; the PAC: "3, 4, 5 → div4, div5, div6"); 0 = the crystal and
/// 2 = RC_FAST, divided by `1 << mspi_fast_ls_div_num` (bits 7:0; "0, 1, 2 →
/// div1, div2, div4").
pub fn mspi_hz(mspi_clk_conf: u32, sysclk_conf: u32) -> u64 {
    match (sysclk_conf >> 16) & 3 {
        1 => SPLL_HZ / (u64::from((mspi_clk_conf >> 8) & 0xff) + 1),
        root => {
            let src = if root == 2 { RC_FAST_HZ } else { XTAL_HZ };
            src >> ((mspi_clk_conf & 0xff).min(2))
        }
    }
}

/// SCK from the MSPI clock and a controller's `clock` register: bit 31
/// (`clk_equ_sysclk`) is one division, else `clkcnt_n` (bits 23:16) + 1.
pub fn sck_hz(mspi_hz: u64, clock: u32) -> u64 {
    if clock & (1 << 31) != 0 {
        mspi_hz
    } else {
        mspi_hz / (u64::from((clock >> 16) & 0xff) + 1)
    }
}

/// One transfer's phases: how many bits each carries and on how many lines.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Phases {
    pub cmd_bits: u32,
    pub cmd_lines: u32,
    pub addr_bits: u32,
    pub addr_lines: u32,
    pub dummy_clocks: u32,
    pub data_bits: u32,
    pub data_lines: u32,
}

impl Phases {
    /// SCK cycles on the wire.
    pub fn clocks(&self) -> u64 {
        let per = |bits: u32, lines: u32| u64::from(bits.div_ceil(lines.max(1)));
        per(self.cmd_bits, self.cmd_lines)
            + per(self.addr_bits, self.addr_lines)
            + u64::from(self.dummy_clocks)
            + per(self.data_bits, self.data_lines)
    }

    /// A single-line command of `cmd_bits`, `addr_bits` and `data_bits` —
    /// the dedicated triggers (`flash_rdsr`, `flash_se`, …).
    pub fn single(cmd_bits: u32, addr_bits: u32, data_bits: u32) -> Self {
        Self {
            cmd_bits,
            cmd_lines: 1,
            addr_bits,
            addr_lines: 1,
            dummy_clocks: 0,
            data_bits,
            data_lines: 1,
        }
    }
}

/// CPU cycles a transfer of `clocks` SCK takes at `sck_hz`, overhead
/// included. Rounded up: a transfer never ends early.
pub fn transfer_cycles(clocks: u64, sck_hz: u64) -> u64 {
    let wire =
        (u128::from(clocks) * u128::from(memmap::CPU_HZ)).div_ceil(u128::from(sck_hz.max(1)));
    wire as u64 + TRANSFER_OVERHEAD_CYCLES
}

// `ctrl` (SPI0 and SPI1 alike; PAC `spi1::ctrl`).
const CTRL_FADDR_OCT: u32 = 1 << 6;
const CTRL_FCMD_QUAD: u32 = 1 << 8;
const CTRL_FCMD_OCT: u32 = 1 << 9;
const CTRL_FASTRD_MODE: u32 = 1 << 13;
const CTRL_FREAD_DUAL: u32 = 1 << 14;
const CTRL_FREAD_QUAD: u32 = 1 << 20;
const CTRL_FREAD_DIO: u32 = 1 << 23;
const CTRL_FREAD_QIO: u32 = 1 << 24;
const CTRL_FDIN_OCT: u32 = 1 << 5;
// `user` (SPI1; PAC `spi1::user`).
const USER_FWRITE_DUAL: u32 = 1 << 12;
const USER_FWRITE_QUAD: u32 = 1 << 13;
const USER_FWRITE_DIO: u32 = 1 << 14;
const USER_FWRITE_QIO: u32 = 1 << 15;
const USER_DUMMY: u32 = 1 << 29;

/// The command phase's lines, from `ctrl`.
pub fn cmd_lines(ctrl: u32) -> u32 {
    if ctrl & CTRL_FCMD_OCT != 0 {
        8
    } else if ctrl & CTRL_FCMD_QUAD != 0 {
        4
    } else {
        1
    }
}

/// A read's (address, data) lines, from `ctrl`. The `fread_*` bits count
/// only with `fastrd_mode` set, as the PAC documents.
pub fn read_lines(ctrl: u32) -> (u32, u32) {
    let addr_oct = ctrl & CTRL_FADDR_OCT != 0;
    let data_oct = ctrl & CTRL_FDIN_OCT != 0;
    let (addr, data) = if ctrl & CTRL_FASTRD_MODE == 0 {
        (1, 1)
    } else if ctrl & CTRL_FREAD_QIO != 0 {
        (4, 4)
    } else if ctrl & CTRL_FREAD_DIO != 0 {
        (2, 2)
    } else if ctrl & CTRL_FREAD_QUAD != 0 {
        (1, 4)
    } else if ctrl & CTRL_FREAD_DUAL != 0 {
        (1, 2)
    } else {
        (1, 1)
    };
    (
        if addr_oct { 8 } else { addr },
        if data_oct { 8 } else { data },
    )
}

/// A write's (address, data) lines, from SPI1's `user`.
pub fn write_lines(user: u32) -> (u32, u32) {
    if user & USER_FWRITE_QIO != 0 {
        (4, 4)
    } else if user & USER_FWRITE_DIO != 0 {
        (2, 2)
    } else if user & USER_FWRITE_QUAD != 0 {
        (1, 4)
    } else if user & USER_FWRITE_DUAL != 0 {
        (1, 2)
    } else {
        (1, 1)
    }
}

/// The phases of one 32-byte line fill, from SPI0's registers: the command
/// (`user2`'s bit length, on `ctrl`'s command lines — always sent: the
/// cache does not use a continuous-read mode here), the address (`user1`'s
/// bit length on the read's address lines), the dummy clocks when `user`
/// asks for them, and 256 bits of data on the read's data lines.
pub fn cache_fill_phases(ctrl: u32, user: u32, user1: u32, user2: u32) -> Phases {
    let (addr_lines, data_lines) = read_lines(ctrl);
    Phases {
        cmd_bits: ((user2 >> 28) & 0xf) + 1,
        cmd_lines: cmd_lines(ctrl),
        addr_bits: ((user1 >> 26) & 0x3f) + 1,
        addr_lines,
        dummy_clocks: if user & USER_DUMMY != 0 {
            (user1 & 0x3f) + 1
        } else {
            0
        },
        data_bits: LINE_BYTES * 8,
        data_lines,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What the emulated IDF bootloader leaves for each header mode (the
    /// header-sweep record in the plan; `SPI0` words).
    const DIO: (u32, u32, u32, u32) = (0x00ac_200c, 0x7000_00c0, 0x6c00_0041, 0x7000_00bb);
    const QIO: (u32, u32, u32, u32) = (0x012c_200c, 0x7000_00c0, 0x7c00_0043, 0x7000_00eb);
    const DOUT: (u32, u32, u32, u32) = (0x0020_6000, 0x7000_00c0, 0x5c00_0047, 0x7000_003b);
    const QOUT: (u32, u32, u32, u32) = (0x0030_2000, 0x7000_00c0, 0x5c00_0047, 0x7000_006b);

    fn fill(mode: (u32, u32, u32, u32), sck: u64) -> u64 {
        let p = cache_fill_phases(mode.0, mode.1, mode.2, mode.3);
        transfer_cycles(p.clocks(), sck)
    }

    #[test]
    fn the_shipped_bootloader_leaves_an_80_mhz_sck() {
        // PLL root, `mspi_clk_conf = 0x500`, `clock = 0x8000_0000`.
        let mspi = mspi_hz(0x500, 0x2801_0200);
        assert_eq!(mspi, 80_000_000);
        assert_eq!(sck_hz(mspi, 0x8000_0000), 80_000_000);
        // Before the PLL, the same words run off the crystal.
        assert_eq!(mspi_hz(0x500, 0x2800_0200), 40_000_000);
        // The ROM's own `clock div:2` at reset clocks.
        assert_eq!(sck_hz(mspi_hz(0x300, 0x2800_0200), 0x0001_0001), 20_000_000);
        assert_eq!(sck_hz(40_000_000, 0x0003_0103), 10_000_000);
    }

    #[test]
    fn a_dio_fill_at_80_mhz_is_the_measured_338_cycles() {
        let p = cache_fill_phases(DIO.0, DIO.1, DIO.2, DIO.3);
        assert_eq!(p.clocks(), 8 + 14 + 2 + 128);
        assert_eq!(fill(DIO, 80_000_000), 338);
        assert_eq!(
            fill(DIO, 40_000_000),
            608 + TRANSFER_OVERHEAD_CYCLES,
            "the calibration's refuted 40 MHz arithmetic"
        );
    }

    #[test]
    fn the_other_header_modes_predict_their_own_fills() {
        assert_eq!(fill(QIO, 80_000_000), 2 * (8 + 8 + 4 + 64) + 34);
        assert_eq!(fill(DOUT, 80_000_000), 2 * (8 + 24 + 8 + 128) + 34);
        assert_eq!(fill(QOUT, 80_000_000), 2 * (8 + 24 + 8 + 64) + 34);
    }

    #[test]
    fn the_cell_recomputes_from_what_was_published() {
        let c = FlashClocks::new();
        let g0 = c.generation();
        c.set_pcr(0x500, 0x2801_0200);
        for (word, value) in [
            (Spi0Word::Ctrl, DIO.0),
            (Spi0Word::User, DIO.1),
            (Spi0Word::User1, DIO.2),
            (Spi0Word::User2, DIO.3),
            (Spi0Word::Clock, 0x8000_0000),
        ] {
            c.set_spi0(word, value);
        }
        assert!(c.generation() > g0);
        assert_eq!(c.line_fill_cycles(), 338);
    }

    #[test]
    fn invalidates_are_taken_once() {
        let c = FlashClocks::new();
        assert!(!c.invalidate_pending());
        c.post_invalidate(0x4275_0000, 64);
        assert!(c.invalidate_pending());
        assert_eq!(c.take_invalidates(), vec![(0x4275_0000, 64)]);
        assert!(!c.invalidate_pending());
        assert!(c.take_invalidates().is_empty());
    }

    #[test]
    fn line_widths_follow_the_ctrl_and_user_bits() {
        assert_eq!(read_lines(DIO.0), (2, 2));
        assert_eq!(read_lines(QIO.0), (4, 4));
        assert_eq!(read_lines(DOUT.0), (1, 2));
        assert_eq!(read_lines(QOUT.0), (1, 4));
        assert_eq!(read_lines(0), (1, 1));
        assert_eq!(write_lines(0), (1, 1));
        assert_eq!(write_lines(USER_FWRITE_QIO), (4, 4));
        assert_eq!(cmd_lines(0), 1);
    }
}
