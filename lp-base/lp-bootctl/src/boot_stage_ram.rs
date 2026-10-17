//! The RAM the C6's boot stages run in, which the app may borrow but no
//! radio DMA may reach.
//!
//! After an HP-only warm reset (a requested reboot, an update's reset, the
//! host's RTS) the modem keeps running: its DMA engines still own whatever
//! buffers the previous app gave them, and keep writing into them while the
//! next boot runs. A boot stage whose code, data or stack sits on such a
//! buffer is corrupted mid-boot. That is the crash of
//! `docs/defects/2026-10-05-a-requested-reboot-crashed-the-c6-bootloader.md`
//! (the IDF bootloader, in `dram2_seg`), and the loader's own range has the
//! same exposure (`docs/defects/2026-10-17-radio-heap-overflow-reached-the-split-loaders-ram.md`).
//!
//! [`BOOT_STAGE_RAM`] is the split image's loader (`loader.x`'s `LRAM`:
//! code at the bottom, stack at the top). The app's `.bss` covers it once the
//! core runs, so the firmware borrows it for its heap, but only for requests
//! no radio makes: [`split_at_boot_stage_ram`] says where a heap array has to
//! be cut so the part from this range up is registered without the
//! capability the radio's allocations ask for.

use core::ops::Range;

/// The split image's loader RAM: `fw-esp32c6-loader/loader.x`'s `LRAM`
/// (its test holds the two equal). The loader is never updated over the air,
/// so this range binds for every board that ever ran a split image.
pub const BOOT_STAGE_RAM: Range<u32> = 0x4085_0000..0x4086_0000;

/// Cut the array `start..start + len` where it reaches [`BOOT_STAGE_RAM`]:
/// `(below, from)`, the lengths of the part that may hold radio DMA and of
/// the part that may not, `below + from == len`.
///
/// The part that may not runs from the range's start to the array's end,
/// above the range too, rather than only the overlap: one cut keeps the heap
/// at one extra allocator region. An array wholly above or below the range
/// is returned whole as `below`.
pub fn split_at_boot_stage_ram(start: u32, len: u32) -> (u32, u32) {
    let end = start.saturating_add(len);
    if end <= BOOT_STAGE_RAM.start || start >= BOOT_STAGE_RAM.end {
        return (len, 0);
    }
    let below = BOOT_STAGE_RAM.start.saturating_sub(start);
    (below, len - below)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_array_below_the_range_is_whole() {
        assert_eq!(
            split_at_boot_stage_ram(0x4080_0000, 0x5_0000),
            (0x5_0000, 0)
        );
    }

    #[test]
    fn an_array_above_the_range_is_whole() {
        assert_eq!(split_at_boot_stage_ram(0x4086_0000, 0x1000), (0x1000, 0));
    }

    #[test]
    fn the_shipped_main_heap_is_cut_at_the_loader() {
        // The split image at main 17c6c34d6: HEAP_MAIN 0x40815970, 0x3EE78 B,
        // ending at 0x408547E8 — 18,408 B inside the loader's RAM.
        let (below, from) = split_at_boot_stage_ram(0x4081_5970, 0x3_EE78);
        assert_eq!(0x4081_5970 + below, BOOT_STAGE_RAM.start);
        assert_eq!(from, 0x4085_47E8 - 0x4085_0000);
    }

    #[test]
    fn an_array_starting_inside_the_range_is_all_fenced() {
        assert_eq!(split_at_boot_stage_ram(0x4085_8000, 0x1000), (0, 0x1000));
    }

    #[test]
    fn an_array_spanning_the_range_is_fenced_from_its_start_up() {
        let (below, from) = split_at_boot_stage_ram(0x4084_0000, 0x3_0000);
        assert_eq!((below, from), (0x1_0000, 0x2_0000));
    }

    #[test]
    fn the_range_is_the_loaders_linker_region() {
        let script = include_str!("../../../lp-fw/fw-esp32c6-loader/loader.x");
        let lram = script
            .lines()
            .find(|l| l.trim_start().starts_with("LRAM"))
            .expect("loader.x names LRAM");
        let expect = std::format!(
            "ORIGIN = 0x{:08X}, LENGTH = 0x{:X}",
            BOOT_STAGE_RAM.start,
            BOOT_STAGE_RAM.end - BOOT_STAGE_RAM.start
        );
        assert!(
            lram.to_ascii_uppercase()
                .contains(&expect.to_ascii_uppercase()),
            "loader.x's `{}` is not BOOT_STAGE_RAM ({expect})",
            lram.trim()
        );
    }
}
