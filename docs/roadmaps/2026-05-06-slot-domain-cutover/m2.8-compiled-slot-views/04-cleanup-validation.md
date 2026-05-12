# Phase 4: Cleanup And Validation

## Scope

Make the new accessor/view model clear, documented, and green.

## Implementation Details

- Add rustdocs explaining `SlotPath` vs `SlotAccessor` vs generated views.
- Keep `lookup_slot_data` for dynamic/general use and tests.
- Ensure generated code stays readable and does not add large `mod.rs` blobs.
- Remove temporary compatibility helpers if they are no longer needed.
- Keep TODOs only for explicit future work:
  - accessor-aware resolver query keys,
  - map/enum/option compiled steps,
  - view cache stored on runtime nodes or engine session.

## Validate

```bash
cargo fmt --check
cargo test -p lpc-model
cargo test -p lpc-engine
cargo clippy -p lpc-engine -p lpc-model -p lpc-slot-macros --all-targets -- -D warnings
```

