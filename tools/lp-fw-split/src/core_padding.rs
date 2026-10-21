//! Re-aiming espflash's address-0 padding segments at the core's `.bss`.
//!
//! espflash 3.3.0's image builder lays a flash segment's bytes at the same
//! offset within a 64 KiB page as its link address. When the core's rodata
//! crosses a 64 KiB page, flash `.text` moves to the next page (esp-hal's
//! `.text_gap`), and the gap in front of it is nearly a whole page. The
//! builder fills a gap with the image's RAM segments first and writes **an
//! address-0 segment of zeros** for what is left. The IDF bootloader skips
//! one; the C6 split loader copies it to address 0 and faults
//! (`docs/defects/2026-10-14-the-c6-loader-copies-an-address-zero-padding-segment-to-address-zero.md`).
//!
//! The loader is never updated over the air, so the image changes instead:
//! each padding segment's **load address** becomes a run of the core's
//! `.bss`. Its bytes stay where they are, so every later segment keeps its
//! offset and its page congruence. The loader then copies those zeros into
//! `.bss`, which is safe because:
//!
//! - **The core zeroes `.bss` before anything reads it.** riscv-rt's start
//!   (`_start`: copy `.data`, zero `__sbss..__ebss`, which esp-hal aliases to
//!   `_bss_start.._bss_end`) runs before Rust does, and the padding is zeros
//!   anyway.
//! - **Nothing is kept there across the copy.** `.bss` holds no state across
//!   a reset; what the core keeps (`.noinit`, `.rtc_fast.persistent`) is
//!   outside it, and the runs here never leave `.bss`.
//! - **It is not the loader's RAM.** `.bss` runs into the loader's
//!   (`0x4085_0000..`, the core's startup zeroes the loader too); a run that
//!   would reach it is refused, as is one that overlaps any other segment of
//!   the image, which the loader copies before or after this one.
//!
//! Since the core's `.text` follows its rodata (`engine_script.rs`,
//! `TEXT_FOLLOWS_RODATA`, 2026-10-15) there is no such gap in a core link,
//! and a core built from this tree carries no padding to aim. This stays as
//! the second line behind that one: whatever else ever makes espflash pad a
//! core, the loader copies the zeros into `.bss` rather than to address 0.
//!
//! The image's appended SHA-256 is recomputed (its checksum byte covers
//! segment data only, which does not change). Nothing at boot checks either
//! one; the image stays well-formed for every reader of it.

use std::ops::Range;

use anyhow::{Result, bail};
use sha2::{Digest, Sha256};

use crate::core_segments;
use crate::segment_rules::{ESP32C6_SEGMENTS, SegmentRules};

/// Header byte 23: 1 when a SHA-256 of the image is appended.
const APPEND_DIGEST_AT: usize = 23;
const DIGEST_LEN: usize = 32;

/// One padding segment that was given a load address.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AimedPadding {
    pub index: usize,
    pub len: u32,
    /// Its new load address, in `.bss`.
    pub addr: u32,
}

/// Give every address-0 segment of `image` a run of `bss`; the runs it chose.
/// An image with none is returned unchanged.
pub fn aim_padding_at_bss(image: &mut [u8], bss: Range<u32>) -> Result<Vec<AimedPadding>> {
    aim_padding_at_bss_in(image, bss, &ESP32C6_SEGMENTS)
}

/// [`aim_padding_at_bss`] for a chip whose loader's RAM `rules` names.
pub fn aim_padding_at_bss_in(
    image: &mut [u8],
    bss: Range<u32>,
    rules: &SegmentRules,
) -> Result<Vec<AimedPadding>> {
    let segs = core_segments::segments(image)?;
    let mut aimed = Vec::new();
    let mut next = bss.start.next_multiple_of(4);
    for s in segs.iter().filter(|s| s.addr == 0) {
        let data = &image[s.data_offset as usize..(s.data_offset + s.len) as usize];
        if data.iter().any(|b| *b != 0) {
            bail!(
                "segment {} is at address 0 but is not espflash's zero padding",
                s.index
            );
        }
        let run = next..next.saturating_add(s.len);
        if run.end > bss.end {
            bail!(
                "the core's .bss ({:#010x}..{:#010x}) has no room for {} B of padding at {:#010x}",
                bss.start,
                bss.end,
                s.len,
                run.start
            );
        }
        if rules
            .loader_ram
            .iter()
            .any(|l| run.start < l.end && run.end > l.start)
        {
            bail!(
                "padding at {:#010x}..{:#010x} would reach the loader's RAM",
                run.start,
                run.end
            );
        }
        if let Some(other) = segs
            .iter()
            .filter(|o| o.addr != 0)
            .find(|o| u64::from(run.start) < o.end() && u64::from(run.end) > u64::from(o.addr))
        {
            bail!(
                "padding at {:#010x}..{:#010x} would overlap segment {} ({:#010x}..{:#010x})",
                run.start,
                run.end,
                other.index,
                other.addr,
                other.end()
            );
        }
        let header = s.data_offset as usize - 8;
        image[header..header + 4].copy_from_slice(&run.start.to_le_bytes());
        aimed.push(AimedPadding {
            index: s.index,
            len: s.len,
            addr: run.start,
        });
        next = run.end.next_multiple_of(4);
    }
    if !aimed.is_empty() && image[APPEND_DIGEST_AT] == 1 {
        let body = image.len() - DIGEST_LEN;
        let digest: [u8; DIGEST_LEN] = Sha256::digest(&image[..body]).into();
        image[body..].copy_from_slice(&digest);
    }
    Ok(aimed)
}

