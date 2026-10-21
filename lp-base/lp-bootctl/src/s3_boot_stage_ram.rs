//! The RAM the ESP32-S3's boot stages run in, which the app may borrow but
//! no radio DMA may reach: the S3's twin of [`crate::BOOT_STAGE_RAM`].
//!
//! The hazard is the C6's (`boot_stage_ram`): after a warm reset a radio's
//! DMA can still be writing into buffers the previous app gave it, and a
//! boot stage whose code, data or stack sits on one is corrupted mid-boot
//! (`docs/defects/2026-10-05-a-requested-reboot-crashed-the-c6-bootloader.md`).
//! The S3 has two boot stages before the core, in two places:
//!
//! - **the split image's loader**, [`S3_LOADER_RAM`]: code through the
//!   instruction-bus alias ([`S3_LOADER_IRAM`]), data and stack through the
//!   data bus. Every core's `.bss` covers it, so the core zeroes it after
//!   the jump. Claimed by the loader forever (the S3 plan's ruling L4).
//! - **the ESP-IDF second-stage bootloader** espflash bundles,
//!   [`S3_IDF_BOOTLOADER_RAM`]: espflash 3.3.0's `esp32s3-bootloader.bin`
//!   loads its code at I `0x403C_9700..0x403C_A304` and
//!   `0x403C_C700..0x403C_F5B0` (D `0x3FCD_9700..` through the alias) and
//!   its data at `0x3FCE_3818..0x3FCE_4F10`, and runs on the mask ROM's
//!   stack below `0x3FCE_D710` — the end of `dram2_seg`
//!   (`0x3FCD_B700..0x3FCE_D710`, esp-hal's `ld/esp32s3/memory.x`), "memory
//!   available after the 2nd stage bootloader is finished". Its lowest code
//!   segment sits 8 KiB **below** `dram2_seg`, at the top of `dram_seg`,
//!   where the app's main stack is. Read off the image's own segment table,
//!   2026-10-21.
//!
//! Both ranges are data-bus addresses: SRAM1 is one memory with two views,
//! and an address is fenced in either. The instruction bus is the data
//! bus plus [`SRAM1_I_FROM_D`].

use core::ops::Range;

/// What the instruction bus adds to a data-bus address in SRAM1: the loader
/// fetches `0x403A_0000` = reads `0x3FCB_0000`.
pub const SRAM1_I_FROM_D: u32 = 0x6F_0000;

/// The S3 loader's RAM, data bus: code at the bottom (fetched through
/// [`S3_LOADER_IRAM`]), data and stack above. Inside every core's `.bss`,
/// clear of every segment the loader copies, and below the IDF
/// bootloader's RAM, which is still live while the loader is loaded.
pub const S3_LOADER_RAM: Range<u32> = 0x3FCB_0000..0x3FCC_0000;
/// [`S3_LOADER_RAM`] on the instruction bus.
pub const S3_LOADER_IRAM: Range<u32> =
    S3_LOADER_RAM.start + SRAM1_I_FROM_D..S3_LOADER_RAM.end + SRAM1_I_FROM_D;

/// The ESP-IDF second-stage bootloader's RAM, data bus: from its lowest code
/// segment (I `0x403C_9700`) to the top of the ROM stack it runs on
/// (`dram2_seg`'s end).
pub const S3_IDF_BOOTLOADER_RAM: Range<u32> = 0x3FCD_9700..0x3FCE_D710;

/// Every boot stage's RAM on the S3, lowest first.
pub const S3_BOOT_STAGE_RAM: [Range<u32>; 2] = [S3_LOADER_RAM, S3_IDF_BOOTLOADER_RAM];

const _: () = {
    assert!(S3_LOADER_RAM.end <= S3_IDF_BOOTLOADER_RAM.start);
    assert!(S3_LOADER_IRAM.start == 0x403A_0000);
};

/// Cut the array `start..start + len` where it first reaches one of
/// [`S3_BOOT_STAGE_RAM`]: `(below, from)`, the lengths of the part that may
/// hold radio DMA and of the part that may not, `below + from == len`.
///
/// As [`crate::split_at_boot_stage_ram`] does, the part that may not runs
/// from that range's start to the array's end (one cut, one extra allocator
/// region). An array that reaches no range is returned whole as `below`.
pub fn split_at_s3_boot_stage_ram(start: u32, len: u32) -> (u32, u32) {
    let end = start.saturating_add(len);
    for range in S3_BOOT_STAGE_RAM {
        if end > range.start && start < range.end {
            let below = range.start.saturating_sub(start);
            return (below, len - below);
        }
    }
    (len, 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_loader_is_inside_the_cores_bss_and_below_the_bootloader() {
        // The spike's core: `.bss` 0x3FC8_C128..0x3FCC_E8C8.
        assert!(0x3FC8_C128 <= S3_LOADER_RAM.start && S3_LOADER_RAM.end <= 0x3FCC_E8C8);
        assert_eq!(S3_LOADER_IRAM, 0x403A_0000..0x403B_0000);
    }

    #[test]
    fn the_bootloader_range_covers_its_image_and_dram2_seg() {
        // espflash 3.3.0's `esp32s3-bootloader.bin` segments.
        for (load, end) in [
            (0x3FCE_3818, 0x3FCE_4F10),
            (0x403C_9700 - SRAM1_I_FROM_D, 0x403C_A304 - SRAM1_I_FROM_D),
            (0x403C_C700 - SRAM1_I_FROM_D, 0x403C_F5B0 - SRAM1_I_FROM_D),
        ] {
            assert!(S3_IDF_BOOTLOADER_RAM.start <= load && end <= S3_IDF_BOOTLOADER_RAM.end);
        }
        // `dram2_seg`.
        assert!(S3_IDF_BOOTLOADER_RAM.start <= 0x3FCD_B700);
        assert_eq!(S3_IDF_BOOTLOADER_RAM.end, 0x3FCE_D710);
    }

    #[test]
    fn an_array_below_every_range_is_whole() {
        assert_eq!(split_at_s3_boot_stage_ram(0x3FC8_8000, 0x1000), (0x1000, 0));
    }

    #[test]
    fn an_arena_reaching_the_loader_is_cut_there() {
        let (below, from) = split_at_s3_boot_stage_ram(0x3FC9_0000, 0x3_C000);
        assert_eq!(0x3FC9_0000 + below, S3_LOADER_RAM.start);
        assert_eq!(below + from, 0x3_C000);
    }

    #[test]
    fn an_array_between_the_ranges_is_whole() {
        assert_eq!(
            split_at_s3_boot_stage_ram(S3_LOADER_RAM.end, 0x1_0000),
            (0x1_0000, 0)
        );
    }

    #[test]
    fn dram2_seg_is_all_fenced() {
        assert_eq!(
            split_at_s3_boot_stage_ram(0x3FCD_B700, 0x3FCE_D710 - 0x3FCD_B700),
            (0, 0x3FCE_D710 - 0x3FCD_B700)
        );
    }

    #[test]
    fn an_array_above_every_range_is_whole() {
        assert_eq!(split_at_s3_boot_stage_ram(0x3FCF_0000, 0x8000), (0x8000, 0));
    }
}
