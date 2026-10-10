//! Serde helpers: fixed-size byte arrays as base64 strings.
//!
//! The repo's convention for binary in JSON is a plain base64 string over
//! the STANDARD alphabet, padded — the same spelling `lpc-access` uses for
//! the salts and keys in the device access files, so a salt read from the
//! cloud and one read from a board compare as text. A string that decodes
//! to the wrong length is refused, so a truncated key can never
//! deserialize into a zero-padded one.
//!
//! Use as `#[serde(with = "crate::base64_bytes")]` on a `[u8; N]`, or
//! `#[serde(with = "crate::base64_bytes::list")]` on a `Vec<[u8; N]>`. Bytes
//! of any length are a [`Base64Bytes`].

use alloc::string::String;
use alloc::vec::Vec;
use base64::Engine;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Bytes of any length, as one base64 string (STANDARD, padded) in JSON.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Base64Bytes(pub Vec<u8>);

impl Serialize for Base64Bytes {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&encode(&self.0))
    }
}

impl<'de> Deserialize<'de> for Base64Bytes {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        base64::engine::general_purpose::STANDARD
            .decode(text)
            .map(Self)
            .map_err(|error| serde::de::Error::custom(alloc::format!("invalid base64: {error}")))
    }
}

/// Serialize `[u8; N]` as a base64 string.
pub fn serialize<S, const N: usize>(bytes: &[u8; N], serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str(&encode(bytes))
}

/// Deserialize a base64 string into exactly `N` bytes.
pub fn deserialize<'de, D, const N: usize>(deserializer: D) -> Result<[u8; N], D::Error>
where
    D: Deserializer<'de>,
{
    let text = String::deserialize(deserializer)?;
    decode::<N>(&text).map_err(serde::de::Error::custom)
}

/// `Vec<[u8; N]>` as a JSON array of base64 strings.
pub mod list {
    use alloc::string::String;
    use alloc::vec::Vec;
    use serde::ser::SerializeSeq;
    use serde::{Deserialize, Deserializer, Serializer};

    /// Serialize each array as a base64 string.
    pub fn serialize<S, const N: usize>(items: &[[u8; N]], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut seq = serializer.serialize_seq(Some(items.len()))?;
        for item in items {
            seq.serialize_element(&super::encode(item))?;
        }
        seq.end()
    }

    /// Deserialize an array of base64 strings, each exactly `N` bytes.
    pub fn deserialize<'de, D, const N: usize>(deserializer: D) -> Result<Vec<[u8; N]>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let texts = Vec::<String>::deserialize(deserializer)?;
        texts
            .iter()
            .map(|text| super::decode::<N>(text).map_err(serde::de::Error::custom))
            .collect()
    }
}

fn encode(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

fn decode<const N: usize>(text: &str) -> Result<[u8; N], String> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(text)
        .map_err(|error| alloc::format!("invalid base64: {error}"))?;
    <[u8; N]>::try_from(bytes.as_slice())
        .map_err(|_| alloc::format!("expected {N} bytes, got {}", bytes.len()))
}

#[cfg(test)]
mod tests {
    use alloc::vec;
    use alloc::vec::Vec;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    struct Holder {
        #[serde(with = "crate::base64_bytes")]
        bytes: [u8; 4],
        #[serde(with = "crate::base64_bytes::list")]
        list: Vec<[u8; 2]>,
    }

    #[test]
    fn round_trips_as_standard_padded_base64() {
        let holder = Holder {
            bytes: [0xde, 0xad, 0xbe, 0xef],
            list: vec![[0xff, 0x00], [0x01, 0x02]],
        };
        let json = serde_json::to_string(&holder).unwrap();
        assert_eq!(json, r#"{"bytes":"3q2+7w==","list":["/wA=","AQI="]}"#);
        let back: Holder = serde_json::from_str(&json).unwrap();
        assert_eq!(back, holder);
    }

    #[test]
    fn bytes_of_any_length_round_trip_as_one_string() {
        let bytes = super::Base64Bytes(vec![0xff, 0x00, 0x80, 0x01, 0x02]);
        let json = serde_json::to_string(&bytes).unwrap();
        assert_eq!(json, r#""/wCAAQI=""#);
        assert_eq!(
            serde_json::from_str::<super::Base64Bytes>(&json).unwrap(),
            bytes
        );
        assert_eq!(
            serde_json::to_string(&super::Base64Bytes(Vec::new())).unwrap(),
            r#""""#
        );
        assert!(serde_json::from_str::<super::Base64Bytes>(r#""not base64!""#).is_err());
    }

    #[test]
    fn wrong_length_is_refused() {
        for bad in [
            r#"{"bytes":"3q2+","list":[]}"#,
            r#"{"bytes":"3q2+7w==","list":["AQID"]}"#,
            r#"{"bytes":"not base64!","list":[]}"#,
        ] {
            assert!(serde_json::from_str::<Holder>(bad).is_err(), "{bad}");
        }
    }
}
