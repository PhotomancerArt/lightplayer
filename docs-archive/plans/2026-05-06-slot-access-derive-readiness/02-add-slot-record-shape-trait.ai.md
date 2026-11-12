# Phase 2: Add Slot Record Shape Trait

## Scope Of Phase

Add a non-root record-shape trait in `lpc-model` and manually prove it in the mockup before introducing proc macros.

In scope:

- Add `SlotRecordShape` or equivalent to `lpc-model`.
- Use it for at least two mockup inline/root records by hand.
- Keep behavior unchanged.
- Add tests demonstrating that a type can expose both `SlotRecordAccess` and generated-like record shape without root registration.

Out of scope:

- Proc macros.
- Enum derive.
- Real `lpc-source` / `lpc-engine` conversion.

## Code Organization Reminders

- Place the trait in its own file if that keeps `slot_access.rs` focused.
- Keep trait docs precise: this is static shape for an indexed record, not a runtime data snapshot.
- Keep tests at the bottom of files.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Likely trait:

```rust
pub trait SlotRecordShape {
    fn slot_record_shape() -> SlotShape;
}
```

Root `StaticSlotAccess` impls should be able to use:

```rust
registry.register_tree(Self::SHAPE_ID, <Self as SlotRecordShape>::slot_record_shape())
```

Good manual proof targets in the mockup:

- `ScalarHint` as an inline record.
- `OutputNode` or `TextureDef` as a root record.

Do not attempt to convert all records manually. This phase exists to establish the trait shape the derive will generate.

## Validate

```bash
cargo test -p lpc-model
cargo test -p lpc-slot-mockup
cargo check -p lpc-model --features schema-gen
git diff --check
```
