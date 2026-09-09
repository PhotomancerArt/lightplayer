//! Package-time guard: the merged image's second-stage bootloader loads where
//! the device model says it does.
//!
//! Studio recognizes a board hung in its bootloader by the ROM's `Saved PC`
//! falling inside the bootloader's code segments
//! (`lpa_devices::bootloader::bootloader_code_ranges`). That table is
//! hardcoded from the bootloader espflash bundles; the day the bundled
//! bootloader changes (an espflash upgrade, or a `--bootloader` override in
//! the build def), the table would silently stop matching and the detection
//! would go blind. The merged image starts with the bootloader's own ESP
//! image header, so the truth is one parse away — this refuses to package an
//! image whose bootloader segments disagree with the table.
//!
//! See `docs/defects/2026-09-06-c6-first-flash-bootloader-hang-lp-analog-i2c-clock.md`.

use std::ops::Range;

use anyhow::{Result, bail};

/// ESP image header: magic, segment count, flash mode, flash size/freq,
/// entry (8 bytes) + the extended header (16 bytes).
const ESP_IMAGE_MAGIC: u8 = 0xE9;
const ESP_IMAGE_HEADER_LEN: usize = 24;

/// One `load:` segment of an ESP image: `[addr, addr + len)`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ImageSegment {
    pub addr: u32,
    pub len: u32,
}

impl ImageSegment {
    fn range(&self) -> Range<u32> {
        self.addr..self.addr + self.len
    }
}

/// Parse the segment table of the ESP image at the start of `bytes`.
pub(super) fn parse_image_segments(bytes: &[u8]) -> Result<Vec<ImageSegment>> {
    if bytes.len() < ESP_IMAGE_HEADER_LEN || bytes[0] != ESP_IMAGE_MAGIC {
        bail!(
            "not an ESP image: expected magic 0x{ESP_IMAGE_MAGIC:02x} and a {ESP_IMAGE_HEADER_LEN}-byte header"
        );
    }
    let count = bytes[1] as usize;
    let mut offset = ESP_IMAGE_HEADER_LEN;
    let mut segments = Vec::with_capacity(count);
    for index in 0..count {
        let Some(header) = bytes.get(offset..offset + 8) else {
            bail!("segment {index} header runs past the end of the image");
        };
        let addr = u32::from_le_bytes([header[0], header[1], header[2], header[3]]);
        let len = u32::from_le_bytes([header[4], header[5], header[6], header[7]]);
        offset += 8;
        if bytes.len() < offset + len as usize {
            bail!("segment {index} (0x{addr:08x}, {len} bytes) runs past the end of the image");
        }
        offset += len as usize;
        segments.push(ImageSegment { addr, len });
    }
    Ok(segments)
}

/// Where the ESP32-C6 app's RAM ends (`_stack_start`); the ROM loads the
/// bootloader above it. Segments at or above this are bootloader code or
/// data, and every code one must be in the model's table.
const C6_APP_RAM_END: u32 = 0x4086_e610;

