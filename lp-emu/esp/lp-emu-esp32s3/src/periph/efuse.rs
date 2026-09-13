//! `EFUSE` at `0x6000_7000` — a memory-backed block seeded from an
//! [`EfuseIdentity`].
//!
//! **The C6's MAC path, base as the only parameter** (`m6/notes.md` §3.0
//! row 9): esp-hal reads eFuse fields word by word from the block's base
//! (`efuse/mod.rs:83-130`, `read_field_le`), `rd_mac_spi_sys_0..5` sit at
//! `+0x44..+0x58` on both chips, and `MAC0`/`MAC1` have the same bit
//! positions (`efuse/esp32s3/fields.rs:163-165`):
//!
//! ```text
//! rd_mac_spi_sys_0 (+0x44)  MAC0 = bits 0..32   [31:24]=MAC[2] [23:16]=MAC[3] [15:8]=MAC[4] [7:0]=MAC[5]
//! rd_mac_spi_sys_1 (+0x48)  MAC1 = bits 32..48  [15:8]=MAC[0]  [7:0]=MAC[1]
//! ```
//!
//! "MAC address is stored in big endian, so load the bytes in reverse" —
//! `efuse/mod.rs:206`, the same line on both chips.
//!
//! # ⚠️ The wafer version is NOT where the C6 keeps it
//!
//! The brief flagged this half as unverified, and it differs. The C6 packs
//! `WAFER_VERSION_MINOR` (4 bits at 114) and `_MAJOR` (2 bits at 118) into
//! `rd_mac_spi_sys_3`. The S3's `efuse/esp32s3/fields.rs` says:
//!
//! ```text
//! WAFER_VERSION_MINOR_LO = EfuseField::new(1, 3, 114, 3)   // +0x50 bits 18:20
//! WAFER_VERSION_MINOR_HI = EfuseField::new(1, 5, 183, 1)   // +0x58 bit 23
//! WAFER_VERSION_MAJOR    = EfuseField::new(1, 5, 184, 2)   // +0x58 bits 24:25
//! ```
//!
//! and `minor_chip_version()` is `(HI << 3) | LO` (`efuse/esp32s3/mod.rs:
//! 170-173`). So the minor version straddles two words and the major is in
//! `rd_mac_spi_sys_5`, not `_3`. This file takes the S3's layout; a copy of
//! the C6's would have put every revision bit in the wrong word and read
//! back `0.0` for any board.
//!
//! # No measured dump, and no synthesised one
//!
//! The classic's block 0 is the desk board's seven `espefuse` words; the S3
//! has **no fuse dump yet**. Every register here is the PAC's reset (all
//! zero in the read-only data words) plus the four identity words the
//! builder is given, whose default is itself the PAC's zero
//! ([`EfuseIdentity`]). Nothing else is seeded — not `WDT_DELAY_SEL`, not
//! the `DIG_DBIAS`/`K_*_LDO` calibration fields esp-hal's
//! `ensure_voltage_raised` reads (they read 0, `pvt_supported()` is false,
//! and esp-hal takes its uncalibrated arm), not `BLK_VERSION`. The classic's
//! `CLK8M_FREQ` lesson is why: a legal, silent zero that made the ROM
//! conclude 26 MHz for a 40 MHz board. **TODO(P09): replace the seed with
//! the desk S3's read dump.**
//!
//! Writes are accepted and remembered (nothing programs eFuses at runtime),
//! and the block reads back what was written — which is what a firmware
//! that *did* write `pgm_data` would see on silicon before a burn.

use lp_emu_esp_common::RegFile;
use lp_emu_esp_common::periph::RegGrade;

use crate::loader::EfuseIdentity;
use crate::regs;

/// The block's aperture: the generated table runs to `+0x1fc` (`date`).
pub const EFUSE_LEN: u32 = 0x200;

pub const RD_MAC_SPI_SYS_0: u32 = 0x44;
pub const RD_MAC_SPI_SYS_1: u32 = 0x48;
pub const RD_MAC_SPI_SYS_3: u32 = 0x50;
pub const RD_MAC_SPI_SYS_5: u32 = 0x58;

