//! Tag bytes. The table in the crate docs is the reference; these are its names.
//!
//! A frame is read in two positions: **value position** (every value, and every
//! array element) and **key position** (inside an object, before each value).
//! The same byte means different things in the two, which is what lets a key
//! dictionary code and a small integer both fit in one byte.

// ---- value position -------------------------------------------------------

/// `00..=3F`: an unsigned integer 0..=63, the value is the byte.
pub const UINT_INLINE_MAX: u8 = 0x3F;
/// `40..=7F`: value-dictionary string 0..=63.
pub const VALUE_DICT_INLINE_BASE: u8 = 0x40;
/// How many value-dictionary entries have a one-byte code.
pub const VALUE_DICT_INLINE_COUNT: usize = 64;
/// `80..=9F`: an inline string of 0..=31 bytes; the UTF-8 follows.
pub const STR_INLINE_BASE: u8 = 0x80;
/// Longest string whose length rides in the tag byte.
pub const STR_INLINE_MAX_LEN: usize = 31;
/// Object start: key-position bytes follow until [`OBJECT_END`].
pub const OBJECT: u8 = 0xA0;
/// Array start: values follow until [`ARRAY_END`].
pub const ARRAY: u8 = 0xA1;
/// Array end.
pub const ARRAY_END: u8 = 0xA2;
/// `null`.
pub const NULL: u8 = 0xA3;
/// `false`.
pub const FALSE: u8 = 0xA4;
/// `true`.
pub const TRUE: u8 = 0xA5;
/// Unsigned integer, LEB128 magnitude follows.
pub const UINT: u8 = 0xA6;
/// Negative integer, LEB128 magnitude follows.
pub const NEG_INT: u8 = 0xA7;
/// Positive decimal: zigzag-LEB128 exponent, then LEB128 coefficient.
pub const DECIMAL_POS: u8 = 0xA8;
/// Negative decimal (including `-0`), laid out as [`DECIMAL_POS`].
pub const DECIMAL_NEG: u8 = 0xA9;
/// String: LEB128 length, then UTF-8.
pub const STRING: u8 = 0xAA;
/// Blob: LEB128 length, then raw bytes. Decodes to canonical padded base64.
pub const BLOB: u8 = 0xAB;
/// Value-dictionary string 64 and up: LEB128 (index − 64).
pub const VALUE_DICT: u8 = 0xAC;
/// Back-reference: LEB128 n, the n-th inline text or blob of this frame,
/// printed as a string.
pub const BACKREF: u8 = 0xAD;
/// Number text escape: LEB128 length, then the number's ASCII verbatim.
pub const NUMBER_TEXT: u8 = 0xAE;
/// Blob back-reference: LEB128 n, the n-th inline text or blob of this frame,
/// printed as base64.
pub const BLOB_BACKREF: u8 = 0xAF;

// ---- key position ---------------------------------------------------------

/// `00..=EF`: key-dictionary 0..=239.
pub const KEY_DICT_INLINE_COUNT: usize = 0xF0;
/// `F0..=FB` + one byte: key-dictionary 240 + ((tag − F0) << 8 | byte).
pub const KEY_DICT_WIDE_BASE: u8 = 0xF0;
/// Last wide key-dictionary tag.
pub const KEY_DICT_WIDE_MAX: u8 = 0xFB;
/// Most keys a dictionary can give a code (240 one-byte + 12 × 256 two-byte).
pub const KEY_DICT_MAX: usize =
    KEY_DICT_INLINE_COUNT + ((KEY_DICT_WIDE_MAX - KEY_DICT_WIDE_BASE) as usize + 1) * 256;
/// Inline key: LEB128 length, then UTF-8.
pub const KEY_INLINE: u8 = 0xFC;
/// Key back-reference: LEB128 n, the n-th inline text or blob of this frame.
pub const KEY_BACKREF: u8 = 0xFD;
/// Object end.
pub const OBJECT_END: u8 = 0xFF;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_dictionary_reaches_3312_entries() {
        assert_eq!(KEY_DICT_MAX, 3312);
    }

    #[test]
    fn value_ranges_do_not_overlap() {
        assert_eq!(UINT_INLINE_MAX + 1, VALUE_DICT_INLINE_BASE);
        assert_eq!(
            VALUE_DICT_INLINE_BASE as usize + VALUE_DICT_INLINE_COUNT,
            STR_INLINE_BASE as usize
        );
        assert_eq!(
            STR_INLINE_BASE as usize + STR_INLINE_MAX_LEN + 1,
            OBJECT as usize
        );
    }
}
