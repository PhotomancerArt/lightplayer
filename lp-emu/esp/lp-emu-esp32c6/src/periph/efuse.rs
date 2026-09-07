//! `EFUSE` at `0x600B_0800` — a memory-backed block seeded from an
//! [`EfuseIdentity`].
//!
//! esp-hal reads eFuse fields word by word from the block's base
//! (`efuse/mod.rs:83-130`, `read_field_le`), so a register file whose words
//! hold the right bits is the whole model. The layout, from
//! `esp-hal-1.1.1/src/efuse/esp32c6/fields.rs` and `efuse/mod.rs:204-217`
//! (discovery §8):
//!
//! ```text
//! rd_mac_spi_sys_0 (+0x44)  MAC0 = bits 0..32   [31:24]=MAC[2] [23:16]=MAC[3] [15:8]=MAC[4] [7:0]=MAC[5]
//! rd_mac_spi_sys_1 (+0x48)  MAC1 = bits 32..48  [15:8]=MAC[0]  [7:0]=MAC[1]; MAC_EXT = bits 48..64 (0)
//! rd_mac_spi_sys_3 (+0x50)  WAFER_VERSION_MINOR = bits 114..118 → [21:18]; MAJOR = bits 118..120 → [23:22]
//! rd_repeat_data*  (+0x30..)  zero; WDT_DELAY_SEL (block 0 word 2 bits 80..82 = +0x34 [17:16]) = 0
//! ```
//!
//! "MAC address is stored in big endian, so load the bytes in reverse" —
//! `efuse/mod.rs:206`. The test applies esp-hal's extraction to these words
//! and gets the configured identity back, and derives the words from the
//! identity the other way round.
//!
//! Writes are accepted and remembered (nothing programs eFuses at runtime),
//! and the block reads back what was written — which is what a firmware
//! that *did* write `pgm_data` would see on silicon before a burn.

use lp_emu_esp_common::RegFile;

use crate::loader::EfuseIdentity;
use crate::regs;

pub const RD_MAC_SPI_SYS_0: u32 = 0x44;
pub const RD_MAC_SPI_SYS_1: u32 = 0x48;
pub const RD_MAC_SPI_SYS_3: u32 = 0x50;

/// The three words an identity occupies, in esp-hal's layout.
pub fn words(id: &EfuseIdentity) -> [(u32, u32); 3] {
    let m = id.mac;
    let w0 = (u32::from(m[2]) << 24) | (u32::from(m[3]) << 16) | (u32::from(m[4]) << 8) | u32::from(m[5]);
    let w1 = (u32::from(m[0]) << 8) | u32::from(m[1]);
    let w3 = (u32::from(id.wafer_minor & 0xf) << 18) | (u32::from(id.wafer_major & 0x3) << 22);
    [(RD_MAC_SPI_SYS_0, w0), (RD_MAC_SPI_SYS_1, w1), (RD_MAC_SPI_SYS_3, w3)]
}

/// esp-hal's extraction, applied to the three words.
pub fn identity(w0: u32, w1: u32, w3: u32) -> EfuseIdentity {
    let mac0 = w0.to_le_bytes();
    let mac1 = (w1 & 0xffff).to_le_bytes();
    EfuseIdentity {
        mac: [mac1[1], mac1[0], mac0[3], mac0[2], mac0[1], mac0[0]],
        wafer_minor: ((w3 >> 18) & 0xf) as u8,
        wafer_major: ((w3 >> 22) & 0x3) as u8,
    }
}

/// The block, seeded.
pub fn efuse(id: EfuseIdentity) -> RegFile {
    let mut rf = RegFile::new("EFUSE", 0x200).with_names(regs::EFUSE);
    for (off, word) in words(&id) {
        rf.poke(off, word);
    }
    rf
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::loader::DESK_MAC;
    use lp_emu_esp_common::{Peripheral, Sandbox};

    #[test]
    fn esp_hal_extraction_gives_the_configured_identity_back_both_ways() {
        let id = EfuseIdentity {
            mac: DESK_MAC,
            wafer_major: 0,
            wafer_minor: 2,
        };
        let [(_, w0), (_, w1), (_, w3)] = words(&id);
        assert_eq!(w0, 0x6287_b48c, "MAC[2..6] big-endian in rd_mac_spi_sys_0");
        assert_eq!(w1, 0x0000_a0f2, "MAC[0..2] in the low half of rd_mac_spi_sys_1");
        assert_eq!(w3, 2 << 18);
        assert_eq!(identity(w0, w1, w3), id);

        let other = EfuseIdentity {
            mac: [0x11, 0x22, 0x33, 0x44, 0x55, 0x66],
            wafer_major: 1,
            wafer_minor: 3,
        };
        let [(_, a), (_, b), (_, c)] = words(&other);
        assert_eq!(identity(a, b, c), other);
    }

    #[test]
    fn the_block_reads_the_words_where_esp_hal_looks_and_zero_elsewhere() {
        let mut sb = Sandbox::new();
        let mut e = efuse(EfuseIdentity::default());
        assert_eq!(sb.read(&mut e, RD_MAC_SPI_SYS_0), 0x6287_b48c);
        assert_eq!(sb.read(&mut e, RD_MAC_SPI_SYS_1), 0x0000_a0f2);
        assert_eq!(sb.read(&mut e, RD_MAC_SPI_SYS_3), 2 << 18);
        // WDT_DELAY_SEL: block 0 word 2 = rd_repeat_data1 at +0x34, bits 16:17.
        assert_eq!(sb.read(&mut e, 0x34) & (0b11 << 16), 0);
        assert_eq!(e.reg_name(RD_MAC_SPI_SYS_0), Some("rd_mac_spi_sys_0"));
    }
}
