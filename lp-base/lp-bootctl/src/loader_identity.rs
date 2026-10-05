//! The loader's version word: what a core reads to learn which loader boots
//! it.
//!
//! The loader is the one piece of a split image never updated over the air,
//! so a core that installs an update has to know what the loader in front
//! of it can do. The loader carries eight bytes:
//!
//! ```text
//!  0  magic     u32  LOADER_ID_MAGIC ("LPLV")
//!  4  version   u16  LOADER_VERSION
//!  6  reserved  u16  0
//! ```
//!
//! **Where:** somewhere in the loader's ESP image, 4-byte aligned, within
//! its first [`LOADER_ID_SCAN_LEN`] bytes. A core finds it by scanning that
//! many bytes of flash from the app partition's start for the magic
//! ([`find_loader_version`]) rather than at a fixed offset: a RAM-only
//! loader's flash bytes are laid out by the image tool's segment order, and
//! scanning 4 KiB is trivial. (The loader keeps the word in its flash
//! placeholder segment, which today lands at image offset `0x20`.)
//!
//! **Meaning:** version *n* is "can read boot records with these marks and
//! applies these rules". Version 1 is the loader that first shipped: boot
//! record v1 with the `attempted`, `confirmed`, `started` and cold-tally
//! marks, `choose`'s warm and cold rules, the fallback to the other record.
//! **0** means "no word": the draft and spike loaders that preceded it.

/// `"LPLV"`, little-endian.
pub const LOADER_ID_MAGIC: u32 = u32::from_le_bytes(*b"LPLV");
/// This tree's loader.
pub const LOADER_VERSION: u16 = 1;
/// Bytes in the identity.
pub const LOADER_ID_LEN: usize = 8;
/// How much of the loader's image a reader scans for it.
pub const LOADER_ID_SCAN_LEN: usize = 4096;

/// The identity for `version`, as the loader carries it.
pub const fn loader_identity(version: u16) -> [u8; LOADER_ID_LEN] {
    let m = LOADER_ID_MAGIC.to_le_bytes();
    let v = version.to_le_bytes();
    [m[0], m[1], m[2], m[3], v[0], v[1], 0, 0]
}

/// The loader version in the first bytes of a loader image (or of flash
/// from the app partition's start): the first 4-byte-aligned identity
/// within [`LOADER_ID_SCAN_LEN`] bytes, or 0 when there is none.
pub fn find_loader_version(image: &[u8]) -> u16 {
    let scan = &image[..image.len().min(LOADER_ID_SCAN_LEN)];
    let magic = LOADER_ID_MAGIC.to_le_bytes();
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
    fn the_word_is_found_where_the_image_put_it() {
        let mut image = [0xffu8; 2336];
        image[0x20..0x28].copy_from_slice(&loader_identity(LOADER_VERSION));
        assert_eq!(find_loader_version(&image), 1);
    }

    #[test]
    fn no_word_is_version_zero() {
        assert_eq!(find_loader_version(&[0xff; 4096]), 0);
        assert_eq!(find_loader_version(&[]), 0);
        // Past the scan window it does not count.
        let mut image = [0u8; 5000];
        image[4096..4104].copy_from_slice(&loader_identity(1));
        assert_eq!(find_loader_version(&image), 0);
    }

    #[test]
    fn an_unaligned_or_unreserved_match_is_not_the_word() {
        let mut image = [0u8; 64];
        image[2..10].copy_from_slice(&loader_identity(1));
        assert_eq!(find_loader_version(&image), 0);
        let mut image = [0u8; 64];
        image[8..16].copy_from_slice(&loader_identity(1));
        image[14] = 1;
        assert_eq!(find_loader_version(&image), 0);
    }
}
