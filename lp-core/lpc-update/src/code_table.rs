//! **The one code table**: every integer code of protocol v1, defined here
//! and nowhere else (`one-way-doors.md` §5.5, §3). The offer (`O`), the board
//! manifest and the firmware-distribution plan's `ota-manifest.json`
//! (`requires`, `encodings[].id`) use these integers; nothing re-types them.
//!
//! **Forever.** Change only by adding: a code, once given a meaning, keeps
//! it; a reader that meets a code it does not know treats it as "not this
//! board's" (an offer is refused `N`/`V`, a host reads the board as needing
//! USB).

/// Protocol v1. `proto` in `Q`, `O` and `M` is information, not a
/// negotiation: a host speaks the board's version and never sends a newer
/// message to an older board.
pub const PROTO_V1: u8 = 1;

/// Bytes in one chunk: every piece is moved in chunks of this many raw
/// bytes, the last one short. It is also the NOR sector size.
pub const CHUNK: u32 = 4096;

/// The chip code of the ESP32-C6, in the binary offer only. Everywhere else
/// (the board manifest, `ota-manifest.json`, the build defs) a chip is its
/// word, [`chip_name`].
pub const CHIP_ESP32C6: u16 = 1;

/// Layout 1: the split image's offsets inside the `factory` partition (the
/// split image, M2):
///
/// | `factory` + | Contents |
/// |---|---|
/// | `0x0000` | the loader, at most [`LAYOUT_1_LOADER_MAX_LEN`] bytes |
/// | `0x5000` | the update-progress record ([`LAYOUT_1_PROGRESS_OFFSET`]) |
/// | `0x6000`, `0x7000` | boot records 0 and 1 |
/// | `0x8000` → end | the core, then the engine; the end is the partition table's |
///
/// An install needs the offer's layout **equal** to the board's.
pub const LAYOUT_1: u16 = 1;

/// Layout 1's room for the loader: the split image's
/// [`lp_bootctl::LOADER_MAX_LEN`] (`0x5000`), referenced, never copied.
pub const LAYOUT_1_LOADER_MAX_LEN: u32 = lp_bootctl::LOADER_MAX_LEN;

/// Layout 1's progress-record sector, from `factory`'s start: the split
/// image's [`lp_bootctl::PROGRESS_RECORD_SECTOR`] less its
/// [`lp_bootctl::LOADER_OFFSET`] (`0x1_5000 - 0x1_0000`).
pub const LAYOUT_1_PROGRESS_OFFSET: u32 =
    lp_bootctl::PROGRESS_RECORD_SECTOR - lp_bootctl::LOADER_OFFSET;

/// Loader version 0: the loader carries no version word (only the draft and
/// spike loaders that preceded the split image).
pub const LOADER_NONE: u16 = 0;

/// Loader version 1: the split image's loader, the first with the word. The
/// word itself (`"LPLV"`, `u16` version, `u16` reserved, found by scanning
/// the loader's first 4 KiB for the magic) is the split image's, defined
/// beside the loader; this table only names the values. An install needs the
/// board's loader **≥** the offer's `min_loader`. Version *n* means "can read
/// boot records with these marks".
pub const LOADER_1: u16 = 1;

/// Encoding 1: raw deflate under the dictionary rule
/// ([`crate::dictionary_rule`]). It is what request flag bit 0 asks for, and
/// the `id` that `ota-manifest.json`'s `encodings[]` lists the `.z` files
/// under.
pub const ENCODING_1: u8 = 1;

/// Every chip this table names: `(code, word)`.
const CHIPS: &[(u16, &str)] = &[(CHIP_ESP32C6, "esp32c6")];

/// The offer's chip code for a chip word, if this table knows it.
#[must_use]
pub fn chip_code(word: &str) -> Option<u16> {
    CHIPS.iter().find(|(_, w)| *w == word).map(|(c, _)| *c)
}

/// The chip word for an offer's chip code, if this table knows it. The word
/// is the same one the build defs, the image's manifest core and
/// `ota-manifest.json` use.
#[must_use]
pub fn chip_name(code: u16) -> Option<&'static str> {
    CHIPS.iter().find(|(c, _)| *c == code).map(|(_, w)| *w)
}

/// Whether this table knows `layout`.
#[must_use]
pub const fn layout_known(layout: u16) -> bool {
    layout == LAYOUT_1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_codes_are_pinned() {
        assert_eq!(PROTO_V1, 1);
        assert_eq!(CHUNK, 4096);
        assert_eq!(CHIP_ESP32C6, 1);
        assert_eq!(LAYOUT_1, 1);
        assert_eq!((LOADER_NONE, LOADER_1), (0, 1));
        assert_eq!(ENCODING_1, 1);
        assert_eq!(LAYOUT_1_LOADER_MAX_LEN, 0x5000);
        assert_eq!(LAYOUT_1_PROGRESS_OFFSET, 0x5000);
    }

    #[test]
    fn the_progress_sector_is_where_the_loader_ends() {
        // One-way-doors §8: the loader may never grow into the progress
        // sector, or erasing the record would erase loader code.
        assert!(LAYOUT_1_LOADER_MAX_LEN <= LAYOUT_1_PROGRESS_OFFSET);
    }

    #[test]
    fn chips_map_both_ways() {
        assert_eq!(chip_code("esp32c6"), Some(1));
        assert_eq!(chip_name(1), Some("esp32c6"));
        assert_eq!(chip_code("esp32"), None);
        assert_eq!(chip_name(0), None);
        assert!(layout_known(1));
        assert!(!layout_known(0) && !layout_known(2));
    }
}
