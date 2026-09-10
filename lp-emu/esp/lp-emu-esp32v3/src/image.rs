//! What is in a flashed chip: the `esptool` image header, the segment table,
//! and the partition table.
//!
//! Nothing here is executed and nothing here loads anything. The ROM-up boot
//! runs the **real** mask ROM and the **real** second-stage bootloader
//! against these bytes, so an image parser in the emulator would be a second
//! opinion competing with the one under test. This exists for the other
//! direction: to say, independently of the boot, what the boot *should* have
//! printed.
//!
//! The two log lines a classic boot spends on this are
//!
//! ```text
//! load:0x3fff0030,len:7104                       ← the ROM, per bootloader segment
//! I (73) esp_image: segment 0: paddr=00010020 vaddr=3f400020 size=00100h (   256) map
//! ```
//!
//! and both are derived from the image, not from the machine: the ROM prints
//! one per segment of the image at [`crate::flash::BOOTLOADER_OFFSET`], the
//! bootloader one per segment of the image in the `factory` partition. A
//! gate that compared them against a transcript recorded from a *different*
//! build would be gating on the linker; a gate that compares them against
//! the image the machine was handed is gating on the boot. This module is
//! the second one's half.
//!
//! # The header, as the ROM reads it
//!
//! `esp_image_header_t` is 24 bytes: an 8-byte common header (magic, segment
//! count, SPI mode, a nibble each of SPI speed and flash size, entry point)
//! and a 16-byte extended header (WP pin, three pin-drive bytes, chip id,
//! minimum revisions, and the `hash_appended` flag). Then `segment_count`
//! segments, each an 8-byte `{load address, length}` and its bytes. The
//! image ends with a one-byte checksum at the next 16-byte boundary and —
//! when `hash_appended` — a 32-byte SHA-256 after it.
//!
//! `SPIWP:0xee` in L0's boot log is the WP-pin byte of the **bootloader's**
//! extended header, printed by the ROM; `mode:DIO, clock div:2` is its SPI
//! mode and speed nibble. So the first lines after the reset banner are this
//! struct read aloud.
//!
//! ⚠️ **The classic's chip id is 0.** The C6's is 13. A parser that checked
//! for the C6's would reject every classic image.

use std::fmt;

/// The magic byte every `esptool` image starts with.
pub const IMAGE_MAGIC: u8 = 0xe9;

/// The partition table's magic, little-endian `0x50AA` — the `0xaa50` a hex
/// dump of the first two bytes shows.
pub const PARTITION_MAGIC: u16 = 0x50AA;

/// `ESP_CHIP_ID_ESP32` — the classic is chip id 0.
pub const CHIP_ID_ESP32: u16 = 0;

/// One `{load address, length}` pair and where its bytes are.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImageSegment {
    /// Where the segment's bytes are in the flash chip — the `paddr` of the
    /// bootloader's `esp_image:` line, which points **past** the 8-byte
    /// segment header at the data itself.
    pub paddr: u32,
    /// Where they go: the `vaddr`.
    pub vaddr: u32,
    pub len: u32,
}

impl ImageSegment {
    /// Is this segment served through a flash cache window rather than
    /// copied into RAM? The bootloader's line ends `map` for these and
    /// `load` for the rest.
    ///
    /// The classic has **two** such windows, DROM and IROM, which is why the
    /// shipped image has two `map` DROM segments and the bootloader prints
    /// `E boot: Image contains multiple DROM segments…` on every boot.
    pub fn is_mapped(&self) -> bool {
        crate::loader::flash_window_of(self.vaddr).is_some()
    }

    /// `"drom"`, `"irom"` or `None` — which of the two windows.
    pub fn window(&self) -> Option<&'static str> {
        crate::loader::flash_window_of(self.vaddr).map(|(name, _)| name)
    }
}

