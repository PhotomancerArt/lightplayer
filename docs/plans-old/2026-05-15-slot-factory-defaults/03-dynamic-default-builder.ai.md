# 03 - Dynamic Default Builder

## Scope of phase

Implement dynamic default object construction from shape metadata.

In scope:

- `SlotFactory::dynamic()`.
- `SlotFactory::unsupported(...)` or equivalent explicit unsupported creation.
- Recursive dynamic `SlotData` builder.
- Default `LpValue` creation from `LpType`.
- Tests for record/map/option/enum/ref/value defaults.

Out of scope:

- Static typed factories.
- Map insertion mutation policy.
- Full codec integration.

## Code organization reminders

- Keep recursive builder helpers private unless another module genuinely needs
  them.
- Keep `LpType` default logic discoverable near the factory code.
- Tests should exercise public `registry.create_default`, not private helpers,
  where possible.

## Sub-agent reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation details

Relevant files:

- `lp-core/lpc-model/src/slot/slot_factory.rs`
- `lp-core/lpc-model/src/slot/slot_shape.rs`
- `lp-core/lpc-model/src/value/lp_type.rs`
- `lp-core/lpc-model/src/value/lp_value.rs`

Dynamic rules:

- `Ref`: resolve the referenced id through the registry.
- `Unit`: `SlotData::Unit { revision: current_revision() }`
- `Value`: `SlotData::Value(WithRevision::new(current_revision(), default_lp_value(&shape.ty)))`
- `Record`: build all fields in order.
- `Map`: empty map with current revision.
- `Option`: none with current revision.
- `Enum`: first variant with default payload and current revision.

Empty enum shapes should return `SlotFactoryError::EmptyEnum(id)`.

`default_lp_value` should cover every `LpType` variant. For struct values,
construct an `LpValue::Struct` with each field's default value. For fixed arrays
and lists, construct the obvious empty/fixed default. If `LpType::List` has no
fixed length, default to an empty list.

Tests:

- simple value shape defaults to expected `LpValue`.
- record shape creates a `DynamicSlotObject` whose data is a record with the
  right field count.
- map defaults to empty.
- option defaults to none.
- enum defaults to first variant.
- ref resolves and builds referenced shape.
- missing ref returns a clear error.
- unsupported factory returns a clear error and is distinguishable from missing
  shape.

## Validate

```bash
cargo fmt -p lpc-model --check
cargo test -p lpc-model slot_factory
cargo test -p lpc-model slot_shape_registry
```
