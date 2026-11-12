# Slot Values

Slot values are the leaf data that LightPlayer can author, inspect, transport,
bind, and evaluate. They are the smallest meaningful values in the slot model:
numbers, booleans, strings, colors, vectors, matrices, resource references,
atomic enums, and dynamic `LpValue` payloads.

Values should stay part of the slot language. A value that appears in authored
storage, wire messages, binding endpoints, node parameters, or runtime state
should not need custom serialization outside the slot system.

## Goals

- Give leaves semantic names instead of flattening everything to primitives.
- Keep authored and wire syntax predictable.
- Preserve enough reflection for editors, validation, and bindings.
- Avoid duplicating value modeling in Serde attributes or custom parser code.
- Keep embedded representation compact enough for no-std runtime use.

## Primitive Values

Primitive slot values are the basic scalar shapes:

- `bool`
- `u32`
- `i32`
- `f32`
- `String`

These should serialize directly as JSON/TOML scalars where possible. They are
the building blocks for semantic values, but they should not erase semantics
when a domain concept is more specific than a raw scalar.

## Structured Values

Structured values include fixed-size numeric shapes such as dimensions,
vectors, colors, transforms, and matrices.

These should generally serialize as records with named fields when the names
carry meaning:

```toml
size = { width = 64, height = 64 }
```

Array-like syntax is appropriate when the position is the convention and the
value is naturally tuple-like:

```toml
white_point = [1.0, 0.94, 0.88]
```

The rule of thumb is simple: use names when names help readers avoid mistakes.
In the internal value model, those names belong to `LpType` or slot shape
metadata. `LpValue` should carry the compact payload data that is interpreted
through that type context.

## Semantic Leaves

Semantic leaves are domain values with behavior or validation beyond their
storage primitive. Examples include:

- color order
- ring order
- GLSL compile options
- resource references
- binding references
- path specs
- scalar hints

The core pattern is:

```rust
pub struct Ratio(pub f32);
pub type RatioSlot = ValueSlot<Ratio>;
```

`ValueSlot<T>` owns revision tracking and slot leaf access. `T: SlotValue` owns
the semantic value contract: conversion to/from `LpValue`, static value shape,
and editor metadata.

Simple semantic leaves should derive `SlotValue`. The derive uses the Rust type
name as the default shape id, so a `Ratio` value becomes the `Ratio` slot value
shape without a handwritten id. Manual `SlotValue` impls are still valid for
enums, references, products, and other leaves that need custom parsing.

A semantic leaf may store as a string, scalar, record, or enum, but the semantic
type should remain visible in the slot model.

## Dynamic Values

`LpValue` represents values whose exact leaf type may vary at runtime. It is
useful for bindings, controls, parameters, and future message payloads.

`LpValue` is LightPlayer's atomic value language, not a perfect GLSL value
mirror. Shader compatibility is a strong design pressure, but the slot system
also models node definitions and authored artifacts. Because definitions need
domain values that are not GLSL values, `LpValue` intentionally includes
modeling primitives such as strings, lists, resources, products, and atomic
enums.

The value/type split follows one rule:

> Names live in shapes and types. Values carry payloads.

For example, `LpType::Struct` owns field names while a struct `LpValue` should
eventually only need ordered field payloads. `LpType::Enum` owns variant names
while `LpValue::Enum` stores the active variant index. Authored JSON/TOML can
still use readable names; SlotCodec translates those names through the type.

Dynamic values should still be constrained by slot context when possible. A
binding endpoint that accepts a `Value(LpValue)` should know whether it expects
a scalar, color, texture reference, or some other semantic value from the slot
shape around it.

`LpType::Any` is the explicit escape hatch for dynamically typed definition-time
payloads. It should stay rare: use it when the value really is an `LpValue`
whose semantic constraints are supplied by a surrounding binding, node, or
validation step.

### Atomic Enum Values

`LpValue::Enum` is for enum-like choices that behave as one value:

- one `ValueSlot<T>` owns the revision
- the whole enum changes as a unit
- callers do not address fields inside the payload through slot paths
- the active variant is stored by index into `LpType::Enum`
- payload data is another `LpValue` or absent for unit variants

This is distinct from `SlotShape::Enum`, which is for addressable slot
structure. A quick test:

> If changing a subfield of the enum payload should be a separate slot mutation,
> use `SlotShape::Enum`. If the whole thing should change at once, use
> `LpValue::Enum`.

Examples that fit `LpValue::Enum`:

- binding endpoints such as `Unset`, `Ref`, and `Value`
- compact semantic choices such as color order, texture format, and shader
  option modes

Examples that should stay `SlotShape::Enum` through `EnumSlot<T>`:

- fixture mapping configs with editable variant fields
- path specs with nested payloads
- wrapper enums whose active variant exposes slot-addressable records

## Refs And Values

Binding-like endpoints often need a choice between a reference and an inline
value. When this choice is atomic, it should be represented as a semantic
`ValueSlot<BindingEndpoint>` backed by `LpValue::Enum`:

```rust
enum BindingEndpoint {
    Unset,
    Ref(BindingRef),
    Value(LpValue),
}
```

For authored TOML, a compact single-value enum form may be worth supporting when
explicitly enabled:

```toml
source = { ref = "bus:visual.out" }
target = { value = 1.0 }
```

This is a special storage policy, not a general enum rule. Lower-case `ref` and
`value` are acceptable here because they read as field names inside a compact
object rather than as type or variant names.

## Defaults

Default values should come from Rust `Default` or generated default instances
for the containing slot-modeled type/record. SlotCodec should not require a second
portable default blob for values.

Leaf defaults are still conceptually leaf-level: the generated reader can start
from a default container and replace only fields that appear in storage.

## Transient Values

If a value exists at runtime but should not be written to disk, model that
explicitly as transient. There is no generic `slot(skip)` path for derived slot
records; if a field is in a `SlotRecord`, it is part of the slot model.

A transient value may still be valid on a wire path if that path is explicitly
runtime-state oriented. Disk persistence and wire transport do not have to use
the same projection.

## Serialization Rules

- Unknown fields are errors until schema versioning exists.
- Primitive values serialize as scalars.
- Semantic leaves serialize through their `SlotValue` conversion.
- Dynamic values should use explicit type context or explicit discriminators.
- Compact single-value enum syntax is opt-in.
- Strings that encode references should belong to a semantic reference type,
  even if the string format changes later.

## Open Questions

- Which `LpValue` variants are required for the first production wire messages?
- Should all resource references share one string grammar, or should each
  reference kind have its own semantic leaf?
- How much value-level validation should happen during parsing versus later
  graph validation?
- Which semantic leaves should become generated from slot metadata first?
