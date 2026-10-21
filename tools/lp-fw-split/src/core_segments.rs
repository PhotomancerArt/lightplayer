//! The core image's segments, held to what the loader can place.
//!
//! `fw-esp32c6-loader` (`src/esp_image.rs`) walks the core's ESP image and
//! does one of two things with each segment: a segment whose load address is
//! inside the flash MMU window is **mapped**, and every other segment is
//! **copied** to its load address, refused only when it overlaps the loader's
//! own RAM. It never asks whether that address is memory. espflash 3.3.0's
//! image builder writes an **address-0 padding segment** when a flash segment
//! needs a gap before it and there are too few RAM bytes left to fill it; the
//! loader copied one to address 0 on silicon, and the store faulted before
//! the core could mark its trial `attempted`
//! (`docs/defects/2026-10-14-the-c6-loader-copies-an-address-zero-padding-segment-to-address-zero.md`).
//!
//! The loader is never updated over the air, so the image is what has to
//! obey: [`check`] refuses a core with **any** segment the loader cannot
//! place, not only one at address 0. A segment is placeable when all of it
//! lies in one of two ranges:
//!
//! - **[`CORE_FLASH`]**, mapped. The loader maps anything in its MMU window
//!   (`fw-esp32c6-loader/src/mmu.rs`, `WINDOW = 0x4200_0000..0x4300_0000`),
//!   but only the core's own part of it is the core's: the engine is mapped
//!   from [`ENGINE_BASE`] by the core later, and the loader reads the core
//!   through its scratch window at `0x4260_0000`
//!   (`fw-esp32c6-loader/src/flash_window.rs`, `SCRATCH`), which a core
//!   segment there would remap under it. The segment's bytes must also sit at
//!   its address's offset within a page, or the loader refuses it ("segment
//!   not page-congruent"); [`crate::app_image::PAGE`] is the page the image is laid out for.
//! - **[`CORE_RAM`] minus [`LOADER_RAM`]**, copied. `CORE_RAM` is the HP SRAM
//!   the core's own link may use (esp-hal's `ld/esp32c6/memory.x`, `RAM :
//!   ORIGIN = 0x40800000, LENGTH = 0x6E610`); above it is the IDF
//!   bootloader's and the ROM's RAM, which the loader still calls into
//!   (`ets_printf`, the cache invalidate) after the copy. `LOADER_RAM` is the
//!   loader's own code, data and stack (`fw-esp32c6-loader/src/main.rs`,
//!   `LOADER_RAM = 0x4085_0000..0x4086_0000`; `loader.x`, `LRAM`), which it
//!   refuses to overwrite, so a core landing there does not boot either.
//!
//! LP SRAM (`0x5000_0000..`) is left out on purpose: no core has carried a
//! segment there, and one would be copied over the RTC memory the recovery
//! ledger keeps across resets. A core that needs one is a decision, not a
//! build that passes by default.
//!
//! The constants are re-typed from the loader's source, which this crate
//! cannot depend on (it is a bare-metal binary in its own workspace); a test
//! reads that source and fails when the two drift.

use std::ops::Range;

use anyhow::{Result, bail};

use crate::engine_script::ENGINE_BASE;
use crate::segment_rules::{ESP32C6_SEGMENTS, SegmentRules};

/// The flash the core's own segments may be mapped at: the start of the
/// loader's MMU window up to the engine region.
pub const CORE_FLASH: Range<u32> = 0x4200_0000..ENGINE_BASE;
/// The loader's MMU window (`fw-esp32c6-loader/src/mmu.rs`, `WINDOW`): a
/// segment in it is mapped, one outside it copied.
pub const LOADER_MMU_WINDOW: Range<u32> = 0x4200_0000..0x4300_0000;
/// The HP SRAM the core's link may place a segment in (esp-hal
/// `ld/esp32c6/memory.x`, `RAM`).
pub const CORE_RAM: Range<u32> = 0x4080_0000..0x4086_E610;
/// The loader's own RAM (`fw-esp32c6-loader/src/main.rs`, `LOADER_RAM`).
pub const LOADER_RAM: Range<u32> = 0x4085_0000..0x4086_0000;

