# Phase 1: Slot Semantics

## Scope Of Phase

Add field-level slot semantics to `lpc-model` and teach `SlotRecord` derive to emit them.

In scope:

- Add `SlotDirection`.
- Add `SlotSemantics`.
- Add `semantics: SlotSemantics` to `SlotFieldShape`.
- Extend shape builders for default and explicit semantics.
- Extend `lpc-slot-macros` field attributes:
  - `#[slot(consumed)]`
  - `#[slot(produced)]`
  - `#[slot(merge = "by_key")]`
  - `#[slot(merge = "latest")]`
  - `#[slot(merge = "error")]`
- Update tests and rustdocs.

Out of scope:

- Required-slot validation.
- UI handling for semantics.
- Fluid model/runtime.

## Code Organization Reminders

- Prefer one concept per file:
  - `slot_direction.rs`
  - `slot_semantics.rs`
- Keep tests at the bottom of files.
- Do not bury new public concepts in `mod.rs`.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests.
- If blocked, stop and report.
- Report changed files, validation, and deviations.

## Implementation Details

Relevant files:

- `lp-core/lpc-model/src/slot/slot_shape.rs`
- `lp-core/lpc-model/src/slot/slot_shape_builder.rs`
- `lp-core/lpc-model/src/slot/mod.rs`
- `lp-core/lpc-slot-macros/src/lib.rs`
- `lp-core/lpc-slot-macros/src/attr.rs`
- `lp-core/lpc-slot-macros/src/record.rs`

Add:

```rust
pub enum SlotDirection {
    Local,
    Consumed,
    Produced,
}
```

Add:

```rust
pub struct SlotSemantics {
    pub direction: SlotDirection,
    pub merge: SlotMerge,
}
```

Defaults:

- `SlotDirection::Local`
- `SlotMerge::Latest`

Update `SlotFieldShape`:

```rust
pub struct SlotFieldShape {
    pub name: SlotName,
    pub shape: SlotShape,
    #[serde(default)]
    pub semantics: SlotSemantics,
}
```

Keep existing builder behavior:

```rust
field("size", shape)
```

should emit default/local semantics.

Add an explicit builder, for example:

```rust
field_with_semantics("emitters", shape, SlotSemantics::consumed(SlotMerge::ByKey))
```

Macro parsing:

- Existing field shape parsing should remain compatible.
- Multiple `#[slot(...)]` attributes should combine cleanly.
- `#[slot(consumed, merge = "by_key")]` should work.
- `#[slot(produced)]` should work.
- `merge` without `consumed` is allowed but still just stores semantics; later validation can decide if it is meaningful.

Update macro docs in `lpc-slot-macros/src/lib.rs` to describe semantics.

Add tests:

- Default field semantics are local/latest.
- Macro-emitted consumed/by_key field semantics are present in the generated shape.
- Serialization round-trips field semantics.

## Validate

```bash
cargo fmt --check
cargo test -p lpc-model slot_semantics
cargo test -p lpc-model slot_shape
cargo test -p lpc-slot-macros
```