/// `WAFER_VERSION_MINOR_LO`: block 1 bit 114 → word 3, bit 18, three bits.
const MINOR_LO_SHIFT: u32 = 114 - 96;
/// `WAFER_VERSION_MINOR_HI`: block 1 bit 183 → word 5, bit 23, one bit.
const MINOR_HI_SHIFT: u32 = 183 - 160;
/// `WAFER_VERSION_MAJOR`: block 1 bit 184 → word 5, bits 24:25.
const MAJOR_SHIFT: u32 = 184 - 160;

/// The four words an identity occupies, in esp-hal's S3 layout.
pub fn words(id: &EfuseIdentity) -> [(u32, u32); 4] {
    let m = id.mac;
    let w0 = (u32::from(m[2]) << 24)
        | (u32::from(m[3]) << 16)
        | (u32::from(m[4]) << 8)
        | u32::from(m[5]);
    let w1 = (u32::from(m[0]) << 8) | u32::from(m[1]);
    let minor = u32::from(id.wafer_minor & 0xf);
    let w3 = (minor & 0b111) << MINOR_LO_SHIFT;
    let w5 = ((minor >> 3) << MINOR_HI_SHIFT) | (u32::from(id.wafer_major & 0b11) << MAJOR_SHIFT);
    [
        (RD_MAC_SPI_SYS_0, w0),
        (RD_MAC_SPI_SYS_1, w1),
        (RD_MAC_SPI_SYS_3, w3),
        (RD_MAC_SPI_SYS_5, w5),
    ]
}

/// esp-hal's extraction, applied to the four words: `base_mac_address` and
/// `major_chip_version` / `minor_chip_version` (`efuse/esp32s3/mod.rs`).
pub fn identity(w0: u32, w1: u32, w3: u32, w5: u32) -> EfuseIdentity {
    let mac0 = w0.to_le_bytes();
    let mac1 = (w1 & 0xffff).to_le_bytes();
    let minor_lo = ((w3 >> MINOR_LO_SHIFT) & 0b111) as u8;
    let minor_hi = ((w5 >> MINOR_HI_SHIFT) & 1) as u8;
    EfuseIdentity {
        mac: [mac1[1], mac1[0], mac0[3], mac0[2], mac0[1], mac0[0]],
        wafer_minor: (minor_hi << 3) | minor_lo,
        wafer_major: ((w5 >> MAJOR_SHIFT) & 0b11) as u8,
    }
}

/// The block, seeded.
///
/// # Grades
///
/// The PAC grades give every register the block's honest default; the four
/// identity words are promoted to `Documented` by hand, because the PAC
/// calls them read-only (a burned fuse is not writable) and
/// [`RegFile::with_pac_grades`] therefore leaves them `Modeled` on the
/// read-only rule alone — which says nothing about whether the *bits* are
/// right.
///
/// | register | grade | source |
/// |---|---|---|
/// | `rd_mac_spi_sys_0` +0x44 | `documented` | esp-hal `efuse/esp32s3/fields.rs:163` `MAC0` = bits 0..32 of block 1, big-endian (`efuse/mod.rs:206`); the bytes are this run's own `--efuse-mac` |
/// | `rd_mac_spi_sys_1` +0x48 | `documented` | `MAC1` = bits 32..48 (`fields.rs:165`) |
/// | `rd_mac_spi_sys_3` +0x50 | `documented` | `WAFER_VERSION_MINOR_LO` bits 114..117 (`fields.rs:189`) |
/// | `rd_mac_spi_sys_5` +0x58 | `documented` | `WAFER_VERSION_MINOR_HI` bit 183, `_MAJOR` bits 184..186 (`fields.rs:225-227`) |
///
/// Nothing here is `measured`, and nothing can be until P09 reads a board.
pub fn efuse(id: EfuseIdentity) -> RegFile {
    let mut rf = RegFile::new("EFUSE", EFUSE_LEN)
        .with_names(regs::EFUSE)
        .with_pac_grades()
        .with_grade(RD_MAC_SPI_SYS_0, RegGrade::Documented)
        .with_grade(RD_MAC_SPI_SYS_1, RegGrade::Documented)
        .with_grade(RD_MAC_SPI_SYS_3, RegGrade::Documented)
        .with_grade(RD_MAC_SPI_SYS_5, RegGrade::Documented);
    for (off, word) in words(&id) {
        rf.poke(off, word);
    }
    rf
}

