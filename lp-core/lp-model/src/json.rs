//! JSON serialization/deserialization wrapper using `serde-json-core`
//!
//! # Why This Wrapper Exists
//!
//! This module provides a `serde_json`-compatible API using `serde-json-core`
//! internally. This is necessary to resolve ESP32 bootloader compatibility issues.
//!
//! ## The Problem
//!
//! When using `serde_json` on ESP32 targets, the bootloader fails with:
//!
//! ```
//! Assert failed in unpack_load_app, bootloader_utility.c:836 (rom_index < 2)
//! ```
//!
//! ## Root Cause
//!
//! The ESP32 bootloader expects at most 2 MAP segments (DROM/IROM), but `serde_json`
//! causes the binary to have 3 MAP segments:
//!
//! - Segment 0: `.rodata_desc` (contains `esp_app_desc_t`) - 4-byte aligned
//! - Segment 2: `.rodata` - 8-byte aligned (due to data types in serde_json)
//! - Segment 4: `.text`
//!
//! The issue is that `serde_json` places 8-byte aligned data structures in `.rodata`,
//! creating a 32-byte gap between the 4-byte aligned `.rodata_desc` and the 8-byte
//! aligned `.rodata` sections. The ESP32 binary conversion tool (espflash/esptool)
//! sees this gap and splits them into separate MAP segments.
//!
//! ## The Solution
//!
//! `serde-json-core` is a `no_std` compatible JSON library that doesn't cause the
//! same alignment issues. However, it has a different API:
//!
//! - `to_slice()` instead of `to_string()` (requires pre-allocated buffer)
//! - `from_slice()` requires `'static` lifetime
//! - No heap allocation by default
//!
//! This wrapper provides a `serde_json`-compatible API that:
//! - Uses heap allocation (we have `alloc` available)
//! - Handles buffer growth for serialization
//! - Copies data to satisfy `'static` requirement for deserialization
//!
//! ## Performance
//!
//! The wrapper performs similarly to `serde_json` since both use heap allocation.
//! The wrapper essentially reimplements what `serde_json::to_string()` does internally,
//! but using `serde-json-core` as the backend to avoid alignment issues.
//!
//! ## Serialization Format Changes
//!
//! To work with `serde-json-core`, internally tagged enums have been changed to
//! externally tagged enums (the default). This changes the JSON format:
//!
//! **Before (internally tagged):**
//! ```json
//! {"direction": "client", "id": 1, "msg": {...}}
//! ```
//!
//! **After (externally tagged):**
//! ```json
//! {"Client": {"id": 1, "msg": {...}}}
//! ```
//!
//! This change affects: `Message`, `ClientRequest`, `FsRequest`, `FsResponse`,
//! `ClientMsgBody`, and `ServerMsgBody` enums.
//!
//! ## References
//!
//! - Issue investigation: `/Users/yona/dev/photomancer/esp32-serde-bug` (minimal reproduction project)
//!   - Demonstrates that `serde_json` causes 3 MAP segments
//!   - Shows that `serde-json-core` successfully boots with 2 MAP segments
//! - Bootloader error: `Assert failed in unpack_load_app, bootloader_utility.c:836 (rom_index < 2)`
//!   - Location: ESP-IDF bootloader source, `bootloader_utility.c` line 836
//!   - The bootloader enforces `rom_index < 2` limit for MAP segments
//! - Root cause code: `serde_json` crate places 8-byte aligned data in `.rodata`
//!   - This creates alignment mismatch with 4-byte aligned `.rodata_desc` section
//!   - The gap prevents sections from merging into a single MAP segment

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use serde::{Deserialize, Serialize};
use serde_json_core;

/// Error type for JSON serialization/deserialization
#[derive(Debug)]
pub enum Error {
    /// Serialization error
    Serialization(serde_json_core::ser::Error),
    /// Deserialization error
    Deserialization(serde_json_core::de::Error),
}

impl From<serde_json_core::ser::Error> for Error {
    fn from(e: serde_json_core::ser::Error) -> Self {
        Error::Serialization(e)
    }
}

impl From<serde_json_core::de::Error> for Error {
    fn from(e: serde_json_core::de::Error) -> Self {
        Error::Deserialization(e)
    }
}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Error::Serialization(e) => write!(f, "Serialization error: {e:?}"),
            Error::Deserialization(e) => write!(f, "Deserialization error: {e:?}"),
        }
    }
}

/// Serialize a value to a JSON string
///
/// This function allocates a buffer on the heap and grows it as needed,
/// similar to how `serde_json::to_string()` works internally.
pub fn to_string<T: Serialize>(value: &T) -> Result<String, Error> {
    // Start with 4KB buffer (reasonable default)
    let mut capacity = 4096;
    let mut buffer = Vec::with_capacity(capacity);
    buffer.resize(capacity, 0);

    loop {
        match serde_json_core::to_slice(value, &mut buffer) {
            Ok(len) => {
                // Success - convert buffer to String
                let json_str = core::str::from_utf8(&buffer[..len])
                    .map_err(|_| Error::Serialization(serde_json_core::ser::Error::BufferFull))?;
                return Ok(json_str.to_string());
            }
            Err(serde_json_core::ser::Error::BufferFull) => {
                // Buffer too small - double capacity and retry
                capacity *= 2;
                buffer.resize(capacity, 0);
            }
            Err(e) => {
                // Other error - return it
                return Err(Error::from(e));
            }
        }
    }
}

