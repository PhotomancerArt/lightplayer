//! Where a seam function's bytes live in flash, from the image alone.
//!
//! A ROM-up boot loads the app through the real bootloader, which maps the
//! app image's segments into the cache window. To arm a seam only once the
//! **app's** instruction is present at the seam's address (and never into a
//! page the bootloader mapped for something else), the emulator needs that
//! instruction's flash offset. It reads it from the flashed image itself:
//! the partition table at `0x8000` names the app partition that holds the
//! seam table, and the ESP image header at that partition's start lists each
//! segment's load address and where its bytes follow.
//!
//! ESP-IDF image format (the behaviour reference is esptool's output, a
//! fact; nothing here is copied from it): a 24-byte header (`0xE9` magic,
//! segment count at byte 1), then per segment a `load_addr: u32`, a
//! `data_len: u32` and the data.

/// The partition table's flash offset on every ESP32 of this generation.
pub const PARTITION_TABLE_OFFSET: usize = 0x8000;
const ENTRY_LEN: usize = 32;
const ENTRY_MAGIC: [u8; 2] = [0xAA, 0x50];
const IMAGE_MAGIC: u8 = 0xE9;
const IMAGE_HEADER_LEN: usize = 24;

/// The app partition (`type 0`) containing flash offset `inside`, as
/// `(offset, size)`.
pub fn app_partition_containing(flash: &[u8], inside: u32) -> Option<(u32, u32)> {
    let mut at = PARTITION_TABLE_OFFSET;
    while at + ENTRY_LEN <= flash.len() && at < PARTITION_TABLE_OFFSET + 0xC00 {
        let e = &flash[at..at + ENTRY_LEN];
        if e[0..2] != ENTRY_MAGIC {
            break;
        }
        let kind = e[2];
        let offset = u32::from_le_bytes(e[4..8].try_into().ok()?);
        let size = u32::from_le_bytes(e[8..12].try_into().ok()?);
        if kind == 0 && inside >= offset && inside < offset.saturating_add(size) {
            return Some((offset, size));
        }
        at += ENTRY_LEN;
    }
    None
}

/// The flash offset of guest address `vaddr`, read from the ESP image that
/// starts at `image_offset`. `None` when there is no image header there (a
/// direct load stages raw segments, not an image) or no segment covers it.
pub fn flash_offset_of(flash: &[u8], image_offset: u32, vaddr: u32) -> Option<u32> {
    let base = image_offset as usize;
    let header = flash.get(base..base + IMAGE_HEADER_LEN)?;
    if header[0] != IMAGE_MAGIC {
        return None;
    }
    let segments = header[1] as usize;
    let mut at = base + IMAGE_HEADER_LEN;
    for _ in 0..segments {
        let h = flash.get(at..at + 8)?;
        let load = u32::from_le_bytes(h[0..4].try_into().ok()?);
        let len = u32::from_le_bytes(h[4..8].try_into().ok()?);
        let data = (at + 8) as u32;
        if vaddr >= load && vaddr < load.saturating_add(len) {
            return Some(data + (vaddr - load));
        }
        at = at + 8 + len as usize;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_segment_is_found_through_the_partition_table_and_the_image_header() {
        let mut flash = vec![0xffu8; 0x30000];
        // factory app at 0x10000, 0x20000 long.
        let e = &mut flash[0x8000..0x8020];
        e[0..2].copy_from_slice(&ENTRY_MAGIC);
        e[2] = 0;
        e[3] = 0;
        e[4..8].copy_from_slice(&0x10000u32.to_le_bytes());
        e[8..12].copy_from_slice(&0x20000u32.to_le_bytes());
        // image: two segments.
        flash[0x10000] = IMAGE_MAGIC;
        flash[0x10001] = 2;
        let mut at = 0x10000 + IMAGE_HEADER_LEN;
        for (load, len) in [(0x4200_0020u32, 0x100u32), (0x4080_0000, 0x40)] {
            flash[at..at + 4].copy_from_slice(&load.to_le_bytes());
            flash[at + 4..at + 8].copy_from_slice(&len.to_le_bytes());
            at += 8 + len as usize;
        }
        assert_eq!(app_partition_containing(&flash, 0x12345), Some((0x10000, 0x20000)));
        assert_eq!(app_partition_containing(&flash, 0x5000), None);
        assert_eq!(
            flash_offset_of(&flash, 0x10000, 0x4200_0030),
            Some(0x10000 + 24 + 8 + 0x10)
        );
        assert_eq!(
            flash_offset_of(&flash, 0x10000, 0x4080_0004),
            Some(0x10000 + 24 + 8 + 0x100 + 8 + 4)
        );
        assert_eq!(flash_offset_of(&flash, 0x10000, 0x4300_0000), None);
    }
}
