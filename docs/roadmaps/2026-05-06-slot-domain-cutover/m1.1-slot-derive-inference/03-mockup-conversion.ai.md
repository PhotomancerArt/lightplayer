# Phase 3: Mockup Conversion

## Scope Of Phase

Convert existing mockup/test slot derives to the cleaner inferred syntax where it is exact.

In scope:

- Update `lpc-model` derive tests.
- Update `lpc-slot-mockup` derive usage:
  - replace `#[slot(shape_id = "...")]` with `#[slot(root)]` when explicit ids are not needed,
  - remove field attrs when `FieldSlot` inference is exact,
  - make manual enum fields implement `FieldSlot` where that removes noisy attrs.
- Keep tests and printed evidence working.

Out of scope:

- Real source def conversion.
- Changing mockup domain semantics beyond derive syntax.

## Code Organization Reminders

- Keep fixture/mockup test helpers easy to scan.
- Do not hide semantic shape issues by changing assertions to weaker checks.
- Tests stay at bottoms of files.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests.
- If blocked, stop and report.
- Report what changed, what was validated, and deviations.

## Implementation Details

Relevant files:

- `lp-core/lpc-model/tests/slot_record_derive.rs`
- `lp-core/lpc-slot-mockup/src/source/*.rs`
- `lp-core/lpc-slot-mockup/src/engine/*.rs`

Guidance:

- Remove attrs such as `#[slot(value = ModelType::Bool)]` from `ValueSlot<bool>` fields.
- Remove attrs such as `#[slot(record)]` from derived-record fields.
- Remove attrs such as `#[slot(map(...))]` where `MapSlot<K,V>` inference is correct.
- Remove attrs such as `#[slot(leaf = ratio_shape())]` where semantic slots are now real newtypes.
- Implement `FieldSlot` for manual enum types that are direct fields.

## Validate

```bash
cargo fmt
cargo test -p lpc-model -p lpc-slot-mockup --lib --tests
```
