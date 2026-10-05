//! SHA-256 values as the JSON formats spell them: 64 lowercase hex digits.

use alloc::string::String;

/// `hash` as 64 lowercase hex digits.
#[must_use]
pub fn sha256_to_hex(hash: &[u8; 32]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(64);
    for &b in hash {
        s.push(DIGITS[(b >> 4) as usize] as char);
        s.push(DIGITS[(b & 0xf) as usize] as char);
    }
    s
}

/// 64 hex digits (either case) as a hash, or `None`.
#[must_use]
pub fn sha256_from_hex(text: &str) -> Option<[u8; 32]> {
    let bytes = text.as_bytes();
    if bytes.len() != 64 {
        return None;
    }
    let mut out = [0u8; 32];
    for (i, pair) in bytes.chunks_exact(2).enumerate() {
        out[i] = (nibble(pair[0])? << 4) | nibble(pair[1])?;
    }
    Some(out)
}

const fn nibble(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_refuses_bad_text() {
        let h: [u8; 32] = core::array::from_fn(|i| (i as u8).wrapping_mul(9));
        let s = sha256_to_hex(&h);
        assert_eq!(s.len(), 64);
        assert_eq!(s, s.to_lowercase());
        assert_eq!(sha256_from_hex(&s), Some(h));
        assert_eq!(sha256_from_hex(&s.to_uppercase()), Some(h));
        assert_eq!(sha256_from_hex(&s[..62]), None);
        assert_eq!(sha256_from_hex(&"g".repeat(64)), None);
    }
}
