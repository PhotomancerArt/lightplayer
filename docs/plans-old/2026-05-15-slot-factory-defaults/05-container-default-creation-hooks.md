# 05 - Container Default Creation Hooks

## Scope of phase

Add explicit generic creation hooks for maps, options, and dynamic enums so
default-and-mutate deserialization can create missing container payloads.

In scope:

- Map insertion of default values through `MapSlotMutAccess`.
- Option `none -> some(default)` through `SlotOptionMutAccess`.
- Dynamic `SlotEnum` default variant switching using registry/shape.
- Tests proving these are explicit operations.

Out of scope:

- Changing `set_slot_value` to auto-create missing map keys.
- Adding wire/runtime client insert/remove operations.
- Full codec reader replacement.

## Code organization reminders

- Keep mutation policy explicit.
- Prefer separate helper functions for creation operations rather than changing
  `set_slot_value` semantics.
- Put shared traversal code in `slot_mutation.rs` only when it truly belongs to
  path-driven mutation.

## Sub-agent reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation details

Relevant files:

- `lp-core/lpc-model/src/slot/slot_mut_access.rs`
- `lp-core/lpc-model/src/slot/slot_mutation.rs`
- `lp-core/lpc-model/src/slot/value_slot.rs`
- `lp-core/lpc-model/src/slot/slot_data.rs`
- `lp-core/lpc-model/src/slot/slot_factory.rs`

Add explicit methods such as:

```rust
fn insert_default(
    &mut self,
    revision: Revision,
    key: &SlotMapKey,
    registry: &SlotShapeRegistry,
    value_shape: &SlotShape,
) -> Result<(), SlotMutationError>;
```

Typed map implementation:

```rust
impl<K, V> MapSlotMutAccess for MapSlot<K, V>
where
    K: MapSlotKeyLike,
    V: Default + SlotMapValueMutAccess,
```

Dynamic map implementation builds `SlotData` from the provided value shape.

Option implementation:

- typed `OptionSlot<T>` uses `T::default()`.
- dynamic `SlotOptionDyn` builds `SlotData` from the provided `some` shape.

Enum implementation:

- static typed enums keep using `SlotEnumDefaultVariant`.
- dynamic `SlotEnum` should be able to set a requested variant by using the
  matching variant shape and dynamic data builder.

Add public helper functions if useful:

```rust
insert_slot_map_entry_default(root, registry, path, revision, key)
set_slot_option_some_default(root, registry, path, revision)
```

Keep `set_slot_value` conservative. Missing map keys should still be rejected
unless the caller explicitly inserted them first.

Tests:

- typed map insert default then set leaf.
- dynamic map insert default then set leaf.
- option none -> some default then set leaf.
- dynamic enum switches variant and exposes default payload.
- `set_slot_value` alone still rejects missing map keys.

## Validate

```bash
cargo fmt -p lpc-model --check
cargo test -p lpc-model slot_mutation
cargo test -p lpc-model slot_mut_access
cargo test -p lpc-model slot_factory
```
