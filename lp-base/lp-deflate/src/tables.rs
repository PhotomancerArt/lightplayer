//! The fixed tables RFC 1951 pins into the spec itself: the length and
//! distance base/extra-bits tables (§3.2.5) that both block kinds share, and
//! the fixed Huffman code (§3.2.6) that a type-1 block uses without
//! transmitting any lengths at all.

use crate::huffman::Huff;

/// Length code `257+i` means "base `LEN_BASE[i]`, plus `LEN_EXTRA[i]` more
/// bits read verbatim" (RFC 1951 §3.2.5, the "Lit Value" 257..285 table).
pub(crate) const LEN_BASE: [u16; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131,
    163, 195, 227, 258,
];
pub(crate) const LEN_EXTRA: [u8; 29] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
];

/// Distance code `i` means "base `DIST_BASE[i]`, plus `DIST_EXTRA[i]` more
/// bits read verbatim" (RFC 1951 §3.2.5, the "Distance codes 0-29" table).
pub(crate) const DIST_BASE: [u16; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537,
    2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577,
];
pub(crate) const DIST_EXTRA: [u8; 30] = [
    0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13,
    13,
];

/// The fixed literal/length and distance codes a type-1 block uses (RFC 1951
/// §3.2.6): literals 0-143 at 8 bits, 144-255 at 9, length codes 256-279 at
/// 7, 280-287 at 8; every distance code at 5 bits.
pub(crate) fn fixed_tables() -> (Huff<288>, Huff<30>) {
    let mut l = [0u8; 288];
    l[..144].fill(8);
    l[144..256].fill(9);
    l[256..280].fill(7);
    l[280..].fill(8);
    let d = [5u8; 30];
    // Both tables are fixed, valid canonical codes by construction, so
    // `Huff::new` cannot fail here.
    (Huff::new(&l).unwrap(), Huff::new(&d).unwrap())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn length_and_distance_tables_are_the_same_length_as_their_extra_bits() {
        assert_eq!(LEN_BASE.len(), LEN_EXTRA.len());
        assert_eq!(DIST_BASE.len(), DIST_EXTRA.len());
    }

    #[test]
    fn longest_length_code_reaches_the_rfc_maximum_match() {
        // RFC 1951: the longest match is 258 bytes, at code 285 (LEN_BASE[28]),
        // with 0 extra bits.
        assert_eq!(LEN_BASE[28], 258);
        assert_eq!(LEN_EXTRA[28], 0);
    }

    #[test]
    fn longest_distance_code_reaches_the_32k_window() {
        // RFC 1951: the largest distance is 32768, at code 29, base 24577
        // plus 13 extra bits (up to 8191 more).
        assert_eq!(DIST_BASE[29] as u32 + (1u32 << DIST_EXTRA[29]) - 1, 32_768);
    }

    #[test]
    fn fixed_tables_build_without_error() {
        let _ = fixed_tables();
    }
}
