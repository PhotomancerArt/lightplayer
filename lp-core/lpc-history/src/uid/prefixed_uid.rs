//! Prefixed base-62 identifier, e.g. `prj_h7Kq9xY2mQ4tB8Wz`.
//!
//! The canonical text form is `<prefix>_<body>` where `<prefix>` is a
//! [`UidPrefix`] and `<body>` is exactly [`UID_BODY_LEN`] characters from
//! the base-62 alphabet `0-9A-Za-z`.
//!
//! Minting takes 128 caller-supplied random bits and keeps the 16
//! least-significant base-62 digits — i.e. the value modulo 62^16
//! (~95 bits of keyspace). The slight non-uniformity from the modulo is
//! irrelevant at this keyspace and is accepted by design; no rng dependency
//! exists in this crate.

use core::fmt;
use core::str::FromStr;

use alloc::string::String;
use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::uid_prefix::UidPrefix;

/// Length of the base-62 body of a [`PrefixedUid`].
pub const UID_BODY_LEN: usize = 16;

const ALPHABET: &[u8; 62] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";

/// A prefixed base-62 identifier (`prj_…`, `mod_…`, `dev_…`).
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
            *slot = ALPHABET[(value % 62) as usize];
            value /= 62;
        }
        Self { prefix, body }
    }

    pub fn prefix(&self) -> UidPrefix {
        self.prefix
    }

    /// The 16-character base-62 body (always ASCII).
    pub fn body_str(&self) -> &str {
        core::str::from_utf8(&self.body).expect("uid body is always ASCII")
    }
}

impl fmt::Display for PrefixedUid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}_{}", self.prefix, self.body_str())
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
    /// The prefix is not one of the known [`UidPrefix`] values.
    UnknownPrefix,
    /// No `_` separator between prefix and body.
    MissingSeparator,
    /// The body is not exactly [`UID_BODY_LEN`] characters.
    BadLength,
    /// The body contains a character outside `0-9A-Za-z`.
    BadChar,
}

impl fmt::Display for UidParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let msg = match self {
            UidParseError::UnknownPrefix => "unknown uid prefix",
            UidParseError::MissingSeparator => "missing `_` separator",
            UidParseError::BadLength => "uid body must be exactly 16 characters",
            UidParseError::BadChar => "uid body must be base-62 (0-9A-Za-z)",
        };
        f.write_str(msg)
    }
}

impl FromStr for PrefixedUid {
    type Err = UidParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (prefix, body) = s.split_once('_').ok_or(UidParseError::MissingSeparator)?;
        let prefix: UidPrefix = prefix.parse()?;
        if body.len() != UID_BODY_LEN {
            return Err(UidParseError::BadLength);
        }
        let mut bytes = [0u8; UID_BODY_LEN];
        for (slot, ch) in bytes.iter_mut().zip(body.bytes()) {
            if !ch.is_ascii_alphanumeric() {
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
        assert_eq!(uid.to_string(), "prj_0000000000000000");
    }

    #[test]
    fn encodes_known_values() {
        // value 61 -> last digit 'z'
        let mut bytes = [0u8; 16];
        bytes[15] = 61;
        let uid = PrefixedUid::mint(UidPrefix::Device, &bytes);
        assert_eq!(uid.to_string(), "dev_000000000000000z");

        // value 62 -> "10"
        bytes[15] = 62;
        let uid = PrefixedUid::mint(UidPrefix::Module, &bytes);
        assert_eq!(uid.to_string(), "mod_0000000000000010");

        // max value: encoding must stay in-alphabet and length 16
        let uid = PrefixedUid::mint(UidPrefix::Project, &[0xFF; 16]);
        assert_eq!(uid.body_str().len(), UID_BODY_LEN);
        assert!(uid.body_str().bytes().all(|b| b.is_ascii_alphanumeric()));
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
        assert_eq!(
            "prj0000000000000000".parse::<PrefixedUid>(),
            Err(UidParseError::MissingSeparator)
        );
        assert_eq!(
            "xxx_0000000000000000".parse::<PrefixedUid>(),
            Err(UidParseError::UnknownPrefix)
        );
        assert_eq!(
            "prj_000000000000000".parse::<PrefixedUid>(),
            Err(UidParseError::BadLength)
        );
        assert_eq!(
            "prj_00000000000000000".parse::<PrefixedUid>(),
            Err(UidParseError::BadLength)
        );
        assert_eq!(
            "prj_00000000000000!0".parse::<PrefixedUid>(),
            Err(UidParseError::BadChar)
        );
        assert_eq!(
            "".parse::<PrefixedUid>(),
            Err(UidParseError::MissingSeparator)
        );
    }

    #[test]
    fn serde_round_trip_and_rejection() {
        let uid = PrefixedUid::mint(UidPrefix::Project, &[7u8; 16]);
        let json = serde_json::to_string(&uid).unwrap();
        assert_eq!(json, alloc::format!("\"{uid}\""));
        let back: PrefixedUid = serde_json::from_str(&json).unwrap();
        assert_eq!(back, uid);

        let bad: Result<PrefixedUid, _> = serde_json::from_str("\"prj_short\"");
        assert!(bad.is_err());
    }
}