/// The ESP image's header: magic, segment count at byte 1, entry at 4..8.
const IMAGE_MAGIC: u8 = 0xE9;
const IMAGE_HEADER_LEN: usize = 24;
const SEGMENT_HEADER_LEN: usize = 8;

/// One segment of an ESP application image.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CoreSegment {
    /// Its index in the image.
    pub index: usize,
    /// Load address.
    pub addr: u32,
    pub len: u32,
    /// Where its bytes start in the image.
    pub data_offset: u32,
}

impl CoreSegment {
    /// `addr + len`, without overflowing.
    pub fn end(&self) -> u64 {
        u64::from(self.addr) + u64::from(self.len)
    }

    /// How the C6's loader treats it, or why it cannot place it.
    pub fn placement(&self) -> Result<Placement, String> {
        self.placement_in(&ESP32C6_SEGMENTS)
    }

    /// How a chip's loader treats it, or why it cannot place it.
    pub fn placement_in(&self, rules: &SegmentRules) -> Result<Placement, String> {
        let within =
            |r: &Range<u32>| self.addr >= r.start && self.end() <= u64::from(r.end) && self.len > 0;
        let overlaps = |r: &Range<u32>| {
            u64::from(self.addr) < u64::from(r.end) && self.end() > u64::from(r.start)
        };
        if let Some(w) = rules.flash.iter().find(|w| w.window.contains(&self.addr)) {
            if !within(&w.core) {
                return Err(format!(
                    "mapped by the loader, but not inside the core's flash {:#010x}..{:#010x}",
                    w.core.start, w.core.end
                ));
            }
            let page = rules.page;
            if self.data_offset % page != self.addr % page {
                return Err(format!(
                    "its bytes at image offset {:#x} are not at its address's offset in a {page:#x} page",
                    self.data_offset
                ));
            }
            return Ok(Placement::Mapped);
        }
        if !rules.ram.iter().any(within) {
            let what = if self.addr == 0 {
                "an address-0 segment (espflash's padding)"
            } else {
                "a segment outside the core's RAM"
            };
            let ranges: Vec<String> = rules
                .ram
                .iter()
                .map(|r| format!("{:#010x}..{:#010x}", r.start, r.end))
                .collect();
            return Err(format!(
                "{what}: the loader would copy it to {:#010x}, which is not inside {}",
                self.addr,
                ranges.join(" or ")
            ));
        }
        if let Some(l) = rules.loader_ram.iter().find(|l| overlaps(l)) {
            return Err(format!(
                "copied over the loader's own RAM {:#010x}..{:#010x}, which the loader refuses",
                l.start, l.end
            ));
        }
        Ok(Placement::Copied)
    }

    /// One line of the C6's segment table.
    pub fn line(&self) -> String {
        self.line_in(&ESP32C6_SEGMENTS)
    }

    /// One line of a chip's segment table.
    pub fn line_in(&self, rules: &SegmentRules) -> String {
        let how = match self.placement_in(rules) {
            Ok(Placement::Mapped) => "mapped".to_string(),
            Ok(Placement::Copied) => "copied".to_string(),
            Err(why) => format!("REFUSED: {why}"),
        };
        format!(
            "  {:2}: {:#010x}..{:#010x} {:8} B @{:#08x}  {how}",
            self.index,
            self.addr,
            self.end(),
            self.len,
            self.data_offset
        )
    }
}

/// What the loader does with a segment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Placement {
    /// Mapped through the MMU at its address.
    Mapped,
    /// Copied into RAM at its address.
    Copied,
}

