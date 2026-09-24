//! Byte-slice equality whose running time does not depend on WHERE the
//! slices differ.
//!
//! A login answer is compared against the expected MAC; an early-exit `==`
//! would leak, through timing, how many leading bytes a guess got right.
//! Over BLE the jitter dwarfs a byte compare, so this is hygiene rather than
//! a measured defence — but it costs nothing and it is the rule.

/// `true` when `a` and `b` are equal. Lengths are not secret (every MAC is
/// 32 bytes), so a length mismatch returns early.
#[must_use]
pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut difference = 0u8;
    for (left, right) in a.iter().zip(b.iter()) {
        difference |= left ^ right;
    }
    // Keep the optimizer from turning the fold back into an early exit.
    core::hint::black_box(difference) == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equal_and_unequal_slices() {
        assert!(constant_time_eq(b"", b""));
        assert!(constant_time_eq(&[7u8; 32], &[7u8; 32]));
        assert!(!constant_time_eq(&[7u8; 32], &[8u8; 32]));

        let mut last_differs = [0u8; 32];
        last_differs[31] = 1;
        assert!(!constant_time_eq(&[0u8; 32], &last_differs));
    }

    #[test]
    fn length_mismatch_is_unequal() {
        assert!(!constant_time_eq(b"abc", b"ab"));
        assert!(!constant_time_eq(b"", b"a"));
    }
}
