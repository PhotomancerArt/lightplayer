# Phase 2: Derive Inference

## Scope Of Phase

Teach `#[derive(SlotRecord)]` to infer fields by default.

In scope:

- Add `#[slot(root)]`.
- Infer root shape id from `module_path!()` and struct name.
- Keep `#[slot(shape_id = "...")]` as explicit root id override.
- Make missing field attrs mean inferred field.
- Support `#[slot(skip)]`.
- Support `#[slot(name = "...")]`.
- Generate `FieldSlot` impl for every derived record.
- Preserve explicit old field shape attrs as overrides.

Out of scope:

- Real source def conversion.

## Code Organization Reminders

- Keep parsing logic in `attr.rs`.
- Keep code generation in `record.rs`.
- Prefer small helper functions over a large inline parser.
- Tests stay at the bottom of test files.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests.
- If blocked, stop and report.
- Report what changed, what was validated, and deviations.

## Implementation Details

Relevant files:

- `lp-core/lpc-slot-macros/src/attr.rs`
- `lp-core/lpc-slot-macros/src/record.rs`
- `lp-core/lpc-model/tests/slot_record_derive.rs`

Expected behavior:

- This compiles:

```rust
#[derive(lpc_model::SlotRecord)]
#[slot(root)]
struct DerivedRecord {
    enabled: ValueSlot<bool>,
    nested: NestedRecord,
}
```

- This excludes `cache`:

```rust
#[slot(skip)]
cache: CachedThing,
```

- This uses a custom slot field name:

```rust
#[slot(name = "count")]
renamed_count: ValueSlot<u32>,
```

- If a field has no `FieldSlot` impl and is not skipped, the generated code should fail to compile through a trait-bound error.

## Validate

```bash
cargo fmt
cargo test -p lpc-model --lib --tests
```