#[cfg(test)]
mod tests {
    use super::*;

    const BSS: Range<u32> = 0x4080_9848..0x4085_1070;

    #[test]
    fn the_1110_with_main_padding_lands_at_the_start_of_bss() {
        let mut img = image(&[
            (0x4080_0000, 24),
            (0x4200_0040, 196_672),
            (0x4080_0018, 38_204),
            (0x4080_9558, 752),
            (0, 26_452),
            (0x4204_0020, 1_181_720),
        ]);
        let before = core_segments::segments(&img).unwrap();
        assert!(core_segments::check(&img).is_err());
        let aimed = aim_padding_at_bss(&mut img, BSS).unwrap();
        assert_eq!(
            aimed,
            [AimedPadding {
                index: 4,
                len: 26_452,
                addr: 0x4080_9848
            }]
        );
        let after = core_segments::check(&img).unwrap();
        // Every segment keeps its bytes' place; only one address moved.
        for (b, a) in before.iter().zip(&after) {
            assert_eq!((b.len, b.data_offset), (a.len, a.data_offset));
            assert_eq!(a.addr, if b.index == 4 { 0x4080_9848 } else { b.addr });
        }
        let body = img.len() - DIGEST_LEN;
        assert_eq!(&img[body..], Sha256::digest(&img[..body]).as_slice());
    }

    #[test]
    fn an_image_with_no_padding_is_untouched() {
        let mut img = image(&[(0x4080_0000, 24), (0x4200_0040, 64)]);
        let copy = img.clone();
        assert!(aim_padding_at_bss(&mut img, BSS).unwrap().is_empty());
        assert_eq!(img, copy);
    }

    #[test]
    fn two_paddings_take_consecutive_runs() {
        let mut img = image(&[(0, 100), (0x4080_0000, 24), (0, 7)]);
        let aimed = aim_padding_at_bss(&mut img, BSS).unwrap();
        assert_eq!(aimed[0].addr, BSS.start);
        assert_eq!(aimed[1].addr, BSS.start + 100);
    }

    #[test]
    fn a_run_that_does_not_fit_or_would_overlap_is_refused() {
        let mut img = image(&[(0, 0x1_0000)]);
        let small = 0x4080_9848..0x4080_9848 + 0x8000;
        assert!(aim_padding_at_bss(&mut img, small).is_err());
        let mut img = image(&[(0, 0x100)]);
        assert!(aim_padding_at_bss(&mut img, 0x4084_ff80..0x4086_0000).is_err());
        let mut img = image(&[(0x4080_9900, 16), (0, 0x100)]);
        assert!(aim_padding_at_bss(&mut img, BSS).is_err());
    }

    #[test]
    fn a_nonzero_address_0_segment_is_not_padding() {
        let mut img = image(&[(0, 16)]);
        img[24 + 8 + 3] = 1;
        assert!(aim_padding_at_bss(&mut img, BSS).is_err());
    }

    /// An ESP image of these `(addr, len)` segments (zero bytes), with an
    /// appended SHA-256 placeholder.
    fn image(segs: &[(u32, u32)]) -> Vec<u8> {
        let mut img = vec![0u8; 24];
        img[0] = 0xE9;
        img[1] = segs.len() as u8;
        img[APPEND_DIGEST_AT] = 1;
        for &(addr, len) in segs {
            img.extend_from_slice(&addr.to_le_bytes());
            img.extend_from_slice(&len.to_le_bytes());
            img.resize(img.len() + len as usize, 0);
        }
        img.resize(img.len() + 16, 0);
        img.resize(img.len() + DIGEST_LEN, 0xAA);
        img
    }
}
