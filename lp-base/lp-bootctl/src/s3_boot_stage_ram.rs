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
//!   instruction-bus alias ([`S3_LOADER_IRAM`]), data through the data bus.
//!   Every core's `.bss` covers it, so the core zeroes it after the jump.
//!   Claimed by the loader forever (the S3 plan's ruling L4, amended
//!   2026-10-22: see [`S3_LOADER_RAM`] for where and why).
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
/// fetches `0x403B_5000` = reads `0x3FCC_5000`.
pub const SRAM1_I_FROM_D: u32 = 0x6F_0000;

/// The S3 loader's RAM, data bus: code in the low half (fetched through
/// [`S3_LOADER_IRAM`]), data in the high half. 16 KiB; the loader is about
/// 3.6 KB.
///
/// **At the top of the core's heap arena, not in its middle** (L4 amended
/// 2026-10-22). A core's `dram_seg` runs, bottom up: its loaded segments
/// (`.rwtext` through the alias, `.data`), `.bss` — the radio's heap region,
/// the main heap arena, the other statics — and the main stack up to
/// `0x3FCD_B700`. The core registers its arena cut around this range (radio
/// DMA may not land here across a warm reset, so the piece inside carries no
/// capability tag), so a range in the arena's middle splits it in three:
/// at `0x3FCB_0000..0x3FCC_0000` it left the Bluetooth image's largest free
/// block at 73,744 B. Here the arena keeps one piece below it.
///
/// It ends at `0x3FCC_9000` (I `0x403B_9000`), where esp-hal's S3
/// `memory.x` says the bootloader's startup code may begin: espflash 3.3's
/// bundled bootloader starts at I `0x403C_9700` ([`S3_IDF_BOOTLOADER_RAM`]),
/// and the margin is for a later IDF bootloader growing down. It is inside
/// every core's `.bss` (`0x3FC9_40E8..0x3FCC_E2E6` at main `ccfa116b0`),
/// clear of every segment the loader copies, and below the IDF bootloader's
/// RAM, which is still live while the loader runs. A core whose `.bss` ends
/// below it puts this range in its main stack instead: RAM the core neither
/// loads nor reads before writing, which is all the loader needs.
pub const S3_LOADER_RAM: Range<u32> = 0x3FCC_5000..0x3FCC_9000;
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
    assert!(S3_LOADER_IRAM.end == 0x403B_9000);
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
        // The shipped split core at main `ccfa116b0` (`p2.elf`): `.bss`
        // 0x3FC9_40E8..0x3FCC_E2E6.
        assert!(0x3FC9_40E8 <= S3_LOADER_RAM.start && S3_LOADER_RAM.end <= 0x3FCC_E2E6);
        assert_eq!(S3_LOADER_IRAM, 0x403B_5000..0x403B_9000);
        // Below where esp-hal says the bootloader's startup IRAM may begin.
        assert!(S3_LOADER_IRAM.end <= 0x403B_9000);
    }

    /// The point of the 2026-10-22 amendment: the shipped Bluetooth image's
    /// heap arena (`HEAP_MAIN`, 0x3FC9_C108 + 172,032 B at `ccfa116b0`) is
    /// cut once, near its top, not in the middle.
    #[test]
    fn the_shipped_arena_keeps_one_piece_below_the_loader() {
        let (below, from) = split_at_s3_boot_stage_ram(0x3FC9_C108, 172_032);
        assert_eq!(0x3FC9_C108 + below, S3_LOADER_RAM.start);
        assert_eq!(below, 167_672);
        assert_eq!(from, 4_360);
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
