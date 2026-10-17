//! The second-stage bootloader's identity word: what tells a reader of flash
//! `0x0` that LightPlayer's own bootloader is there, rather than the ESP-IDF
//! one.
//!
//! The C6 boots through one of two front doors (ADR
//! `docs/adr/2026-10-17-the-c6-boots-through-our-own-bootloader.md`):
//!
//! - **The IDF door**: ESP-IDF's second stage at `0x0`, which starts the
//!   loader at `factory+0` as its app (every board flashed before the
//!   cutover, and any board put back with `--release previous`);
//! - **Our door**: the loader's *bootloader flavour* at `0x0`, loaded by the
//!   mask ROM itself, doing the loader's job with no IDF in between.
//!
//! `app.bin` is byte-identical under both, so the factory loader's own word
//! ([`crate::loader_identity`], `LPLV`) does not say which door booted a
//! board. Ours carries eight more bytes, in its RAM segment's first bytes
//! (image offset `0x20`):
//!
//! ```text
//!  0  magic     u32  BOOTLOADER_ID_MAGIC ("LPLB")
//!  4  version   u16  BOOTLOADER_VERSION
//!  6  reserved  u16  0
//! ```
//!
//! A reader scans the first [`crate::loader_identity::LOADER_ID_SCAN_LEN`]
//! bytes of flash `0x0` for it ([`find_bootloader_version`]); 0 means the
//! IDF's (or none). Version 1 is the first: it applies loader version 1's
//! rules (the same `choose`, the same records) and leaves the core the entry
//! contract the ADR writes down.

use crate::loader_identity::{LOADER_ID_LEN, LOADER_ID_SCAN_LEN};

/// `"LPLB"`, little-endian.
pub const BOOTLOADER_ID_MAGIC: u32 = u32::from_le_bytes(*b"LPLB");
/// This tree's second-stage bootloader.
pub const BOOTLOADER_VERSION: u16 = 1;

/// The identity for `version`, as the bootloader carries it.
pub const fn bootloader_identity(version: u16) -> [u8; LOADER_ID_LEN] {
    let m = BOOTLOADER_ID_MAGIC.to_le_bytes();
    let v = version.to_le_bytes();
    [m[0], m[1], m[2], m[3], v[0], v[1], 0, 0]
}

/// The bootloader version in the first bytes of flash `0x0`: the first
/// 4-byte-aligned identity within the scan window, or 0 when there is none
/// (an IDF bootloader, or nothing).
pub fn find_bootloader_version(image: &[u8]) -> u16 {
    let scan = &image[..image.len().min(LOADER_ID_SCAN_LEN)];
    let magic = BOOTLOADER_ID_MAGIC.to_le_bytes();
    let mut at = 0;
    while at + LOADER_ID_LEN <= scan.len() {
        if scan[at..at + 4] == magic && scan[at + 6..at + 8] == [0, 0] {
            return u16::from_le_bytes([scan[at + 4], scan[at + 5]]);
        }
        at += 4;
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_word_is_found_at_the_image_offset_the_tool_puts_it() {
        let mut image = [0xffu8; 4096];
        image[0x20..0x28].copy_from_slice(&bootloader_identity(BOOTLOADER_VERSION));
        assert_eq!(find_bootloader_version(&image), 1);
    }

    #[test]
    fn the_loader_word_is_not_the_bootloader_word() {
        let mut image = [0xffu8; 4096];
        image[0x20..0x28].copy_from_slice(&crate::loader_identity::loader_identity(1));
        assert_eq!(find_bootloader_version(&image), 0);
        assert_eq!(find_bootloader_version(&[]), 0);
    }
}
