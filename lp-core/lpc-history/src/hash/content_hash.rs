//! SHA-256 content hash with lowercase-hex text form.

use core::fmt;
use core::str::FromStr;

use alloc::string::String;
use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sha2::{Digest, Sha256};

const HEX: &[u8; 16] = b"0123456789abcdef";

/// SHA-256 hash of some content (a file's bytes, or a tree preimage).
///
/// Text form is 64 lowercase hex characters; [`ContentHash::short`] gives the
/// first 8 for UI and logs.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ContentHash([u8; 32]);

impl ContentHash {
    /// Hash the given bytes.
    pub fn of(data: &[u8]) -> Self {
        Self(Sha256::digest(data).into())
    }

    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// First 8 hex characters, for UI and logs.
    pub fn short(&self) -> String {
        let mut s = String::with_capacity(8);
        for byte in &self.0[..4] {
            s.push(HEX[(byte >> 4) as usize] as char);
            s.push(HEX[(byte & 0x0F) as usize] as char);
        }
        s
    }
}

impl fmt::Display for ContentHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in &self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

impl fmt::Debug for ContentHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ContentHash({self})")
    }
}

/// A content-hash string was not 64 lowercase hex characters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HashParseError;

impl fmt::Display for HashParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("content hash must be 64 lowercase hex characters")
    }
}

impl FromStr for ContentHash {
    type Err = HashParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.len() != 64 {
            return Err(HashParseError);
        }
        let mut bytes = [0u8; 32];
        for (slot, chunk) in bytes.iter_mut().zip(s.as_bytes().chunks_exact(2)) {
            let hi = hex_val(chunk[0]).ok_or(HashParseError)?;
            let lo = hex_val(chunk[1]).ok_or(HashParseError)?;
            *slot = (hi << 4) | lo;
        }
        Ok(Self(bytes))
    }
}

fn hex_val(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        _ => None,
    }
}

impl Serialize for ContentHash {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for ContentHash {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        s.parse().map_err(D::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;

    #[test]
    fn hex_round_trip() {
        let hash = ContentHash::of(b"hello");
        let hex = hash.to_string();
        assert_eq!(hex.len(), 64);
        let parsed: ContentHash = hex.parse().unwrap();
        assert_eq!(parsed, hash);
    }

    #[test]
    fn short_is_hex_prefix() {
        let hash = ContentHash::of(b"hello");
        assert_eq!(hash.short(), hash.to_string()[..8]);
    }

    #[test]
    fn rejects_bad_strings() {
        assert!("".parse::<ContentHash>().is_err());
        assert!("ab".parse::<ContentHash>().is_err());
        // uppercase is rejected: canonical form is lowercase only
        let upper = ContentHash::of(b"x").to_string().to_uppercase();
        assert!(upper.parse::<ContentHash>().is_err());
        let bad = "g".repeat(64);
        assert!(bad.parse::<ContentHash>().is_err());
    }

    #[test]
    fn serde_round_trip() {
        let hash = ContentHash::of(b"data");
        let json = serde_json::to_string(&hash).unwrap();
        assert_eq!(json, alloc::format!("\"{hash}\""));
        let back: ContentHash = serde_json::from_str(&json).unwrap();
        assert_eq!(back, hash);
    }
}
