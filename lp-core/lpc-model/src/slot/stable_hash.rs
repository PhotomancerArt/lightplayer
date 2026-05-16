//! Stable non-cryptographic hashes used by slot metadata.
//!
//! These functions are for compact, deterministic identifiers inside the slot
//! system. They are not security boundaries and should not be used for hash
//! tables with untrusted attacker-controlled keys.

/// 32-bit FNV-1a hash.
///
/// FNV-1a is the Fowler-Noll-Vo "FNV-1 alternate" hash. It starts from a fixed
/// offset basis, xors each input byte into the hash, then multiplies by a fixed
/// prime with wrapping arithmetic.
///
/// Useful properties:
///
/// - tiny implementation
/// - `const fn` friendly
/// - stable across targets and Rust versions
/// - good enough for compact ids over small, trusted name sets
///
/// Not useful for:
///
/// - cryptographic integrity
/// - adversarial collision resistance
/// - large global id spaces where a 32-bit collision would be unacceptable
pub(crate) const fn fnv1a_32(input: &str) -> u32 {
    const OFFSET_BASIS: u32 = 0x811c_9dc5;
    const PRIME: u32 = 0x0100_0193;

    let bytes = input.as_bytes();
    let mut hash = OFFSET_BASIS;
    let mut index = 0;
    while index < bytes.len() {
        hash ^= bytes[index] as u32;
        hash = hash.wrapping_mul(PRIME);
        index += 1;
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fnv1a_32_matches_known_vectors() {
        assert_eq!(fnv1a_32(""), 0x811c_9dc5);
        assert_eq!(fnv1a_32("a"), 0xe40c_292c);
        assert_eq!(fnv1a_32("hello"), 0x4f9f_2cab);
    }

    #[test]
    fn fnv1a_32_is_byte_order_stable() {
        assert_eq!(fnv1a_32("fixture.config"), 0x1dde_d212);
    }
}
