//! Prefixed base-32 identifier, e.g. `prjhk7q9xy2mq4tb8wz`.
//!
//! The canonical text form is `<prefix><body>` — one token, no separator —
//! where `<prefix>` is a [`UidPrefix`] and `<body>` is exactly
//! [`UID_BODY_LEN`] characters of lowercase Crockford base-32 (`0-9a-z`
//! minus the confusables `i l o u`). The body is lowercase-only so a uid
//! survives case-folding contexts (URLs, case-insensitive filesystems,
//! being read aloud), and `_` stays out of the uid entirely so it remains
//! free as a delimiter in composite keys that EMBED a uid. No separator is
//! needed for exact parsing: prefixes are a closed set and the body length
//! is fixed, so the split point is always known.
//!
//! Minting takes 128 caller-supplied random bits and keeps the low
//! 16 × 5 = 80 bits — exactly uniform, since the keyspace is a power of
//! two. Values below 2^80 survive the reduction untouched, which the
//! deterministic efuse-MAC embed (`HardwareId::device_uid`) relies on. No
//! rng dependency exists in this crate.

use core::fmt;
use core::str::FromStr;

use alloc::string::String;
use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::uid_prefix::UidPrefix;

/// Length of the base-32 body of a [`PrefixedUid`].
pub const UID_BODY_LEN: usize = 16;

/// Lowercase Crockford base-32: `0-9` then `a-z` without `i l o u`.
const ALPHABET: &[u8; 32] = b"0123456789abcdefghjkmnpqrstvwxyz";

/// A prefixed base-32 identifier (`prj…`, `mod…`, `dev…`).
///
/// Compact (no heap per uid), ordered by prefix then body bytes.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PrefixedUid {
    prefix: UidPrefix,
    body: [u8; UID_BODY_LEN],
}

impl PrefixedUid {
    /// Mint a uid from caller-supplied random bytes.
    ///
    /// The caller owns randomness; this crate never generates any.
    pub fn mint(prefix: UidPrefix, random: &[u8; 16]) -> Self {
        let mut value = u128::from_be_bytes(*random);
        let mut body = [0u8; UID_BODY_LEN];
        for slot in body.iter_mut().rev() {
            *slot = ALPHABET[(value & 31) as usize];
            value >>= 5;
        }
        Self { prefix, body }
    }

    pub fn prefix(&self) -> UidPrefix {
        self.prefix
    }

    /// The 16-character base-32 body (always ASCII).
    pub fn body_str(&self) -> &str {
        core::str::from_utf8(&self.body).expect("uid body is always ASCII")
    }
}

impl fmt::Display for PrefixedUid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", self.prefix, self.body_str())
    }
}

impl fmt::Debug for PrefixedUid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PrefixedUid({self})")
    }
}

/// Why a uid string failed to parse.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UidParseError {
    /// The string does not start with a known [`UidPrefix`].
    UnknownPrefix,
    /// The body after the prefix is not exactly [`UID_BODY_LEN`] characters.
    BadLength,
    /// The body contains a character outside lowercase Crockford base-32.
    BadChar,
}

impl fmt::Display for UidParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let msg = match self {
            UidParseError::UnknownPrefix => "unknown uid prefix",
            UidParseError::BadLength => "uid body must be exactly 16 characters",
            UidParseError::BadChar => {
                "uid body must be lowercase Crockford base-32 (0-9a-z minus ilou)"
            }
        };
        f.write_str(msg)
    }
}

fn is_body_char(byte: u8) -> bool {
    ALPHABET.contains(&byte)
}

impl FromStr for PrefixedUid {
    type Err = UidParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let prefix = UidPrefix::ALL
            .into_iter()
            .find(|p| s.starts_with(p.as_str()))
            .ok_or(UidParseError::UnknownPrefix)?;
        let body = &s[prefix.as_str().len()..];
        if body.len() != UID_BODY_LEN {
            return Err(UidParseError::BadLength);
        }
        let mut bytes = [0u8; UID_BODY_LEN];
        for (slot, ch) in bytes.iter_mut().zip(body.bytes()) {
            if !is_body_char(ch) {
                return Err(UidParseError::BadChar);
            }
            *slot = ch;
        }
        Ok(Self {
            prefix,
            body: bytes,
        })
    }
}