/// An `esptool` image's header and segment table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EspImage {
    /// Where in the chip this image starts.
    pub offset: u32,
    pub entry: u32,
    /// The SPI mode nibble: 0 QIO, 1 QOUT, 2 DIO, 3 DOUT.
    pub spi_mode: u8,
    /// The low nibble of byte 3 — the SPI speed code.
    pub spi_speed: u8,
    /// The high nibble of byte 3 — the flash size code (2 = 4 MiB).
    pub flash_size: u8,
    /// The WP pin byte of the extended header, which the ROM prints as
    /// `SPIWP:0x..`.
    pub wp_pin: u8,
    pub chip_id: u16,
    pub hash_appended: bool,
    pub segments: Vec<ImageSegment>,
    /// The first offset past the image: the byte after the checksum, or
    /// after the appended hash when there is one.
    pub end: u32,
}

impl EspImage {
    /// The `mode:` word the ROM prints.
    pub fn spi_mode_name(&self) -> &'static str {
        match self.spi_mode {
            0 => "QIO",
            1 => "QOUT",
            2 => "DIO",
            3 => "DOUT",
            4 => "FAST_READ",
            5 => "SLOW_READ",
            _ => "?",
        }
    }

    /// The clock divider the ROM prints for the speed nibble. The ROM
    /// divides an 80 MHz source, so `40MHz` is `clock div:2` — which is
    /// exactly what L0's banner says.
    pub fn clock_div(&self) -> u32 {
        match self.spi_speed {
            0 => 2,   // 40 MHz
            1 => 1,   // 80 MHz
            2 => 4,   // 20 MHz
            0xf => 1, // 80 MHz (the "fast" encoding)
            _ => 0,
        }
    }

    /// The flash size in bytes the size nibble names.
    pub fn flash_size_bytes(&self) -> u32 {
        match self.flash_size {
            0 => 1 << 20,
            1 => 2 << 20,
            2 => 4 << 20,
            3 => 8 << 20,
            4 => 16 << 20,
            5 => 32 << 20,
            _ => 0,
        }
    }

    /// How many of the segments are mapped through a flash window. **More
    /// than one DROM segment is what the bootloader complains about**, and
    /// the shipped `fw-esp32v3` image has two.
    pub fn drom_segments(&self) -> usize {
        self.segments
            .iter()
            .filter(|s| s.window() == Some("drom"))
            .count()
    }
}

/// Why a byte range is not an image.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImageError {
    /// The first byte is not `0xe9`. On a chip this means "nothing was
    /// flashed here", which is exactly what the ROM concludes.
    NotAnImage { offset: u32, magic: u8 },
    /// A header or a segment reaches past the end of the chip.
    Truncated { offset: u32, needed: u32, have: u32 },
    /// A segment claims a length no chip could hold — a corrupt header read
    /// as a length is how a parser hangs, so it is refused by size.
    AbsurdSegment { index: usize, len: u32 },
}

impl fmt::Display for ImageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ImageError::NotAnImage { offset, magic } => write!(
                f,
                "no image at flash offset {offset:#010x}: the first byte is {magic:#04x}, \
                 not {IMAGE_MAGIC:#04x}"
            ),
            ImageError::Truncated {
                offset,
                needed,
                have,
            } => write!(
                f,
                "the image at {offset:#010x} needs {needed} bytes and the chip has {have}"
            ),
            ImageError::AbsurdSegment { index, len } => write!(
                f,
                "segment {index} claims {len} bytes; that is not a segment, it is a bad header"
            ),
        }
    }
}

impl std::error::Error for ImageError {}

/// No single segment of a real image is anywhere near 32 MiB — the largest
/// part this chip family addresses. A length past it is a misread header.
const MAX_SEGMENT_LEN: u32 = 32 << 20;

