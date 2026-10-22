//! The flash controllers' registers a core's read-mode switch touches, and
//! the checks it makes before it trusts the switch: one definition for every
//! chip whose flash sits behind two `SPI_MEM` controllers (the C6, the S3).
//!
//! A chip's switch (`fw-esp32c6/src/flash_qio.rs`, `fw-esp32s3/src/flash_qio.rs`)
//! runs with its flash cache suspended, from RAM, and calls only mask-ROM
//! code: so everything here that it calls is `#[inline(always)]`, takes the
//! two controllers' bases as plain values ([`Mspi`]), and reads no
//! `.rodata` (a table would be in flash). The chip crate owns what differs:
//! where its controllers are (the C6's SPI0 is at `0x6000_2000`, the S3's at
//! `0x6000_3000`, the other way round), which caches it suspends, its ROM
//! entry points, and the steps only it takes (the S3's clock).
//!
//! The register offsets and bits are the same in both chips' `SPI_MEM`
//! blocks (the C6 TRM's register summary; the S3's PAC, `esp32s3` 0.35.2
//! `spi0.rs`/`spi1.rs`): `ctrl` 0x08, `user` 0x18, `user1` 0x1c, `user2`
//! 0x20, `rd_status` 0x2c, `cache_fctrl` 0x3c, `w0` 0x58; `ctrl.fread_dio`
//! bit 23, `fread_qio` bit 24; `cmd.flash_rdid` bit 28.

use core::ptr::{read_volatile, write_volatile};

/// `SPI_MEM_CMD`.
pub const CMD: usize = 0x00;
/// `SPI_MEM_CTRL`: the read-mode bits.
pub const CTRL: usize = 0x08;
/// `SPI_MEM_USER`.
pub const USER: usize = 0x18;
/// `SPI_MEM_USER1`: address bit length (bits 31:26) and dummy cycles (5:0).
pub const USER1: usize = 0x1c;
/// `SPI_MEM_USER2`: the read command (bits 15:0).
pub const USER2: usize = 0x20;
/// SPI0: `SPI_MEM_RD_STATUS`, whose `WB_MODE` field is the QIO mode byte.
pub const RD_STATUS: usize = 0x2c;
/// SPI0: `SPI_MEM_CACHE_FCTRL`.
pub const FCTRL: usize = 0x3c;
/// SPI1: the first data word (`RDID`'s answer).
pub const W0: usize = 0x58;

/// `SPI_MEM_CMD.SPI_MEM_FLASH_RDID`.
pub const CMD_RDID: u32 = 1 << 28;
/// `SPI_MEM_CTRL.SPI_MEM_FREAD_DIO`.
pub const FREAD_DIO: u32 = 1 << 23;
/// `SPI_MEM_CTRL.SPI_MEM_FREAD_QIO`.
pub const FREAD_QIO: u32 = 1 << 24;
/// The QIO read command.
pub const CMD_QIO_READ: u32 = 0xeb;
/// `SPIMasterReadModeCnfig`'s mode argument for QIO (the ROM's read-mode
/// enum: QIO, QOUT, DIO, DOUT, FASTRD, SLOWRD — the same on both chips).
pub const ROM_MODE_QIO: u32 = 0;

/// Where a switch's read-back check reads: the start of the flash, which
/// holds a bootloader image under every door (never erased, never all ones).
pub const CHECK_AT: u32 = 0;
/// How many words the read-back check compares.
pub const CHECK_WORDS: usize = 8;

/// A switch's verdict, as a byte: what a RAM function returns without asking
/// for anything else.
pub const SWITCH_ON: u8 = 0;
/// The controller did not read back as the switch left it.
pub const NOT_SET_AFTER: u8 = 1;
/// The bytes read after the switch differed from the bytes read before.
pub const READ_BACK_DIFFERED: u8 = 2;

/// The two flash controllers of one chip: SPI0 (the cache's) and SPI1 (the
/// one the ROM's flash routines and esp-storage drive).
#[derive(Clone, Copy)]
pub struct Mspi {
    pub spi0: usize,
    pub spi1: usize,
}

impl Mspi {
    #[inline(always)]
    pub fn rd(addr: usize) -> u32 {
        // SAFETY: a register of the chip's own SPI_MEM block.
        unsafe { read_volatile(addr as *const u32) }
    }

    #[inline(always)]
    pub fn wr(addr: usize, value: u32) {
        // SAFETY: as `rd`.
        unsafe { write_volatile(addr as *mut u32, value) }
    }

    /// The part's JEDEC id, as `SPI1.W0` holds it after the controller's own
    /// `RDID` (low 24 bits: manufacturer, type, capacity). Reads only.
    #[inline(always)]
    pub fn rdid(self) -> u32 {
        Self::wr(self.spi1 + CMD, CMD_RDID);
        while Self::rd(self.spi1 + CMD) & CMD_RDID != 0 {}
        Self::rd(self.spi1 + W0) & 0x00ff_ffff
    }

    /// SPI0's `ctrl`: its read-mode bits.
    #[inline(always)]
    pub fn spi0_ctrl(self) -> u32 {
        Self::rd(self.spi0 + CTRL)
    }

