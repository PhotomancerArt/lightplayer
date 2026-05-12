# Phase 1: Consolidate Uncommitted Changes

## Scope of phase

Clean up uncommitted work: keep useful lp-model and justfile changes; remove the chunked approach (ChunkingSerWrite, OUTGOING_CHUNKS) from fw-esp32. Unstage and revert transport.rs to allow rewrite in phase 4; temporarily revert io_task and main to use MessageRouterTransport so the workspace compiles.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions at the bottom of files
- Keep related functionality grouped together
- Any temporary code should be a TODO comment so we find it later

## Implementation Details

### 1. Reset fw-esp32 chunked work

- Unstage `lp-fw/fw-esp32/src/transport.rs` and `lp-fw/fw-esp32/src/tests/test_json.rs`
- Revert `lp-fw/fw-esp32/src/transport.rs` to empty or remove (transport module will be rewritten in phase 4)
- Revert `lp-fw/fw-esp32/src/serial/io_task.rs` to remove OUTGOING_CHUNKS, get_chunk_channel()
- Revert `lp-fw/fw-esp32/src/main.rs` to use MessageRouterTransport from fw-core instead of StreamingMessageRouterTransport
- Remove `lp-fw/fw-esp32/src/tests/test_json.rs` (will be recreated in phase 8) or keep as stub
- Revert `lp-fw/fw-esp32/Cargo.toml` to remove ser-write-json and test_json (will re-add in phase 3, 8)
- Revert `justfile` fwtest-json-esp32c6 (will restore in phase 8)

### 2. Keep lp-model changes

- **lp-core/lp-model/Cargo.toml**: Keep ser-write-json feature and dependency
- **lp-core/lp-model/src/json.rs**: Keep ser_write_json_tests module
- **lp-core/lp-model/src/project/api.rs**: Keep NodeStateSerializer, serialize_struct_variant changes

### 3. Handle transport module in main.rs

- Remove `mod transport` and `StreamingMessageRouterTransport` usage
- Use `fw_core::transport::MessageRouterTransport` with `MessageRouter`
- MessageRouterTransport serializes to full JSON string - will hit 32KB limit for large messages, but workspace compiles. We fix this in phase 4.

### 4. Ensure lp-model compiles with ser-write-json

- `lp-model` needs ser-write-json for the tests. Ensure `cargo test -p lp-model --features ser-write-json` passes.

## Validate

```bash
just check
cargo test -p lp-model --features ser-write-json
just build-fw-esp32
```

Expect: Workspace compiles; fw-esp32 uses MessageRouterTransport; lp-model ser-write-json tests pass.