/// Parse the image at `offset` in `chip`.
pub fn parse(chip: &[u8], offset: u32) -> Result<EspImage, ImageError> {
    let have = chip.len() as u32;
    let at = |p: u32, n: u32| -> Result<&[u8], ImageError> {
        let end = p.checked_add(n).ok_or(ImageError::Truncated {
            offset,
            needed: u32::MAX,
            have,
        })?;
        if end > have {
            return Err(ImageError::Truncated {
                offset,
                needed: end,
                have,
            });
        }
        Ok(&chip[p as usize..end as usize])
    };

    let header = at(offset, 24)?;
    if header[0] != IMAGE_MAGIC {
        return Err(ImageError::NotAnImage {
            offset,
            magic: header[0],
        });
    }
    let segment_count = header[1] as usize;
    let spi_mode = header[2];
    let spi_speed = header[3] & 0x0f;
    let flash_size = header[3] >> 4;
    let entry = u32::from_le_bytes([header[4], header[5], header[6], header[7]]);
    let wp_pin = header[8];
    let chip_id = u16::from_le_bytes([header[12], header[13]]);
    let hash_appended = header[23] == 1;

    let mut segments = Vec::with_capacity(segment_count);
    let mut p = offset + 24;
    for index in 0..segment_count {
        let head = at(p, 8)?;
        let vaddr = u32::from_le_bytes([head[0], head[1], head[2], head[3]]);
        let len = u32::from_le_bytes([head[4], head[5], head[6], head[7]]);
        if len > MAX_SEGMENT_LEN {
            return Err(ImageError::AbsurdSegment { index, len });
        }
        let paddr = p + 8;
        at(paddr, len)?;
        segments.push(ImageSegment { paddr, vaddr, len });
        p = paddr + len;
    }

    // The image is padded so the checksum is the **last** byte of a 16-byte
    // block: the first position `p` or later that is 15 mod 16.
    let checksum_at = p + (15 - (p % 16));
    let mut end = checksum_at + 1;
    if hash_appended {
        end += 32;
    }
    at(end.saturating_sub(1), 1)?;

    Ok(EspImage {
        offset,
        entry,
        spi_mode,
        spi_speed,
        flash_size,
        wp_pin,
        chip_id,
        hash_appended,
        segments,
        end,
    })
}

/// One row of the partition table, as the bootloader prints it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Partition {
    pub label: String,
    pub kind: u8,
    pub subtype: u8,
    pub offset: u32,
    pub len: u32,
}

impl Partition {
    /// `app` (type 0) or `data` (type 1)?
    pub fn is_app(&self) -> bool {
        self.kind == 0
    }
}

/// Read the partition table at [`crate::flash::PARTITION_TABLE_OFFSET`].
///
/// Each entry is 32 bytes: the `0x50AA` magic, type, subtype, offset,
/// length, a 16-byte label and 4 flag bytes. The table ends at the first
/// entry whose magic is not `0x50AA` — an erased chip reads `0xffff`, which
/// is how the bootloader knows it is done.
pub fn partitions(chip: &[u8]) -> Vec<Partition> {
    let mut out = Vec::new();
    let mut p = crate::flash::PARTITION_TABLE_OFFSET as usize;
    while p + 32 <= chip.len() {
        let entry = &chip[p..p + 32];
        if u16::from_le_bytes([entry[0], entry[1]]) != PARTITION_MAGIC {
            break;
        }
        let label_bytes = &entry[12..28];
        let label_end = label_bytes
            .iter()
            .position(|b| *b == 0)
            .unwrap_or(label_bytes.len());
        out.push(Partition {
            kind: entry[2],
            subtype: entry[3],
            offset: u32::from_le_bytes([entry[4], entry[5], entry[6], entry[7]]),
            len: u32::from_le_bytes([entry[8], entry[9], entry[10], entry[11]]),
            label: String::from_utf8_lossy(&label_bytes[..label_end]).into_owned(),
        });
        p += 32;
    }
    out
}

/// The whole chip, read the way a boot reads it: the bootloader at `0x1000`,
/// the partition table at `0x8000`, and the app in the first `app`
/// partition.
#[derive(Clone, Debug)]
pub struct MergedImage {
    pub bootloader: EspImage,
    pub partitions: Vec<Partition>,
    /// The app image, and the partition it was found in. `None` when the
    /// table has no app partition or nothing was flashed into it — a chip a
    /// boot would stop on, which is a scenario and not an error.
    pub app: Option<(Partition, EspImage)>,
}

