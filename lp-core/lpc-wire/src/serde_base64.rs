//! Base64 serialization helpers for binary payloads on the wire.

use alloc::{string::String, vec::Vec};
use serde::{Deserializer, Serializer};

/// Serialize `Vec<u8>` as a base64 string.
pub fn serialize<S>(bytes: &Vec<u8>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    use base64::Engine;
    let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
    serializer.serialize_str(&encoded)
}

/// Deserialize a base64 string to `Vec<u8>`.
pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
where
    D: Deserializer<'de>,
{
    use base64::Engine;
    use serde::Deserialize;
    let s = String::deserialize(deserializer)?;
    base64::engine::general_purpose::STANDARD
        .decode(s)
        .map_err(serde::de::Error::custom)
}

/// Serialize `Option<Vec<u8>>` as base64 (`None` → JSON null).
pub fn serialize_option<S>(bytes: &Option<Vec<u8>>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    match bytes {
        Some(bytes) => {
            use base64::Engine;
            let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
            serializer.serialize_some(&encoded)
        }
        None => serializer.serialize_none(),
    }
}

/// Deserialize base64 string or null to `Option<Vec<u8>>`.
pub fn deserialize_option<'de, D>(deserializer: D) -> Result<Option<Vec<u8>>, D::Error>
where
    D: Deserializer<'de>,
{
    use base64::Engine;
    use serde::Deserialize;
    let opt: Option<String> = Option::deserialize(deserializer)?;
    match opt {
        Some(s) => base64::engine::general_purpose::STANDARD
            .decode(s)
            .map(Some)
            .map_err(serde::de::Error::custom),
        None => Ok(None),
    }
}

/// Serialize UTF-8 text as a JSON string; arbitrary bytes as base64.
pub fn serialize_smart<S>(bytes: &Vec<u8>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    match core::str::from_utf8(bytes) {
        Ok(text) => serializer.serialize_str(text),
        Err(_) => {
            use base64::Engine;
            let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
            serializer.serialize_str(&encoded)
        }
    }
}

/// Deserialize smart string: plain UTF-8 or base64 binary (see original `lpc-model` logic).
pub fn deserialize_smart<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
where
    D: Deserializer<'de>,
{
    use base64::Engine;
    use serde::Deserialize;
    let s = String::deserialize(deserializer)?;

    if let Ok(decoded_bytes) = base64::engine::general_purpose::STANDARD.decode(&s) {
        if core::str::from_utf8(&decoded_bytes).is_err() {
            return Ok(decoded_bytes);
        }
        if let Ok(decoded_text) = core::str::from_utf8(&decoded_bytes) {
            if decoded_text == s {
                return Ok(decoded_bytes);
            }
        }
    }

    Ok(s.into_bytes())
}

/// Smart serialize `Option<Vec<u8>>`.
pub fn serialize_option_smart<S>(bytes: &Option<Vec<u8>>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    match bytes {
        Some(bytes) => serialize_smart(bytes, serializer),
        None => serializer.serialize_none(),
    }
}

/// Smart deserialize `Option<Vec<u8>>`.
pub fn deserialize_option_smart<'de, D>(deserializer: D) -> Result<Option<Vec<u8>>, D::Error>
where
    D: Deserializer<'de>,
{
    use base64::Engine;
    use serde::Deserialize;
    let opt: Option<String> = Option::deserialize(deserializer)?;
    match opt {
        Some(s) => {
            if let Ok(decoded_bytes) = base64::engine::general_purpose::STANDARD.decode(&s) {
                if core::str::from_utf8(&decoded_bytes).is_err() {
                    return Ok(Some(decoded_bytes));
                }
                if let Ok(decoded_text) = core::str::from_utf8(&decoded_bytes) {
                    if decoded_text == s {
                        return Ok(Some(decoded_bytes));
                    }
                }
            }
            Ok(Some(s.into_bytes()))
        }
        None => Ok(None),
    }
}