    /// Whether both controllers read back as the ROM's QIO read mode leaves
    /// them: QIO, command `0xEB`, a 32-bit address phase, and two more dummy
    /// cycles than `saved` (the DIO set) had.
    #[inline(always)]
    pub fn reads_as_qio(self, saved: &SavedReadMode) -> bool {
        let user1 = Self::rd(self.spi0 + USER1);
        Self::rd(self.spi0 + CTRL) & (FREAD_DIO | FREAD_QIO) == FREAD_QIO
            && Self::rd(self.spi1 + CTRL) & (FREAD_DIO | FREAD_QIO) == FREAD_QIO
            && Self::rd(self.spi0 + USER2) & 0xffff == CMD_QIO_READ
            && user1 >> 26 == 31
            && user1 & 0x3f == (saved.spi0_user1 & 0x3f) + 2
    }
}

/// The registers the ROM's `SPIMasterReadModeCnfig` writes, as they were.
/// Spelled out rather than a table: a table would be `.rodata`, in flash,
/// and these run with the cache suspended.
pub struct SavedReadMode {
    pub spi0_ctrl: u32,
    pub spi0_user: u32,
    pub spi0_user1: u32,
    pub spi0_user2: u32,
    pub spi0_rd_status: u32,
    pub spi0_fctrl: u32,
    pub spi1_ctrl: u32,
}

impl SavedReadMode {
    #[inline(always)]
    pub fn take(m: Mspi) -> Self {
        SavedReadMode {
            spi0_ctrl: Mspi::rd(m.spi0 + CTRL),
            spi0_user: Mspi::rd(m.spi0 + USER),
            spi0_user1: Mspi::rd(m.spi0 + USER1),
            spi0_user2: Mspi::rd(m.spi0 + USER2),
            spi0_rd_status: Mspi::rd(m.spi0 + RD_STATUS),
            spi0_fctrl: Mspi::rd(m.spi0 + FCTRL),
            spi1_ctrl: Mspi::rd(m.spi1 + CTRL),
        }
    }

    #[inline(always)]
    pub fn put_back(&self, m: Mspi) {
        Mspi::wr(m.spi0 + CTRL, self.spi0_ctrl);
        Mspi::wr(m.spi0 + USER, self.spi0_user);
        Mspi::wr(m.spi0 + USER1, self.spi0_user1);
        Mspi::wr(m.spi0 + USER2, self.spi0_user2);
        Mspi::wr(m.spi0 + RD_STATUS, self.spi0_rd_status);
        Mspi::wr(m.spi0 + FCTRL, self.spi0_fctrl);
        Mspi::wr(m.spi1 + CTRL, self.spi1_ctrl);
    }
}

/// Whether the two read-back runs differ in any bit. A failed ROM read
/// counts as a difference: pass its nonzero status in `failed`.
#[inline(always)]
pub fn differs(before: &[u32; CHECK_WORDS], after: &[u32; CHECK_WORDS], failed: u32) -> bool {
    let mut d = failed;
    let mut i = 0;
    while i < CHECK_WORDS {
        d |= before[i] ^ after[i];
        i += 1;
    }
    d != 0
}

/// The switch's verdict: [`NOT_SET_AFTER`] before [`READ_BACK_DIFFERED`],
/// [`SWITCH_ON`] when both hold.
#[inline(always)]
pub fn verdict(set_after: bool, differed: bool) -> u8 {
    if !set_after {
        NOT_SET_AFTER
    } else if differed {
        READ_BACK_DIFFERED
    } else {
        SWITCH_ON
    }
}

/// The words a boot line uses for a verdict that is not [`SWITCH_ON`].
pub fn verdict_words(v: u8) -> &'static str {
    match v {
        NOT_SET_AFTER => "the controller did not read back as set",
        READ_BACK_DIFFERED => "the read-back differed",
        _ => "on",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_verdict_names_the_register_check_before_the_read_back() {
        assert_eq!(verdict(true, false), SWITCH_ON);
        assert_eq!(verdict(false, true), NOT_SET_AFTER);
        assert_eq!(verdict(false, false), NOT_SET_AFTER);
        assert_eq!(verdict(true, true), READ_BACK_DIFFERED);
    }

    #[test]
    fn any_differing_bit_or_a_failed_read_differs() {
        let a = [0x1234_5678; CHECK_WORDS];
        let mut b = a;
        assert!(!differs(&a, &b, 0));
        assert!(differs(&a, &b, 1));
        b[CHECK_WORDS - 1] ^= 0x8000_0000;
        assert!(differs(&a, &b, 0));
    }

    #[test]
    fn the_offsets_are_the_spi_mem_blocks() {
        assert_eq!((CTRL, USER, USER1, USER2), (0x08, 0x18, 0x1c, 0x20));
        assert_eq!((RD_STATUS, FCTRL, W0), (0x2c, 0x3c, 0x58));
        assert_eq!(
            (FREAD_DIO, FREAD_QIO, CMD_RDID),
            (1 << 23, 1 << 24, 1 << 28)
        );
    }
}
