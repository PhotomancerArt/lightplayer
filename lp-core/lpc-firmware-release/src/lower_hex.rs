//! Lowercase hex: the one spelling of every hash and commit in this crate.

use alloc::string::String;
use sha2::{Digest, Sha256};

/// Length of a SHA-256 written as lowercase hex.
pub const SHA256_HEX_LEN: usize = 64;

/// Length of a full git commit written as lowercase hex.
pub const COMMIT_HEX_LEN: usize = 40;

/// How many leading commit digits a build id carries (N1, doors #1).
pub const BUILD_ID_COMMIT_DIGITS: usize = 12;

/// SHA-256 of `bytes`, as 64 lowercase hex digits.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(SHA256_HEX_LEN);
    for byte in digest {
        out.push(hex_digit(byte >> 4));
        out.push(hex_digit(byte & 0x0f));
    }
    out
}

/// True when `s` is exactly `len` lowercase hex digits (`[0-9a-f]`).
pub fn is_lower_hex(s: &str, len: usize) -> bool {
    s.len() == len && is_lower_hex_digits(s)
}

/// True when every byte of `s` is a lowercase hex digit (an empty `s` is too).
pub(crate) fn is_lower_hex_digits(s: &str) -> bool {
    s.bytes()
        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn hex_digit(nibble: u8) -> char {
    char::from(if nibble < 10 {
        b'0' + nibble
    } else {
        b'a' + nibble - 10
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_of_empty_is_the_known_digest() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn lower_hex_refuses_uppercase_and_wrong_length() {
        assert!(is_lower_hex("abc123", 6));
        assert!(!is_lower_hex("ABC123", 6));
        assert!(!is_lower_hex("abc12", 6));
        assert!(!is_lower_hex("abc12g", 6));
    }
}
