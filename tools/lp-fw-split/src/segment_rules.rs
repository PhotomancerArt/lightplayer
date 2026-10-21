//! What a chip's loader can place of a core image, as data
//! ([`crate::core_segments`] applies it).
//!
//! A loader maps a segment whose load address is in one of its MMU windows
//! and copies every other one to its address. So a core segment is
//! placeable when it lies wholly in the core's part of a mapped window (at
//! its address's offset within a page), or wholly in the core's RAM less the
//! loader's own. The C6's numbers are [`crate::core_segments`]'s constants,
//! unchanged; the S3's come from its memory map and layout 2.

use std::ops::Range;

use crate::chip_profile::layout_2;
use crate::core_segments::{CORE_FLASH, CORE_RAM, LOADER_MMU_WINDOW, LOADER_RAM};

/// One flash window the loader maps segments through.
#[derive(Debug)]
pub struct FlashWindow {
    /// Every address in it is mapped, not copied.
    pub window: Range<u32>,
    /// The part of it that is the core's (below the engine, below the
    /// loader's scratch).
    pub core: Range<u32>,
}

/// The ranges one chip's loader places a core's segments in.
#[derive(Debug)]
pub struct SegmentRules {
    pub flash: &'static [FlashWindow],
    /// RAM the core's link may place a copied segment in.
    pub ram: &'static [Range<u32>],
    /// The loader's own RAM, in every view: it refuses to copy over it.
    pub loader_ram: &'static [Range<u32>],
    /// The MMU page a mapped segment must be congruent to.
    pub page: u32,
}

/// The C6 (`fw-esp32c6-loader`; the ranges and their sources are
/// [`crate::core_segments`]'s).
pub static ESP32C6_SEGMENTS: SegmentRules = SegmentRules {
    flash: &[FlashWindow {
        window: LOADER_MMU_WINDOW,
        core: CORE_FLASH,
    }],
    ram: &[CORE_RAM],
    loader_ram: &[LOADER_RAM],
    page: crate::app_image::PAGE,
};

/// The S3 (`fw-esp32s3-loader`). Both buses are mapped through one MMU
/// table; the core's part of each is everything below the engine's view.
/// RAM is esp-hal's `dram_seg` (`ld/esp32s3/memory.x`: `0x3FC8_8000` to
/// `dram2_seg`'s `0x3FCD_B700`, where the IDF bootloader's own RAM begins)
/// and its instruction-bus alias (`+0x6F_0000`; the core's vectors and
/// `.rwtext` start at `0x4037_8000`). The loader lives at data view
/// `0x3FCB_0000..0x3FCC_0000` (code at the alias), inside every core's
/// `.bss`.
pub static ESP32S3_SEGMENTS: SegmentRules = SegmentRules {
    flash: &[
        FlashWindow {
            window: 0x3C00_0000..0x3E00_0000,
            core: 0x3C00_0000..layout_2::ENGINE_DROM,
        },
        FlashWindow {
            window: 0x4200_0000..0x4400_0000,
            core: 0x4200_0000..layout_2::ENGINE_IROM,
        },
    ],
    ram: &[S3_DRAM, S3_IRAM],
    loader_ram: &[
        layout_2::LOADER_DRAM,
        (layout_2::LOADER_DRAM.start + S3_IRAM_ALIAS)..(layout_2::LOADER_DRAM.end + S3_IRAM_ALIAS),
    ],
    page: layout_2::LAYOUT_2_PAGE,
};

/// esp-hal's S3 `dram_seg`.
const S3_DRAM: Range<u32> = 0x3FC8_8000..0x3FCD_B700;
/// Internal SRAM1's instruction-bus view is its data-bus view plus this.
const S3_IRAM_ALIAS: u32 = 0x6F_0000;
/// `dram_seg` seen from the instruction bus.
const S3_IRAM: Range<u32> = (S3_DRAM.start + S3_IRAM_ALIAS)..(S3_DRAM.end + S3_IRAM_ALIAS);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_s3_loaders_ram_is_inside_the_cores_and_its_alias_too() {
        let r = &ESP32S3_SEGMENTS;
        assert_eq!(r.loader_ram[1], 0x403A_0000..0x403B_0000);
        assert!(r.ram[0].start <= r.loader_ram[0].start && r.loader_ram[0].end <= r.ram[0].end);
        assert!(r.ram[1].start <= r.loader_ram[1].start && r.loader_ram[1].end <= r.ram[1].end);
        assert_eq!(r.ram[1].start, 0x4037_8000, "the core's vectors");
    }

    /// Where the S3 numbers come from: esp-hal's memory map.
    #[test]
    fn the_s3_ranges_are_esp_hals() {
        let memory_x = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../third_party/esp-hal/ld/esp32s3/memory.x"
        ))
        .unwrap();
        assert!(
            memory_x.contains("dram2_seg ( RW )       : ORIGIN = 0x3FCDB700"),
            "esp-hal's S3 dram2_seg moved: update S3_DRAM here"
        );
        assert!(
            memory_x.contains("dram_seg ( RW )        : ORIGIN = 0x3FC88000"),
            "esp-hal's S3 dram_seg moved: update S3_DRAM here"
        );
    }
}
