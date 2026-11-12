# Slot Dynamic Mutation Notes

## Scope

Build the generic mutation layer that the slot system is missing.

This plan is intentionally tight to mutation:

- Add mutable slot access traits beside the current read-only `SlotAccess` tree.
- Generate or derive only the tiny Rust reflection bridge that Rust cannot provide: field/variant/key access by shape index/name.
- Route runtime slot mutation through the generic layer instead of hand-coded target dispatch.
- Prepare the layer for default-and-mutate deserialization without making stream deserialization itself part of this plan.

Out of scope:

- Rewriting JSON/TOML parsing.
- Removing all generated codec code.
- Full map insertion/removal semantics beyond the minimal useful vertical slice.
- Full enum variant switching as the first operation, unless the vertical slice proves it is required immediately.

## Current State

The read side has a clear shape:

- `lp-core/lpc-model/src/slot/slot_access.rs`
  - `SlotAccess` exposes `shape_id()` and read-only `data()`.
  - `SlotDataAccess<'a>` is read-only and can expose value, record, map, enum, and option nodes.
  - `SlotRecordAccess::field(index)` is the important record reflection hook.
  - `SlotEnumAccess` exposes active variant name and read-only variant data.
- `lp-core/lpc-model/src/slot/slot_accessor.rs`
  - `SlotAccessor` compiles a `SlotPath` against `SlotShapeRegistry`.
  - It resolves record field names to indexes and walks read-only `SlotDataAccess`.
  - It does not mutate.
- `lp-core/lpc-slot-macros/src/record.rs`
  - `#[derive(SlotRecord)]` already generates immutable `SlotRecordAccess::field(index)`.
  - Fields must be public, which is good for keeping slot models simple.
- `lp-core/lpc-model/src/slot/value_slot.rs`
  - `ValueSlot<T>` already has `set_with_version`.
  - `MapSlot` and `OptionSlot` already have revision-aware mutation methods.

The current mockup runtime mutation is not generic:

- `lp-core/lpc-slot-mockup/src/engine/runtime.rs`
  - It validates shape/data revisions.
  - Then it pattern-matches exact string paths like `params.exposure` and calls custom setters.
  - This proves the protocol, but not the generic slot mutation model.

The current generated codec read path also uses per-record code:

- `lp-core/lpc-slot-codegen/src/render/slot_codecs.rs`
  - Generated code creates `Default::default()` and assigns fields by matching property names.
  - This should become generic over mutable slot access once the mutation layer exists.

## User Direction

- Dynamic mutation was always intended to be the foundation for serialization/deserialization.
- Try the default-and-mutate model first because binary size is a leading constraint.
- Generated code should be primitive/reflection-focused, not format-specific.
- Codegen should not know JSON/TOML/streams.
- Keep the model simple and explicit. If a type is complex, it can provide a custom impl.
- Think carefully about enums because they are the trickiest mutation case.

## Suggested Answers

### Should all slot-modeled values have defaults?

Answer: yes.

At the model layer, all slot-modeled values should have defaults. Required
values make serialization, partial mutation, and embedded-size-conscious codegen
much more complex. The model can always be constructed, loaded, and mutated.
The logic layer is responsible for saying whether a model is currently valid,
renderable, connectable, or complete enough for a given operation.

### Should mutation accept stream readers or actual values?

Suggested answer: actual values first.

The mutation layer should operate on semantic slot operations, especially `LpValue` leaves. Stream readers belong above this layer. A deserializer can parse a field value, convert it to a semantic mutation, then call the same mutation engine used by runtime client edits.

### What is the minimal generated/derived code?

Suggested answer: mutable field dispatch only.

For records, `#[derive(SlotRecord)]` should generate the mutable counterpart of existing immutable dispatch:

```rust
impl SlotRecordMutAccess for FixtureDef {
    fn field_mut(&mut self, index: usize) -> Option<SlotDataMutAccess<'_>> {
        match index {
            0 => Some(self.name.slot_field_data_mut()),
            1 => Some(self.mapping.slot_field_data_mut()),
            _ => None,
        }
    }
}
```

### What should the first mutation operation support?

Suggested answer: set value leaf by compiled slot path.

This proves the core architecture without taking on every container mutation immediately:

```rust
apply_slot_value_mutation(
    root: &mut dyn SlotMutAccess,
    registry: &SlotShapeRegistry,
    path: &SlotPath,
    value: LpValue,
    revision: Revision,
)
```

### How should enums work initially?

Answer: support both explicit default variant switching and active-payload mutation.

For an enum field, there are two different operations:

1. Mutating a field inside the current variant payload.
2. Switching the active variant to another variant with default payload.

Those should not be hidden behind one accidental behavior. Field mutation should
walk through the active variant only when the path and shape agree with that
variant. Switching variants should be explicit and should construct that variant
with defaults:

```rust
enum_slot.set_default_variant(revision, "path_points")?;
generic_set_fields_inside_active_variant(enum_slot, ...)?;
```

This is also the right shape for JSON/TOML deserialization: read the
discriminator, switch to that default variant, then use generic mutation to fill
the fields that are present.

The convenience form `set_variant_from_slot_data` can come later, but it should
be built from the two primitives rather than replacing them.

### Should generic mutation create missing map keys or option payloads?

Suggested answer: not in the first phase.

Mutating existing leaves is enough for the vertical slice. Inserting map keys, removing map keys, and creating `OptionSlot::some(Default::default())` are important, but should be added deliberately after the leaf path is stable.

## Open Questions

None blocking for the first plan. The enum policy above is a proposed direction, not a blocker: implement active-variant mutation first, then add explicit variant switching later.