#[cfg(test)]
mod tests {
    use super::*;
    use lp_emu_esp_common::{Peripheral, Sandbox};

    #[test]
    fn esp_hal_extraction_gives_the_configured_identity_back_both_ways() {
        let id = EfuseIdentity {
            mac: [0xa0, 0xf2, 0x62, 0x87, 0xb4, 0x8c],
            wafer_major: 0,
            wafer_minor: 2,
        };
        let [(_, w0), (_, w1), (_, w3), (_, w5)] = words(&id);
        assert_eq!(w0, 0x6287_b48c, "MAC[2..6] big-endian in rd_mac_spi_sys_0");
        assert_eq!(
            w1, 0x0000_a0f2,
            "MAC[0..2] in the low half of rd_mac_spi_sys_1"
        );
        assert_eq!(w3, 2 << 18, "minor 2 fits in the three low bits at 114");
        assert_eq!(w5, 0, "no high minor bit, major 0");
        assert_eq!(identity(w0, w1, w3, w5), id);

        // A minor version past 7 needs the high bit in the OTHER word — the
        // split a C6 copy would have lost.
        let other = EfuseIdentity {
            mac: [0x11, 0x22, 0x33, 0x44, 0x55, 0x66],
            wafer_major: 1,
            wafer_minor: 9,
        };
        let [(_, a), (_, b), (_, c), (_, d)] = words(&other);
        assert_eq!(c, 1 << 18, "9 & 0b111 = 1 in rd_mac_spi_sys_3");
        assert_eq!(
            d,
            (1 << 23) | (1 << 24),
            "9 >> 3 = 1 at bit 183; major 1 at bit 184"
        );
        assert_eq!(identity(a, b, c, d), other);
    }

    #[test]
    fn the_block_reads_the_words_where_esp_hal_looks_and_the_pacs_zero_elsewhere() {
        let mut sb = Sandbox::new();
        let id = EfuseIdentity {
            mac: [0xa0, 0xf2, 0x62, 0x87, 0xb4, 0x8c],
            wafer_major: 0,
            wafer_minor: 2,
        };
        let mut e = efuse(id);
        assert_eq!(sb.read(&mut e, RD_MAC_SPI_SYS_0), 0x6287_b48c);
        assert_eq!(sb.read(&mut e, RD_MAC_SPI_SYS_1), 0x0000_a0f2);
        assert_eq!(sb.read(&mut e, RD_MAC_SPI_SYS_3), 2 << 18);
        assert_eq!(sb.read(&mut e, RD_MAC_SPI_SYS_5), 0);
        // WDT_DELAY_SEL: block 0 bit 80 = rd_repeat_data1 (+0x34) bits
        // 16:17; the PAC's zero, not a seed, so `rwdt_multiplier()` is 0.
        assert_eq!(sb.read(&mut e, 0x34) & (0b11 << 16), 0);
        assert_eq!(e.reg_name(RD_MAC_SPI_SYS_0), Some("rd_mac_spi_sys_0"));
        assert_eq!(e.reg_grade(RD_MAC_SPI_SYS_5), Some(RegGrade::Documented));
    }

    /// The default identity is the PAC's zero, not a plausible board.
    #[test]
    fn the_default_identity_is_the_absence_of_a_dump() {
        let mut sb = Sandbox::new();
        let mut e = efuse(EfuseIdentity::default());
        for off in [
            RD_MAC_SPI_SYS_0,
            RD_MAC_SPI_SYS_1,
            RD_MAC_SPI_SYS_3,
            RD_MAC_SPI_SYS_5,
        ] {
            assert_eq!(sb.read(&mut e, off), regs::EFUSE.reset(off).unwrap_or(0));
        }
    }
}
