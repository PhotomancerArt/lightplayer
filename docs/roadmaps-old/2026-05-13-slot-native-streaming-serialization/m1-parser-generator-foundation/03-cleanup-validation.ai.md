# Phase 3: Cleanup And Validation

## Scope

Review the M1 diff, remove scratch code, and run focused validation.

## Validation

```bash
cargo test -p lpc-model slot_codec
cargo test -p lpc-wire slot
cargo test -p lpc-slot-mockup native_stream
cargo check -p lpc-wire --no-default-features
```
