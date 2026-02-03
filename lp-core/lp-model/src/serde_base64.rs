//! Base64 serialization helpers for binary data
//!
//! Provides serde serializers/deserializers that encode Vec<u8> as base64 strings
//! instead of JSON arrays, which is more space-efficient.
//!
//! Also provides "smart" serialization that encodes text data as plain JSON strings
//! and binary data as base64, optimizing for the common case of text files.

use alloc::{string::String, vec::Vec};
use serde::{Deserializer, Serializer};

/// Serialize Vec<u8> as base64 string
pub fn serialize<S>(bytes: &Vec<u8>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    use base64::Engine;
    let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
    serializer.serialize_str(&encoded)
}

/// Deserialize base64 string to Vec<u8>
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

/// Serialize Option<Vec<u8>> as base64 string (None becomes null)
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

/// Deserialize base64 string or null to Option<Vec<u8>>
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

/// Smart serialize Vec<u8>: text as string, binary as base64
///
/// If the bytes are valid UTF-8, serializes as a plain JSON string.
/// Otherwise, serializes as a base64-encoded string.
pub fn serialize_smart<S>(bytes: &Vec<u8>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    // Check if bytes are valid UTF-8
    match core::str::from_utf8(bytes) {
        Ok(text) => {
            // Valid UTF-8: serialize as plain string
            serializer.serialize_str(text)
        }
        Err(_) => {
            // Invalid UTF-8: serialize as base64
            use base64::Engine;
            let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
            serializer.serialize_str(&encoded)
        }
    }
}

/// Smart deserialize: handles both plain string (UTF-8) and base64 string
///
/// If the string decodes successfully as base64 AND the decoded bytes are NOT valid UTF-8,
/// treat it as base64-encoded binary data. Otherwise, treat it as UTF-8 text.
pub fn deserialize_smart<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
where
    D: Deserializer<'de>,
{
    use base64::Engine;
    use serde::Deserialize;
    let s = String::deserialize(deserializer)?;

    // Try to decode as base64
    if let Ok(decoded_bytes) = base64::engine::general_purpose::STANDARD.decode(&s) {
        // Check if decoded bytes are valid UTF-8
        if core::str::from_utf8(&decoded_bytes).is_err() {
            // Decoded bytes are NOT valid UTF-8, so this was base64-encoded binary
            return Ok(decoded_bytes);
        }
        // Decoded bytes ARE valid UTF-8 - check if they match the original string
        // If they do, it was likely base64-encoded text (less common), use decoded bytes
        // If they don't match, the string itself was the text, use UTF-8 encoding
        if let Ok(decoded_text) = core::str::from_utf8(&decoded_bytes) {
            if decoded_text == s {
                // The string equals its base64 decode - this was base64-encoded text
                return Ok(decoded_bytes);
            }
        }
    }

    // Treat as UTF-8 text: convert string to bytes
    Ok(s.into_bytes())
}

/// Smart serialize Option<Vec<u8>>: text as string, binary as base64
pub fn serialize_option_smart<S>(bytes: &Option<Vec<u8>>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    match bytes {
        Some(bytes) => {
            // Use smart serialization for Some variant
            serialize_smart(bytes, serializer)
        }
        None => serializer.serialize_none(),
    }
}

/// Smart deserialize Option: handles both plain string (UTF-8) and base64 string
pub fn deserialize_option_smart<'de, D>(deserializer: D) -> Result<Option<Vec<u8>>, D::Error>
where
    D: Deserializer<'de>,
{
    use base64::Engine;
    use serde::Deserialize;
    let opt: Option<String> = Option::deserialize(deserializer)?;
    match opt {
        Some(s) => {
            // Use same logic as deserialize_smart
            // Try to decode as base64
            if let Ok(decoded_bytes) = base64::engine::general_purpose::STANDARD.decode(&s) {
                // Check if decoded bytes are valid UTF-8
                if core::str::from_utf8(&decoded_bytes).is_err() {
                    // Decoded bytes are NOT valid UTF-8, so this was base64-encoded binary
                    return Ok(Some(decoded_bytes));
                }
                // Decoded bytes ARE valid UTF-8 - check if they match the original string
                if let Ok(decoded_text) = core::str::from_utf8(&decoded_bytes) {
                    if decoded_text == s {
                        // The string equals its base64 decode - this was base64-encoded text
                        return Ok(Some(decoded_bytes));
                    }
                }
            }

            // Treat as UTF-8 text: convert string to bytes
            Ok(Some(s.into_bytes()))
        }
        None => Ok(None),
    }
}
