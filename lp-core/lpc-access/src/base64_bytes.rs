//! Serde helpers: fixed-size byte arrays as base64 strings.
//!
//! The repo's convention for binary on the wire and in files is a plain
//! base64 string over the STANDARD alphabet, padded (`lpc-wire`'s
//! `serde_base64`, the display-layout packing, the fs chunk payloads). This
//! module applies the same encoding to `[u8; N]` fields — salts, derived
//! keys, challenges, MACs — and refuses a string that decodes to the wrong
//! length, so a truncated key can never deserialize into a zero-padded one.
//!
//! Use as `#[serde(with = "crate::base64_bytes")]`.

use alloc::string::String;
use base64::Engine;
use serde::{Deserialize, Deserializer, Serializer};

/// Serialize `[u8; N]` as a base64 string.
pub fn serialize<S, const N: usize>(bytes: &[u8; N], serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
    serializer.serialize_str(&encoded)
}

/// Deserialize a base64 string into exactly `N` bytes.
pub fn deserialize<'de, D, const N: usize>(deserializer: D) -> Result<[u8; N], D::Error>
where
    D: Deserializer<'de>,
{
    let text = String::deserialize(deserializer)?;
    decode::<N>(&text).map_err(serde::de::Error::custom)
}

/// Decode a base64 string that must hold exactly `N` bytes.
pub fn decode<const N: usize>(text: &str) -> Result<[u8; N], alloc::string::String> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(text)
        .map_err(|error| alloc::format!("invalid base64: {error}"))?;
    <[u8; N]>::try_from(bytes.as_slice())
        .map_err(|_| alloc::format!("expected {N} bytes, got {}", bytes.len()))
}

#[cfg(test)]
mod tests {
    use serde::{Deserialize, Serialize};

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    struct Holder {
        #[serde(with = "crate::base64_bytes")]
        bytes: [u8; 4],
    }

    #[test]
    fn round_trips_as_standard_padded_base64() {
        let holder = Holder {
            bytes: [0xde, 0xad, 0xbe, 0xef],
        };
        let json = serde_json::to_string(&holder).unwrap();
        assert_eq!(json, "{\"bytes\":\"3q2+7w==\"}");
        let back: Holder = serde_json::from_str(&json).unwrap();
        assert_eq!(back, holder);
    }

    #[test]
    fn wrong_length_is_refused() {
        assert!(serde_json::from_str::<Holder>("{\"bytes\":\"3q2+\"}").is_err());
        assert!(serde_json::from_str::<Holder>("{\"bytes\":\"3q2+7wA=\"}").is_err());
        assert!(serde_json::from_str::<Holder>("{\"bytes\":\"not base64!\"}").is_err());
    }
}