impl MergedImage {
    /// Parse a whole chip. The bootloader is required: a merged image
    /// without one is not one.
    pub fn parse(chip: &[u8]) -> Result<Self, ImageError> {
        let bootloader = parse(chip, crate::flash::BOOTLOADER_OFFSET)?;
        let partitions = partitions(chip);
        let app = partitions
            .iter()
            .find(|p| p.is_app())
            .and_then(|p| parse(chip, p.offset).ok().map(|image| (p.clone(), image)));
        Ok(Self {
            bootloader,
            partitions,
            app,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flash::{BOOTLOADER_OFFSET, FACTORY_OFFSET};

    #[test]
    fn an_erased_chip_holds_no_image_and_says_so_by_its_magic() {
        let chip = vec![0xffu8; 0x20000];
        assert_eq!(
            parse(&chip, BOOTLOADER_OFFSET),
            Err(ImageError::NotAnImage {
                offset: BOOTLOADER_OFFSET,
                magic: 0xff
            })
        );
        assert!(partitions(&chip).is_empty());
    }

    /// A hand-built image with the shipped one's shape — two DROM segments,
    /// a DRAM load and an IROM map — at the classic's own offsets.
    fn synthetic(offset: u32) -> Vec<u8> {
        let mut chip = vec![0xffu8; 0x20000];
        let mut header = vec![
            IMAGE_MAGIC,
            4,    // segments
            2,    // DIO
            0x20, // 4 MiB, 40 MHz
        ];
        header.extend_from_slice(&0x4008_0844u32.to_le_bytes());
        header.extend_from_slice(&[0xee, 0, 0, 0]); // wp + drive
        header.extend_from_slice(&CHIP_ID_ESP32.to_le_bytes());
        header.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 0, 0]);
        header.push(1); // hash appended
        assert_eq!(header.len(), 24);
        let at = offset as usize;
        chip[at..at + 24].copy_from_slice(&header);

        let mut p = at + 24;
        for (vaddr, len) in [
            (0x3F40_0020u32, 0x100u32),
            (0x3FFB_0000, 0x10),
            (0x3F40_0140, 0x40),
            (0x400D_0020, 0x80),
        ] {
            chip[p..p + 4].copy_from_slice(&vaddr.to_le_bytes());
            chip[p + 4..p + 8].copy_from_slice(&len.to_le_bytes());
            p += 8 + len as usize;
        }
        chip
    }

    #[test]
    fn the_header_and_the_segment_table_read_the_way_the_rom_prints_them() {
        let image = parse(&synthetic(BOOTLOADER_OFFSET), BOOTLOADER_OFFSET).unwrap();
        assert_eq!(image.entry, 0x4008_0844);
        assert_eq!(image.spi_mode_name(), "DIO", "L0's `mode:DIO`");
        assert_eq!(image.clock_div(), 2, "L0's `clock div:2`");
        assert_eq!(image.flash_size_bytes(), 4 << 20);
        assert_eq!(image.wp_pin, 0xee, "L0's `SPIWP:0xee`");
        assert_eq!(image.chip_id, CHIP_ID_ESP32);
        assert!(image.hash_appended);
        assert_eq!(image.segments.len(), 4);
    }

    #[test]
    fn the_two_flash_windows_are_both_map_and_dram_is_load() {
        let image = parse(&synthetic(FACTORY_OFFSET), FACTORY_OFFSET).unwrap();
        let kinds: Vec<Option<&'static str>> = image.segments.iter().map(|s| s.window()).collect();
        assert_eq!(
            kinds,
            vec![Some("drom"), None, Some("drom"), Some("irom")],
            "DRAM is loaded; both flash windows are mapped"
        );
        assert!(!image.segments[1].is_mapped());
        // The line the desk board prints on every boot is about this count.
        assert_eq!(image.drom_segments(), 2);
    }

    #[test]
    fn a_length_no_chip_could_hold_is_a_bad_header_not_a_long_read() {
        let mut chip = synthetic(BOOTLOADER_OFFSET);
        let at = (BOOTLOADER_OFFSET + 24 + 4) as usize;
        chip[at..at + 4].copy_from_slice(&0x7fff_ffffu32.to_le_bytes());
        assert_eq!(
            parse(&chip, BOOTLOADER_OFFSET),
            Err(ImageError::AbsurdSegment {
                index: 0,
                len: 0x7fff_ffff
            })
        );
    }

    #[test]
    fn a_segment_reaching_past_the_chip_is_truncated_not_a_panic() {
        let mut chip = synthetic(BOOTLOADER_OFFSET);
        chip.truncate(BOOTLOADER_OFFSET as usize + 40);
        assert!(matches!(
            parse(&chip, BOOTLOADER_OFFSET),
            Err(ImageError::Truncated { .. })
        ));
    }
}
