# Phase 1: Foundation API

## Scope

Add the syntax event, JSON parser, TOML adapter, slot reader, and JSON writer
facade.

## Out Of Scope

- Codegen.
- Production loading.
- Full slot-shape-driven decode.

## Implementation Details

- Keep the API in `lpc-wire` under `slot/native.rs` or a small submodule.
- Reuse `JsonWriter` instead of creating another low-level byte writer.
- Keep errors path-aware enough for tests and future diagnostics.
- Keep the code `no_std + alloc` compatible.

## Validation

```bash
cargo test -p lpc-wire slot::native
```

