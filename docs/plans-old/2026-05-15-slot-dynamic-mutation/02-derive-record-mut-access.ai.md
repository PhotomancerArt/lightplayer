# Phase 2: Derive Record Mutable Access

## Scope Of Phase

Extend `#[derive(SlotRecord)]` so every derived slot record also exposes mutable field dispatch.

In scope:

- Generate `SlotRecordMutAccess` for named public fields.
- Generate `SlotMutAccess` for records.
- Generate mutable field access for each supported `#[slot(...)]` field shape.
- Add derive tests.

Out of scope:

- Enum derive support.
- Format-specific codec generation.
- Runtime mutation rewiring.

## Code Organization Reminders

- Keep the derive implementation small and parallel to existing immutable access generation.
- If helper functions grow, split them in `lpc-slot-macros/src/attr.rs` or a focused helper file.
- Put tests at the bottom of test files.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `lp-core/lpc-slot-macros/src/record.rs`
- `lp-core/lpc-slot-macros/src/attr.rs`
- `lp-core/lpc-model/tests/slot_record_derive.rs`

Current derive already emits:

```rust
impl SlotRecordAccess for #ident {
    fn field(&self, index: usize) -> Option<SlotDataAccess<'_>> { ... }
}
```

Add the mutable mirror:

```rust
impl SlotRecordMutAccess for #ident {
    fn field_mut(&mut self, index: usize) -> Option<SlotDataMutAccess<'_>> { ... }
}
```

Also add:

```rust
impl SlotMutAccess for #ident {
    fn data_mut(&mut self) -> SlotDataMutAccess<'_> {
        SlotDataMutAccess::Record(self)
    }
}
```

`attr.rs` should get a mutable counterpart to `field_access_tokens`, maybe:

```rust
field_mut_access_tokens(...)
```

For inferred fields, emit:

```rust
<#ty as FieldSlotMut>::slot_field_data_mut(&mut self.field)
```

or, if naming is cleaner after Phase 1:

```rust
self.field.slot_field_data_mut()
```

Tests:

- A derived record can expose field 0 as `SlotDataMutAccess::Value`.
- Mutating through the derived field dispatch changes the real field.
- Private fields are still rejected by the derive.

## Validate

```bash
cargo fmt -p lpc-model -p lpc-slot-macros
cargo test -p lpc-model --test slot_record_derive
cargo test -p lpc-model slot_mut
```
