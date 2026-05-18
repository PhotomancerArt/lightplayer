# Phase 3: Generic Set-Value Mutation

## Scope Of Phase

Add the first generic mutation operation: set one existing value leaf by slot path.

In scope:

- Add `slot_mutation.rs` to `lpc-model`.
- Walk a path against shape metadata and mutable slot access.
- Support record fields, existing map keys, option `some`, and active enum payload traversal.
- Set only value leaves from `LpValue`.
- Add explicit enum default-variant switching as a separate operation.
- Return clear errors.

Out of scope:

- Map insertion/removal.
- Option `none -> some` construction.
- Wire protocol changes.

## Code Organization Reminders

- Keep path walking generic and shape-driven.
- Keep error variants explicit and testable.
- Avoid format-specific code.
- Put tests at the bottom of the file.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `lp-core/lpc-model/src/slot/slot_accessor.rs`
- `lp-core/lpc-model/src/slot/slot_lookup.rs`
- `lp-core/lpc-model/src/slot/slot_mut_access.rs`
- `lp-core/lpc-model/src/slot/slot_mutation.rs`
- `lp-core/lpc-model/src/slot/slot_shape.rs`

Add:

```rust
pub fn set_slot_value(
    root: &mut dyn SlotMutAccess,
    registry: &SlotShapeRegistry,
    path: &SlotPath,
    revision: Revision,
    value: LpValue,
) -> Result<(), SlotMutationError>
```

Also add:

```rust
pub fn set_slot_variant_default(
    root: &mut dyn SlotMutAccess,
    registry: &SlotShapeRegistry,
    path: &SlotPath,
    revision: Revision,
    variant: &str,
) -> Result<(), SlotMutationError>
```

Implementation requirements:

- Verify root shape id exists.
- Resolve `SlotShape::Ref` while walking.
- For record field segments, use shape metadata to resolve field index, then `SlotRecordMutAccess::field_mut(index)`.
- For map key segments, validate key domain and use `MapSlotMutAccess::get_mut`.
- For option `.some`, use `SlotOptionMutAccess::data_mut`.
- For enum shape:
  - Use active variant name from `SlotEnumMutAccess`.
  - Find matching variant shape.
  - Continue walking through active variant payload.
  - If the requested field belongs to another variant, return an active-variant/path error instead of switching variants.
- Require final shape to be `SlotShape::Value`.
- Call `SlotValueMutAccess::set_lp_value`.

Suggested error variants:

- `UnknownRoot`
- `UnknownPath`
- `WrongType`
- `StaleShape` if a compiled accessor is reused later
- `UnsupportedTarget`
- `InactiveEnumVariant { active: String }`

Tests:

- Set a simple record value leaf.
- Reject wrong `LpValue` type.
- Set a nested record value leaf.
- Set an existing map entry leaf.
- Set an option `some` leaf and reject `none`.
- Set a field inside the active enum variant.
- Reject a field that belongs to a different enum variant.
- Switch an enum slot to a default variant, then set a field inside it.

## Validate

```bash
cargo fmt -p lpc-model
cargo test -p lpc-model slot_mutation
```
