# Phase 5: Cleanup And Validation

## Scope Of Phase

Remove stale produced-root-plan remnants and validate.

In scope:

- Delete old `ProducedSlotAccess` methods if no longer needed.
- Update rustdocs to say produced resolution reads runtime state slots.
- Write `summary.md`.
- Run focused validation.

Out of scope:

- Broad CI.
- Runtime state sync UI.
- Mutations.

## Validate

```bash
cargo fmt -p lpc-model -p lpc-engine
cargo check -p lpc-engine
cargo test -p lpc-model
cargo test -p lpc-engine
cargo clippy -p lpc-engine -p lpc-model --all-targets -- -D warnings
```

