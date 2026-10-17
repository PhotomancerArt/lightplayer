//! How long a NOR part stays busy after a program or an erase — the
//! write-in-progress (WIP) time a guest polls the status register through.
//!
//! **Off unless a chip asks for it.** A [`FlashEngine`] without a
//! [`NorTiming`] completes every command inside the write, as it always has,
//! and WIP reads 0. With one, a program or an erase the part executed holds
//! WIP set until the busy time has passed in guest cycles, and the guest's
//! own poll loop (a mask ROM's `Wait_SPI_Idle`) spins on it exactly as it
//! does on silicon. Nothing here shortens or skips that spin: the time is the
//! guest's own instructions polling, so interrupts the guest masked stay
//! masked across it.
//!
//! The numbers belong to a **part**, not to a SoC: a different flash chip on
//! the same controller is a different [`NorTiming`]. The one this emulator
//! has measurements for is [`NorTiming::CX1`].
//!
//! [`FlashEngine`]: super::spi_flash::FlashEngine

/// One flash part's busy times, in nanoseconds of real time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NorTiming {
    /// Busy time of one program command, per started [`PROGRAM_UNIT`] bytes
    /// it carries.
    pub program_ns_per_unit: u64,
    /// Busy time of one 4 KiB sector erase.
    pub sector_erase_ns: u64,
    /// Busy time of one 64 KiB block erase.
    pub block_erase_ns: u64,
    /// Busy time of a write to the status register (`WRSR`).
    pub write_status_ns: u64,
}

/// The program size the C6's mask ROM sends (`esp_rom_spiflash_write` cuts a
/// write into 32-byte page programs; `docs/reports/2026-10-08-c6-nor-tear-calibration.md`
/// found the tear unit is this command, not the 256-byte page). A program of
/// any other size is billed per started unit of this many bytes.
pub const PROGRAM_UNIT: u32 = 32;

impl NorTiming {
    /// CX1's part (JEDEC `0x46 0x40 0x16`, 4 MiB; the tree store's desk C6).
    ///
    /// | figure | value | grade | source |
    /// |---|---|---|---|
    /// | 32-byte program | 24 µs | `measured` (fitted) | `flash-tears` `ft-timing`: a 256-byte write (eight ROM programs, each with its WREN and poll) takes 304–369 µs on CX1 (medians of the per-boot minimum and maximum: 318 and 353 µs, 510 boots); the same payload at `t4` spends 142 µs of a write in its own instructions and wire time, and 24 µs a program brings the write to ~335 µs, the middle of silicon's |
    /// | 4 KiB sector erase | 21 ms | `measured` | the first-boot format of 176 mostly blank sectors: 3.92 s (tree-store A3, `main`), 3.3–3.9 s across runs, less its CPU. The `ft-timing` scratch sector, cycled ~500 times, erases in 21.1–32.6 ms (median 26.7) — the spread with wear and content this single number does not model |
    /// | 64 KiB block erase | 105 ms | **estimate** | 5 × the sector erase, the ratio of the typical figures in the common 32 Mbit NOR datasheets; no silicon figure, and the product never block-erases `lpfs` |
    /// | status write | 0 | **not modelled** | no silicon figure for this part; the ROM writes the status once per `FlashStorage` (its unlock) |
    pub const CX1: Self = Self {
        program_ns_per_unit: 24_000,
        sector_erase_ns: 21_000_000,
        block_erase_ns: 105_000_000,
        write_status_ns: 0,
    };

    /// Busy time of a program command carrying `len` bytes.
    pub fn program_ns(&self, len: u32) -> u64 {
        u64::from(len.div_ceil(PROGRAM_UNIT).max(1)) * self.program_ns_per_unit
    }

    /// Busy time of an erase of `len` bytes: a sector, a block, or (any
    /// other length) a whole chip of that many bytes, billed as its blocks.
    pub fn erase_ns(&self, len: u32) -> u64 {
        match len {
            super::spi_flash::SECTOR_LEN => self.sector_erase_ns,
            super::spi_flash::BLOCK_LEN => self.block_erase_ns,
            other => u64::from(other.div_ceil(super::spi_flash::BLOCK_LEN)) * self.block_erase_ns,
        }
    }
}

/// Nanoseconds to guest cycles at `cpu_hz`, rounded up: a part is never
/// done *before* its time.
pub fn ns_to_cycles(ns: u64, cpu_hz: u64) -> u64 {
    (u128::from(ns) * u128::from(cpu_hz)).div_ceil(1_000_000_000) as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::spi_flash::{BLOCK_LEN, SECTOR_LEN};

    #[test]
    fn a_program_is_billed_per_started_rom_unit() {
        let t = NorTiming::CX1;
        assert_eq!(t.program_ns(32), 24_000);
        assert_eq!(t.program_ns(1), 24_000);
        assert_eq!(t.program_ns(33), 48_000);
        assert_eq!(t.program_ns(256), 8 * 24_000);
        assert_eq!(t.program_ns(0), 24_000, "a program command is never free");
    }

    #[test]
    fn an_erase_is_billed_by_its_granule() {
        let t = NorTiming::CX1;
        assert_eq!(t.erase_ns(SECTOR_LEN), 21_000_000);
        assert_eq!(t.erase_ns(BLOCK_LEN), 105_000_000);
        assert_eq!(
            t.erase_ns(4 << 20),
            64 * 105_000_000,
            "a 4 MiB chip is 64 blocks"
        );
    }

    #[test]
    fn nanoseconds_become_cycles_rounding_up() {
        assert_eq!(ns_to_cycles(21_000_000, 160_000_000), 3_360_000);
        assert_eq!(ns_to_cycles(1, 160_000_000), 1);
        assert_eq!(ns_to_cycles(0, 160_000_000), 0);
    }
}
