//! The flag rule for `O.flags` and `R.flags` (`one-way-doors.md` §5.1).
//!
//! - The **low 4 bits** may be ignored by a reader that does not know them:
//!   a hint an old reader can safely miss.
//! - The **high 4 bits are must-understand**: a flag that changes what the
//!   message means. A board that sees one it does not know in an `O` refuses
//!   the offer `N`/`V` (`what` 4) before anything else; a host that sees one
//!   in an `R` does not serve that request and reports it.
//!
//! v1 defines one bit, [`REQUEST_TAKES_ENCODING_1`]. Every other bit is
//! reserved and written 0. **Forever:** a bit, once given a meaning, keeps it.

/// The must-understand half of a flags byte.
pub const MUST_UNDERSTAND: u8 = 0xF0;

/// `R.flags` bit 0: this board takes encoding 1 (`Z`) for this chunk. A host
/// may still answer `D` (no compressed form of that chunk).
pub const REQUEST_TAKES_ENCODING_1: u8 = 0x01;

/// The `O.flags` bits a v1 reader knows: none.
pub const OFFER_FLAGS_KNOWN_V1: u8 = 0x00;

/// The `R.flags` bits a v1 reader knows.
pub const REQUEST_FLAGS_KNOWN_V1: u8 = REQUEST_TAKES_ENCODING_1;

/// The must-understand bits of `flags` that are not in `known`: non-zero
/// means "refuse, I do not know what this message means".
#[must_use]
pub const fn unknown_must_understand(flags: u8, known: u8) -> u8 {
    flags & MUST_UNDERSTAND & !known
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn low_bits_are_ignorable_and_high_bits_are_not() {
        for low in 0..16u8 {
            assert_eq!(unknown_must_understand(low, OFFER_FLAGS_KNOWN_V1), 0);
        }
        for bit in 4..8 {
            let f = 1u8 << bit;
            assert_eq!(unknown_must_understand(f, OFFER_FLAGS_KNOWN_V1), f);
            assert_eq!(unknown_must_understand(f | 1, REQUEST_FLAGS_KNOWN_V1), f);
        }
        assert_eq!(
            unknown_must_understand(REQUEST_TAKES_ENCODING_1, REQUEST_FLAGS_KNOWN_V1),
            0
        );
    }
}
