# Phase 1: Static Shape Registry API

## Scope Of Phase

Add the model-layer primitives needed for idempotent static shape registration.

In scope:

- Add `StaticSlotShape` to `lpc-model`.
- Adjust `StaticSlotAccess` to build on `StaticSlotShape` while preserving
  existing `register_shape` call sites.
- Add idempotent registry APIs such as `contains` and `ensure_tree`.
- Add a clear registry error for conflicting static shape ids.
- Add shape traversal helpers to collect `SlotShape::Ref` ids.

Out of scope:

- Build codegen.
- Derive macro changes.
- Mockup conversion.
- Source def conversion.

## Code Organization Reminders

- Prefer granular files with one main concept per file.
- Keep helpers lower in the file.
- Keep tests at the bottom of the file.
- Preserve `no_std + alloc` in `lpc-model`.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests.
- If blocked, stop and report.
- Report changed files, validation, and deviations.

## Implementation Details

Relevant files:

- `lp-core/lpc-model/src/slot/slot_access.rs`
- `lp-core/lpc-model/src/slot/slot_shape.rs`
- `lp-core/lpc-model/src/slot/slot_shape_registry.rs`
- `lp-core/lpc-model/src/slot/mod.rs`
- `lp-core/lpc-model/src/lib.rs`

Expected changes:

- Introduce `StaticSlotShape`.
- Make static ensure idempotent:
  - same id + same shape: ok, no duplicate error.
  - same id + different shape: error.
  - new id: insert with `current_state_version()`.
- Preserve `register_tree` behavior if current tests expect duplicate ids to be
  errors. Add new `ensure_tree` rather than weakening `register_tree`.
- Add a helper that collects referenced shape ids from a `SlotShape` tree.
  Returning a `Vec<SlotShapeId>` is fine.
- Add tests for:
  - `ensure_tree` first insert,
  - `ensure_tree` same-shape idempotence,
  - `ensure_tree` conflicting shape error,
  - reference-id collection over nested records/maps/enums/options.

## Validate

```bash
cargo fmt --package lpc-model
cargo test -p lpc-model --lib --tests
cargo check -p lpc-model --features schema-gen
```