/// Deserialize a value from a JSON string
///
/// This function deserializes directly from the input string.
/// The deserialized type must be owned (e.g., String, Vec, etc.).
///
/// Uses `from_slice_escaped` instead of `from_slice` to properly handle escaped
/// characters in JSON strings (e.g., `\"`, `\\`, etc.). This is required because
/// `serde_json_core::from_slice` does not unescape strings when deserializing
/// into owned types.
pub fn from_str<T>(s: &str) -> Result<T, Error>
where
    T: for<'de> Deserialize<'de>,
{
    // Copy to Vec<u8> to satisfy 'static lifetime requirement
    let bytes = s.as_bytes().to_vec();

    // Allocate buffer for unescaping strings (2x input size should be sufficient)
    let mut unescape_buffer = Vec::with_capacity(bytes.len() * 2);
    unescape_buffer.resize(bytes.len() * 2, 0);

    let (result, _) = serde_json_core::from_slice_escaped(&bytes, &mut unescape_buffer)
        .map_err(|e| Error::Deserialization(e))?;
    Ok(result)
}

/// Deserialize a value from a JSON byte slice
///
/// This function deserializes directly from the input slice.
/// The deserialized type must be owned (e.g., String, Vec, etc.).
///
/// Uses `from_slice_escaped` instead of `from_slice` to properly handle escaped
/// characters in JSON strings (e.g., `\"`, `\\`, etc.). This is required because
/// `serde_json_core::from_slice` does not unescape strings when deserializing
/// into owned types.
pub fn from_slice<T>(bytes: &[u8]) -> Result<T, Error>
where
    T: for<'de> Deserialize<'de>,
{
    // Copy to Vec<u8> to satisfy 'static lifetime requirement
    let owned = bytes.to_vec();

    // Allocate buffer for unescaping strings (2x input size should be sufficient)
    let mut unescape_buffer = Vec::with_capacity(owned.len() * 2);
    unescape_buffer.resize(owned.len() * 2, 0);

    let (result, _) = serde_json_core::from_slice_escaped(&owned, &mut unescape_buffer)
        .map_err(|e| Error::Deserialization(e))?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Serialize, Deserialize, PartialEq)]
    struct TestStruct {
        test: u32,
        name: String,
    }

    #[test]
    fn test_to_string() {
        let value = TestStruct {
            test: 42,
            name: "test".to_string(),
        };
        let json = to_string(&value).unwrap();
        assert!(json.contains("\"test\":42"));
        assert!(json.contains("\"name\":\"test\""));
    }

    #[test]
    fn test_from_str() {
        let json = r#"{"test":42,"name":"test"}"#;
        let value: TestStruct = from_str(json).unwrap();
        assert_eq!(value.test, 42);
        assert_eq!(value.name, "test");
    }

    #[test]
    fn test_round_trip() {
        let original = TestStruct {
            test: 42,
            name: "test".to_string(),
        };
        let json = to_string(&original).unwrap();
        let deserialized: TestStruct = from_str(&json).unwrap();
        assert_eq!(original, deserialized);
    }

    #[test]
    fn test_project_config_round_trip() {
        use crate::project::ProjectConfig;
        use alloc::string::ToString;

        let original = ProjectConfig {
            uid: "test".to_string(),
            name: "Test Project".to_string(),
        };
        let json = to_string(&original).unwrap();

        // Verify JSON format is correct (not double-escaped)
        assert!(json.contains("\"uid\":\"test\""));
        assert!(json.contains("\"name\":\"Test Project\""));
        assert!(
            !json.contains("\\\\\""),
            "JSON should not be double-escaped: {}",
            json
        );

        // Verify round-trip deserialization
        let deserialized: ProjectConfig = from_str(&json).unwrap();
        assert_eq!(original.uid, deserialized.uid);
        assert_eq!(original.name, deserialized.name);
    }

    #[test]
    fn test_project_config_from_slice() {
        use crate::project::ProjectConfig;
        use alloc::string::ToString;

        let original = ProjectConfig {
            uid: "test".to_string(),
            name: "Test Project".to_string(),
        };
        let json = to_string(&original).unwrap();
        let json_bytes = json.as_bytes();

        // Test from_slice (used by loader)
        let deserialized: ProjectConfig = from_slice(json_bytes).unwrap();
        assert_eq!(original.uid, deserialized.uid);
        assert_eq!(original.name, deserialized.name);
    }
}
