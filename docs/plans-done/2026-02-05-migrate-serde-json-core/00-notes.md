# Migration to serde-json-core

## Scope of Work

Migrate lp-core from `serde_json` to `serde-json-core` to resolve ESP32 bootloader compatibility issues. The root cause is that `serde_json` causes 8-byte alignment requirements in `.rodata`, which creates a gap between `.rodata_desc` and `.rodata` sections, resulting in 3 MAP segments instead of the required 2.

**Goal**: Replace all `serde_json` usage with `serde-json-core` while maintaining the same API surface through a compatibility wrapper.

## Current State

### Dependencies
- `lp-model/Cargo.toml`: `serde_json = { workspace = true }`
- `lp-server/Cargo.toml`: `serde_json = { workspace = true, default-features = false, features = ["alloc"] }`
- `lp-client/Cargo.toml`: `serde_json = "1"`
- `lp-engine/Cargo.toml`: `serde_json = { workspace = true }`
- `lp-shared/Cargo.toml`: `serde_json = { workspace = true }`

### Usage Patterns

1. **Serialization** (`serde_json::to_string()`):
   - `lp-client/src/transport_ws.rs`: WebSocket message serialization
   - `lp-client/src/transport_serial/emulator.rs`: Serial transport
   - `lp-shared/src/project/builder.rs`: Project file serialization
   - `lp-model/src/server/config.rs`: Config serialization
   - `lp-model/src/project/api.rs`: Test serialization
   - `lp-model/src/message.rs`: Test serialization
   - `lp-model/src/server/fs_api.rs`: Test serialization (many tests)

2. **Deserialization** (`serde_json::from_str()` / `from_slice()`):
   - `lp-client/src/transport_ws.rs`: WebSocket message deserialization
   - `lp-client/src/transport_serial/emulator.rs`: Serial transport
   - `lp-engine/src/project/loader.rs`: File loading (uses `from_slice`)
   - `lp-engine/src/project/runtime.rs`: Node config loading (uses `from_slice`)
   - All test files: Round-trip serialization tests

### Key Challenges

1. **API Differences**:
   - `serde-json-core::to_slice()` requires pre-allocated buffer (no `to_string()`)
   - `serde-json-core::from_slice()` requires `'static` lifetime
   - No heap allocation by default (designed for `no_std`)

2. **Solution**: Create wrapper module that:
   - Uses heap allocation (we have `alloc` available)
   - Provides `to_string()`, `from_str()`, and `from_slice()` APIs matching `serde_json`
   - Handles buffer growth for serialization
   - Copies data to satisfy `'static` requirement for deserialization

## Questions

1. **Where should the wrapper module live?**
   - ✅ **Answer**: `lp-model/src/json.rs` - most crates already depend on lp-model, keeps JSON serialization close to data models

2. **Should we maintain feature flags for std vs no_std?**
   - ✅ **Answer**: No feature flags - completely replace `serde_json` with `serde-json-core` wrapper. Simpler and ensures ESP32 compatibility.

3. **Error type compatibility?**
   - ✅ **Answer**: Simple error wrapper that implements `From` for both `serde_json_core::ser::Error` and `serde_json_core::de::Error`. Allows using `?` operator with minimal code changes.

4. **Testing strategy?**
   - ✅ **Answer**: Update all tests to use the new wrapper module. Tests verify wrapper works correctly and ensure migration is complete.

5. **Performance considerations?**
   - **Answer**: The wrapper approach is essentially what `serde_json` does internally:
     - `serde_json::to_string()` allocates a `String` buffer internally and writes JSON to it
     - Our wrapper: allocate `Vec<u8>`, use `to_slice()` (may need to grow buffer), convert to `String`
     - `serde_json::from_slice()` deserializes into owned types (allocates `String`, `Vec`, etc.)
     - Our wrapper: copy slice to `Vec<u8>` for `'static`, then deserialize (same allocations)
   - Performance should be similar - both use heap allocation. Minor overhead from potential buffer growth, but acceptable for ESP32 compatibility.

6. **Migration order?**
   - ✅ **Answer**: 
     1. Create wrapper module in lp-model
     2. Update lp-model to use wrapper
     3. Update dependent crates (lp-shared, lp-engine, lp-client, lp-server)
     4. Remove serde_json dependencies
     5. Update workspace Cargo.toml
