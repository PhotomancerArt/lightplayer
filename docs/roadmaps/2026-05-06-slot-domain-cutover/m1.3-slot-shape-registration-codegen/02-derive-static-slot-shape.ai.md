# Phase 2: Derive StaticSlotShape

## Scope Of Phase

Teach `#[derive(SlotRecord)]` to generate the new static shape trait for root
records.

In scope:

- Update `lpc-slot-macros` root derive output.
- Keep existing root `SlotAccess` behavior.
- Keep existing `StaticSlotAccess::register_shape` call sites working.
- Update derive tests.

Out of scope:

- Build codegen.
- Mockup registration cleanup.
- Real source conversion.
- Typed reference attribute redesign.

## Code Organization Reminders

- Keep derive parsing in `attr.rs` and code generation in `record.rs`.
- Keep tests focused and at the bottom of files or in existing integration test
  files.
- Do not introduce macro attributes that M1.3 does not use.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests.
- If blocked, stop and report.
- Report changed files, validation, and deviations.

## Implementation Details

Relevant files:

- `lp-core/lpc-slot-macros/src/record.rs`
- `lp-core/lpc-model/tests/slot_record_derive.rs`

Expected changes:

- For `#[slot(root)]` records, generate:
  - `impl StaticSlotShape for Type`
  - `impl StaticSlotAccess for Type`
- `StaticSlotShape::slot_shape()` should return the record shape.
- `StaticSlotShape::ensure_registered()` can use the default implementation if
  the trait provides one.
- Keep the shape id inference from M1.1:
  `concat!(module_path!(), "::", stringify!(Type))` unless `shape_id = "..."`
  is provided.
- Update derive tests to assert:
  - `Type::SHAPE_ID` still works through `StaticSlotShape`/`StaticSlotAccess`,
  - `Type::ensure_registered(...)` works,
  - existing `Type::register_shape(...)` compatibility still works.

## Validate

```bash
cargo fmt --package lpc-model --package lpc-slot-macros
cargo test -p lpc-model --features derive --test slot_record_derive
cargo test -p lpc-model --lib --tests
```
