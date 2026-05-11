# Phase 1: Create Wrapper Module and Error Type

## Scope of Phase

Create the `lp-model/src/json.rs` wrapper module that provides a `serde_json`-compatible API using `serde-json-core` internally. This includes implementing the error type and all three main functions (`to_string`, `from_str`, `from_slice`).

## Code Organization Reminders

- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later

## Implementation Details

### Step 1: Add serde-json-core dependency

Update `lp-core/lp-model/Cargo.toml`:
- Add `serde-json-core = { version = "0.5", default-features = false }`
- Keep `serde_json` for now (we'll remove it in phase 7)

### Step 2: Add comprehensive module documentation

At the top of `lp-model/src/json.rs`, add detailed documentation explaining why this wrapper is necessary:

```rust
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
```

### Step 3: Create error type

In `lp-model/src/json.rs`, create an error type that wraps both serialization and deserialization errors:

```rust
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
            Error::Serialization(e) => write!(f, "Serialization error: {:?}", e),
            Error::Deserialization(e) => write!(f, "Deserialization error: {:?}", e),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for Error {}
```

### Step 4: Implement `to_string`

```rust
use alloc::vec::Vec;
use alloc::string::String;
use serde::Serialize;

/// Serialize a value to a JSON string
///
/// This function allocates a buffer on the heap and grows it as needed,
/// similar to how `serde_json::to_string()` works internally.
pub fn to_string<T: Serialize>(value: &T) -> Result<String, Error> {
    // Start with 4KB buffer (reasonable default)
    let mut buffer = Vec::with_capacity(4096);
    
    loop {
        match serde_json_core::to_slice(value, &mut buffer) {
            Ok(len) => {
                // Success - convert buffer to String
                let json_str = core::str::from_utf8(&buffer[..len])
                    .map_err(|_| Error::Serialization(serde_json_core::ser::Error::InvalidValue))?;
                return Ok(json_str.to_string());
            }
            Err(serde_json_core::ser::Error::BufferTooSmall) => {
                // Buffer too small - double capacity and retry
                buffer.resize(buffer.capacity() * 2, 0);
            }
            Err(e) => {
                // Other error - return it
                return Err(Error::from(e));
            }
        }
    }
}
```

### Step 5: Implement `from_str`

```rust
use serde::Deserialize;

/// Deserialize a value from a JSON string
///
/// This function copies the string to a Vec<u8> to satisfy the 'static
/// lifetime requirement of serde_json_core::from_slice().
pub fn from_str<'de, T: Deserialize<'de>>(s: &str) -> Result<T, Error> {
    // Copy to Vec<u8> to get 'static lifetime
    let bytes = s.as_bytes().to_vec();
    let (result, _) = serde_json_core::from_slice(&bytes)?;
    Ok(result)
}
```

### Step 6: Implement `from_slice`

```rust
/// Deserialize a value from a JSON byte slice
///
/// This function copies the slice to a Vec<u8> to satisfy the 'static
/// lifetime requirement of serde_json_core::from_slice().
pub fn from_slice<'de, T: Deserialize<'de>>(bytes: &[u8]) -> Result<T, Error> {
    // Copy to Vec<u8> to get 'static lifetime
    let owned = bytes.to_vec();
    let (result, _) = serde_json_core::from_slice(&owned)?;
    Ok(result)
}
```

### Step 6: Export module

Update `lp-model/src/lib.rs` to export the module:
```rust
pub mod json;
```

### Step 8: Add basic test

Add a simple test to verify the wrapper works:

```rust
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
}
```

## Validate

Run the following commands to validate:

```bash
cd lp-core/lp-model
cargo check
cargo test --lib json
```

Expected results:
- Code compiles without errors
- Tests pass
- No warnings (except possibly unused imports, which we'll use in later phases)