/// Refuse when the bootloader at the head of `image` does not load its code
/// where `lpa_devices::bootloader::bootloader_code_ranges(chip)` says.
///
/// Two-way check: every table range must be a segment of the image, and
/// every image segment in the bootloader's part of HP SRAM must be in the
/// table — except the first (lowest) segment, which is the bootloader's DRAM
/// data and never executes. Chips without a table are not checked.
pub(super) fn check_bootloader_segments(chip: &str, image: &[u8]) -> Result<()> {
    let expected = lpa_devices::bootloader::bootloader_code_ranges(chip);
    if expected.is_empty() {
        return Ok(());
    }
    let segments = parse_image_segments(image)?;
    let ranges: Vec<Range<u32>> = segments.iter().map(ImageSegment::range).collect();

    let missing: Vec<&Range<u32>> = expected
        .iter()
        .filter(|range| !ranges.contains(range))
        .collect();
    let data_segment = segments.iter().map(|segment| segment.addr).min();
    let unexpected: Vec<&Range<u32>> = ranges
        .iter()
        .filter(|range| range.start >= C6_APP_RAM_END)
        .filter(|range| Some(range.start) != data_segment)
        .filter(|range| !expected.contains(range))
        .collect();

    if missing.is_empty() && unexpected.is_empty() {
        return Ok(());
    }
    let fmt = |ranges: &[&Range<u32>]| {
        ranges
            .iter()
            .map(|range| format!("0x{:08x}..0x{:08x}", range.start, range.end))
            .collect::<Vec<_>>()
            .join(", ")
    };
    bail!(
        "the merged image's bootloader does not load where `lpa_devices::bootloader::bootloader_code_ranges(\"{chip}\")` \
         says it does — Studio's hung-bootloader detection would go blind. Table ranges missing from the image: [{}]; \
         image segments above 0x{C6_APP_RAM_END:08x} missing from the table: [{}]. Update the table (and the bench \
         script's copy in scripts/c6-lp-ana-i2c.py) from the new bootloader's header.",
        fmt(&missing),
        fmt(&unexpected),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build an ESP image with the given segments (contents zero).
    fn image_with(segments: &[(u32, u32)]) -> Vec<u8> {
        let mut bytes = vec![0u8; ESP_IMAGE_HEADER_LEN];
        bytes[0] = ESP_IMAGE_MAGIC;
        bytes[1] = segments.len() as u8;
        for (addr, len) in segments {
            bytes.extend_from_slice(&addr.to_le_bytes());
            bytes.extend_from_slice(&len.to_le_bytes());
            bytes.extend(std::iter::repeat_n(0u8, *len as usize));
        }
        bytes
    }

    /// espflash 3.3.0's esp32c6-bootloader.bin, from its header.
    const SHIPPED_C6: &[(u32, u32)] = &[
        (0x4086_c410, 0xd48),
        (0x4086_e610, 0x2d68),
        (0x4087_5720, 0x1800),
    ];

    #[test]
    fn parses_the_segment_table() {
        let segments = parse_image_segments(&image_with(SHIPPED_C6)).unwrap();
        assert_eq!(
            segments,
            vec![
                ImageSegment {
                    addr: 0x4086_c410,
                    len: 0xd48
                },
                ImageSegment {
                    addr: 0x4086_e610,
                    len: 0x2d68
                },
                ImageSegment {
                    addr: 0x4087_5720,
                    len: 0x1800
                },
            ]
        );
    }

    #[test]
    fn refuses_a_non_image() {
        assert!(parse_image_segments(&[0xFF; 64]).is_err());
        assert!(parse_image_segments(&[0xE9]).is_err());
    }

    #[test]
    fn refuses_a_truncated_segment() {
        let mut bytes = image_with(SHIPPED_C6);
        bytes.truncate(bytes.len() - 16);
        assert!(parse_image_segments(&bytes).is_err());
    }

    #[test]
    fn the_shipped_c6_bootloader_matches_the_table() {
        check_bootloader_segments("esp32c6", &image_with(SHIPPED_C6)).unwrap();
    }

    #[test]
    fn a_moved_code_segment_is_refused() {
        let moved = &[
            (0x4086_c410, 0xd48),
            (0x4086_e620, 0x2d68), // shifted by 16 bytes
            (0x4087_5720, 0x1800),
        ];
        let error = check_bootloader_segments("esp32c6", &image_with(moved)).unwrap_err();
        let message = error.to_string();
        assert!(message.contains("0x4086e610..0x40871378"), "{message}");
        assert!(message.contains("0x4086e620..0x40871388"), "{message}");
    }

    #[test]
    fn a_missing_code_segment_is_refused() {
        let fewer = &[(0x4086_c410, 0xd48), (0x4086_e610, 0x2d68)];
        assert!(check_bootloader_segments("esp32c6", &image_with(fewer)).is_err());
    }

    #[test]
    fn chips_without_a_table_are_not_checked() {
        check_bootloader_segments("esp32", &[0xFF; 8]).unwrap();
    }
}