impl Serialize for PrefixedUid {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for PrefixedUid {
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
    fn encodes_all_zero_bytes_as_zero_body() {
        let uid = PrefixedUid::mint(UidPrefix::Project, &[0u8; 16]);
        assert_eq!(uid.to_string(), "prj0000000000000000");
    }

    #[test]
    fn encodes_known_values() {
        // value 31 -> last digit 'z'
        let mut bytes = [0u8; 16];
        bytes[15] = 31;
        let uid = PrefixedUid::mint(UidPrefix::Device, &bytes);
        assert_eq!(uid.to_string(), "dev000000000000000z");

        // value 32 -> "10"
        bytes[15] = 32;
        let uid = PrefixedUid::mint(UidPrefix::Module, &bytes);
        assert_eq!(uid.to_string(), "mod0000000000000010");

        // max value: encoding must stay in-alphabet and length 16
        let uid = PrefixedUid::mint(UidPrefix::Project, &[0xFF; 16]);
        assert_eq!(uid.body_str().len(), UID_BODY_LEN);
        assert!(uid.body_str().bytes().all(is_body_char));
    }

    #[test]
    fn values_below_keyspace_survive_untouched() {
        // The efuse-MAC embed (HardwareId::device_uid) packs a value
        // < 2^56 and relies on mint being the identity below 2^80.
        let mut bytes = [0u8; 16];
        bytes[9] = 0x01;
        bytes[10..16].copy_from_slice(&[0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff]);
        let uid = PrefixedUid::mint(UidPrefix::Device, &bytes);
        // Round-tripping the body back through the alphabet recovers the value.
        let mut value: u128 = 0;
        for ch in uid.body_str().bytes() {
            let digit = ALPHABET.iter().position(|c| *c == ch).unwrap() as u128;
            value = (value << 5) | digit;
        }
        assert_eq!(value, u128::from_be_bytes(bytes));
    }

    #[test]
    fn round_trips_display_and_parse() {
        for prefix in UidPrefix::ALL {
            let uid = PrefixedUid::mint(prefix, &[0xA5; 16]);
            let parsed: PrefixedUid = uid.to_string().parse().unwrap();
            assert_eq!(parsed, uid);
        }
    }

    #[test]
    fn rejects_malformed_input() {
        // the retired underscore form: separator makes the body 17 chars
        assert_eq!(
            "prj_0000000000000000".parse::<PrefixedUid>(),
            Err(UidParseError::BadLength)
        );
        assert_eq!(
            "xxx0000000000000000".parse::<PrefixedUid>(),
            Err(UidParseError::UnknownPrefix)
        );
        assert_eq!(
            "prj000000000000000".parse::<PrefixedUid>(),
            Err(UidParseError::BadLength)
        );
        assert_eq!(
            "prj00000000000000000".parse::<PrefixedUid>(),
            Err(UidParseError::BadLength)
        );
        // 'i' is a confusable excluded from the alphabet
        assert_eq!(
            "prj000000000000000i".parse::<PrefixedUid>(),
            Err(UidParseError::BadChar)
        );
        // uppercase is out: the body is lowercase-only
        assert_eq!(
            "prj000000000000000Z".parse::<PrefixedUid>(),
            Err(UidParseError::BadChar)
        );
        assert_eq!("".parse::<PrefixedUid>(), Err(UidParseError::UnknownPrefix));
    }

    #[test]
    fn serde_round_trip_and_rejection() {
        let uid = PrefixedUid::mint(UidPrefix::Project, &[7u8; 16]);
        let json = serde_json::to_string(&uid).unwrap();
        assert_eq!(json, alloc::format!("\"{uid}\""));
        let back: PrefixedUid = serde_json::from_str(&json).unwrap();
        assert_eq!(back, uid);

        let bad: Result<PrefixedUid, _> = serde_json::from_str("\"prjshort\"");
        assert!(bad.is_err());
    }
}
