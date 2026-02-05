# Design: serde-json-core Migration

## Scope of Work

Migrate lp-core from `serde_json` to `serde-json-core` to resolve ESP32 bootloader compatibility issues. The migration maintains the same API surface through a compatibility wrapper module.

## File Structure

```
lp-core/
└── lp-model/
    └── src/
        ├── json.rs              # NEW: Wrapper module providing serde_json-compatible API
        └── lib.rs               # UPDATE: Export json module
```

## Conceptual Architecture

```
┌─────────────────────────────────────────────────────────┐
│                    Application Code                      │
│  (lp-client, lp-server, lp-engine, lp-shared, etc.)    │
└────────────────────┬────────────────────────────────────┘
                     │ Uses
                     ▼
┌─────────────────────────────────────────────────────────┐
│              lp-model::json Module                      │
│  ┌──────────────────────────────────────────────────┐  │
│  │ to_string<T: Serialize>(value: &T) -> String    │  │
│  │ from_str<T: Deserialize>(s: &str) -> T          │  │
│  │ from_slice<T: Deserialize>(bytes: &[u8]) -> T   │  │
│  └──────────────────────────────────────────────────┘  │
│  ┌──────────────────────────────────────────────────┐  │
│  │ Error type (wraps ser/de errors)                 │  │
│  └──────────────────────────────────────────────────┘  │
└────────────────────┬────────────────────────────────────┘
                     │ Uses
                     ▼
┌─────────────────────────────────────────────────────────┐
│           serde-json-core Library                      │
│  ┌──────────────────────────────────────────────────┐  │
│  │ to_slice(value, buffer) -> Result<usize>        │  │
│  │ from_slice(bytes: &'static [u8]) -> Result<T>   │  │
│  └──────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────┘
```

## Main Components

### 1. `lp-model/src/json.rs` - Wrapper Module

Provides three main functions matching `serde_json` API:

**`to_string<T: Serialize>(value: &T) -> Result<String, Error>`**
- Allocates `Vec<u8>` buffer (starts at 4KB)
- Calls `serde_json_core::to_slice()` 
- If buffer too small, doubles size and retries
- Converts final buffer to `String`

**`from_str<T: Deserialize<'de>>(s: &str) -> Result<T, Error>`**
- Copies `&str` to `Vec<u8>` (satisfies `'static` requirement)
- Calls `serde_json_core::from_slice()`
- Returns deserialized value

**`from_slice<T: Deserialize<'de>>(bytes: &[u8]) -> Result<T, Error>`**
- Copies `&[u8]` to `Vec<u8>` (satisfies `'static` requirement)
- Calls `serde_json_core::from_slice()`
- Returns deserialized value

**Error Type**
- Wraps both `serde_json_core::ser::Error` and `serde_json_core::de::Error`
- Implements `From` for both error types
- Provides `Display` and `std::error::Error` implementations

### 2. Module Export

Update `lp-model/src/lib.rs` to export the `json` module:
```rust
pub mod json;
```

### 3. Usage Pattern

Replace:
```rust
use serde_json::{to_string, from_str, from_slice};
```

With:
```rust
use lp_model::json::{to_string, from_str, from_slice};
```

## How Components Interact

1. **Application code** calls wrapper functions (`to_string`, `from_str`, `from_slice`)
2. **Wrapper module** handles buffer management and lifetime requirements
3. **serde-json-core** performs actual JSON serialization/deserialization
4. **Wrapper** converts results back to expected types (`String`, owned structs)

## Key Design Decisions

1. **Heap allocation**: Wrapper uses `alloc` crate for `Vec<u8>` and `String` - same as `serde_json` internally
2. **Buffer growth**: Starts at 4KB, doubles on `BufferTooSmall` error - similar to `serde_json` behavior
3. **Lifetime handling**: Copies input data to `Vec<u8>` for `'static` requirement - necessary for `serde-json-core::from_slice()`
4. **Error handling**: Unified error type allows using `?` operator seamlessly
5. **API compatibility**: Functions match `serde_json` signatures exactly - drop-in replacement
