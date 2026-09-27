//! 8-bit sequence numbers, compared modulo 256. Windows stay at or below 127
//! frames, so "behind" and "ahead" are never ambiguous.

/// How far `to` is ahead of `from`, modulo 256.
pub fn seq_dist(from: u8, to: u8) -> u8 {
    to.wrapping_sub(from)
}

/// `seq` lies behind `expected` (a duplicate), not ahead of it.
pub fn seq_is_behind(expected: u8, seq: u8) -> bool {
    seq_dist(expected, seq) >= 128
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wraps() {
        assert_eq!(seq_dist(250, 3), 9);
        assert!(seq_is_behind(3, 250));
        assert!(!seq_is_behind(250, 3));
    }
}
