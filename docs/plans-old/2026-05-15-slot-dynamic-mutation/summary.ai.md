# Slot Dynamic Mutation Summary

## What was built

- Added mutable slot access traits in `lpc-model` for value leaves, records, maps, enums, options, and root objects.
- Added generic `set_slot_value`, `set_slot_variant_default`, and `slot_data_revision` helpers that walk slot shapes and mutable slot data together.
- Extended `#[derive(SlotRecord)]` to generate mutable field dispatch, map-value access, and root mutable access.
- Replaced mockup runtime's path-specific value mutation with the generic slot mutation path.
- Added explicit default-variant switching for mockup and model enum slots that need it today.
- Documented the default-and-mutate construction model in the slot serialization design notes.

## Decisions for future reference

#### Defaults Everywhere

- **Decision:** Slot-modeled values are default-constructible at the model layer.
- **Why:** Required fields make embedded-size-conscious deserialization and dynamic mutation much more complex. Logic can validate whether a defaulted model is renderable.
- **Rejected alternatives:** Required model fields during deserialization.
- **Revisit when:** Schema validation becomes a first-class layer.

#### Mutation Is Shape-Driven

- **Decision:** Runtime value mutation walks `SlotShapeRegistry` plus `SlotMutAccess` instead of hand-coded path dispatch.
- **Why:** This is the same foundation needed for default-and-mutate JSON/TOML deserialization.
- **Rejected alternatives:** Mockup-specific mutation routing and generated format-specific setters.

#### Enum Switching Is Explicit

- **Decision:** Mutating fields inside the active enum variant and switching variants are separate operations.
- **Why:** Deserialization can read a discriminator, switch to a default payload, then fill fields. Runtime mutation should not silently switch variants because a field name happened to match another variant.
- **Rejected alternatives:** Inferring variant changes from payload field paths.

#### Codegen Stays Narrow

- **Decision:** Codegen should emit Rust reflection bridges such as field dispatch, not format-specific serialization policy.
- **Why:** The custom codec should stay generic over slot shapes and avoid mockup-specific hard-coded logic.
- **Revisit when:** Generated enum helpers become large enough to threaten binary size.