/// An ESP application image's segments, in image order.
pub fn segments(image: &[u8]) -> Result<Vec<CoreSegment>> {
    if image.len() < IMAGE_HEADER_LEN || image[0] != IMAGE_MAGIC {
        bail!("not an ESP application image (no 0xE9 header)");
    }
    let count = usize::from(image[1]);
    let mut out = Vec::with_capacity(count);
    let mut off = IMAGE_HEADER_LEN;
    for index in 0..count {
        let Some(header) = image.get(off..off + SEGMENT_HEADER_LEN) else {
            bail!("segment {index}'s header lies past the image's end");
        };
        let addr = u32::from_le_bytes(header[0..4].try_into().unwrap());
        let len = u32::from_le_bytes(header[4..8].try_into().unwrap());
        let data = off + SEGMENT_HEADER_LEN;
        if data + len as usize > image.len() {
            bail!("segment {index} ({len} B at {addr:#010x}) runs past the image's end");
        }
        out.push(CoreSegment {
            index,
            addr,
            len,
            data_offset: data as u32,
        });
        off = data + len as usize;
    }
    Ok(out)
}

/// The core image's segment table, or an error naming every segment the
/// loader cannot place.
pub fn check(image: &[u8]) -> Result<Vec<CoreSegment>> {
    check_in(image, &ESP32C6_SEGMENTS)
}

/// [`check`] against a chip's loader.
pub fn check_in(image: &[u8], rules: &SegmentRules) -> Result<Vec<CoreSegment>> {
    let segs = segments(image)?;
    let refused: Vec<_> = segs
        .iter()
        .filter(|s| s.placement_in(rules).is_err())
        .collect();
    if !refused.is_empty() {
        let table: Vec<_> = segs.iter().map(|s| s.line_in(rules)).collect();
        bail!(
            "the core image has {} segment(s) the loader cannot place; a board would not boot it \
             (docs/defects/2026-10-14-the-c6-loader-copies-an-address-zero-padding-segment-to-address-zero.md):\n{}",
            refused.len(),
            table.join("\n")
        );
    }
    Ok(segs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_image::PAGE;

    #[test]
    fn release_2026_10_14_10s_core_passes() {
        // The segment table of release 2026.10.14-10's core (`f7fc2c781`).
        let img = image(&[
            (0x4080_0000, 24),
            (0x4200_0040, 196_312),
            (0x4080_0018, 248),
            (0x4203_0020, 1_182_000),
            (0x4080_0110, 20_892),
            (0x4080_52b0, 22_884),
            (0x4080_ac18, 752),
        ]);
        let segs = check(&img).unwrap();
        assert_eq!(segs.len(), 7);
        assert_eq!(segs[1].placement(), Ok(Placement::Mapped));
        assert_eq!(segs[4].placement(), Ok(Placement::Copied));
    }

    #[test]
    fn the_address_0_padding_of_1110_with_main_is_refused() {
        // #1110 merged with main (`5fb7d260b`): rodata crossed 0x42030000.
        let img = image(&[
            (0x4080_0000, 24),
            (0x4200_0040, 196_672),
            (0x4080_0018, 38_204),
            (0x4080_9558, 752),
            (0, 26_452),
            (0x4204_0020, 1_181_720),
        ]);
        let err = check(&img).unwrap_err().to_string();
        assert!(err.contains("1 segment(s)"), "{err}");
        assert!(err.contains("address-0 segment"), "{err}");
        assert!(err.contains("REFUSED"), "{err}");
    }

    #[test]
    fn the_class_is_refused_not_only_address_0() {
        let refused = |addr: u32, len: u32| {
            let seg = CoreSegment {
                index: 0,
                addr,
                len,
                data_offset: addr % PAGE,
            };
            seg.placement().is_err()
        };
        assert!(refused(0, 16), "address 0");
        assert!(refused(0x10, 16), "low memory");
        assert!(refused(0x4000_0000, 16), "the mask ROM");
        assert!(refused(0x407f_fff0, 32), "starts below the SRAM");
        assert!(refused(0x4086_e600, 32), "runs into the bootloader's RAM");
        assert!(
            refused(0x4087_0000, 16),
            "the bootloader's and the ROM's RAM"
        );
        assert!(refused(0x4084_fff0, 32), "runs into the loader");
        assert!(refused(0x4085_8000, 16), "inside the loader");
        assert!(refused(0x5000_0000, 16), "LP SRAM");
        assert!(refused(0x6000_0000, 16), "peripherals");
        assert!(refused(ENGINE_BASE, 16), "the engine region");
        assert!(refused(0x4260_0000, 16), "the loader's scratch window");
        assert!(refused(0x423f_fff0, 32), "runs into the engine region");
        assert!(refused(0x4080_0000, 0), "an empty segment");
        assert!(!refused(0x4080_0000, 16), "SRAM's start");
        assert!(!refused(0x4084_fff0, 16), "up to the loader");
        assert!(
            !refused(0x4086_0000, 0x4086_e610 - 0x4086_0000),
            "after the loader"
        );
        assert!(!refused(0x4200_0040, 16), "core flash");
    }

    #[test]
    fn a_flash_segment_off_its_page_offset_is_refused() {
        let seg = CoreSegment {
            index: 1,
            addr: 0x4203_0020,
            len: 64,
            data_offset: 0x3_0040,
        };
        assert!(seg.placement().unwrap_err().contains("page"));
    }

    #[test]
    fn a_truncated_image_is_an_error() {
        let mut img = image(&[(0x4080_0000, 24)]);
        img.truncate(30);
        assert!(segments(&img).is_err());
        assert!(segments(&[0u8; 32]).is_err());
    }

    /// The loader's source is where these ranges come from; fail when it and
    /// this file disagree.
    #[test]
    fn the_ranges_are_the_loaders() {
        let loader = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../lp-fw/fw-esp32c6-loader/src"
        );
        let read = |f: &str| std::fs::read_to_string(format!("{loader}/{f}")).unwrap();
        assert!(
            read("main.rs")
                .contains("const LOADER_RAM: core::ops::Range<u32> = 0x4085_0000..0x4086_0000;"),
            "fw-esp32c6-loader's LOADER_RAM moved: update LOADER_RAM here"
        );
        assert!(
            read("mmu.rs")
                .contains("pub const WINDOW: core::ops::Range<u32> = 0x4200_0000..0x4300_0000;"),
            "fw-esp32c6-loader's MMU WINDOW moved: update LOADER_MMU_WINDOW here"
        );
        assert!(
            read("flash_window.rs").contains("const SCRATCH: u32 = 0x4260_0000;"),
            "fw-esp32c6-loader's scratch window moved: CORE_FLASH must stay below it"
        );
        assert!(read("esp_image.rs").contains("if mmu::WINDOW.contains(&addr) {"));
        let memory_x = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../third_party/esp-hal/ld/esp32c6/memory.x"
        ))
        .unwrap();
        assert!(
            memory_x.contains("RAM : ORIGIN = 0x40800000 , LENGTH = 0x6E610"),
            "esp-hal's C6 RAM region moved: update CORE_RAM here"
        );
        assert!(CORE_FLASH.end <= 0x4260_0000);
    }

    /// An ESP image with these `(addr, len)` segments, each one's bytes right
    /// after its header, as espflash writes them. Given a real image's table,
    /// every flash segment lands at its real (congruent) offset.
    fn image(segs: &[(u32, u32)]) -> Vec<u8> {
        let mut img = vec![0u8; IMAGE_HEADER_LEN];
        img[0] = IMAGE_MAGIC;
        img[1] = segs.len() as u8;
        for &(addr, len) in segs {
            if LOADER_MMU_WINDOW.contains(&addr) {
                let data = (img.len() + SEGMENT_HEADER_LEN) as u32;
                assert_eq!(
                    data % PAGE,
                    addr % PAGE,
                    "test image: {addr:#x} at {data:#x}"
                );
            }
            img.extend_from_slice(&addr.to_le_bytes());
            img.extend_from_slice(&len.to_le_bytes());
            img.resize(img.len() + len as usize, 0);
        }
        img
    }
}
