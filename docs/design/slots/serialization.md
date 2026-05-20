# SlotCodec Serialization

`SlotCodec` is LightPlayer's slot-native serialization system. It exists so the
slot model can be the source of truth for slot-authored persisted data and
slot-shaped wire payloads, instead of requiring every domain concept to be
expressed in both slots and Serde.

The design is intentionally opinionated. This is not a general-purpose Rust
serialization framework. It only needs to serve the shapes LightPlayer actually
persists and sends:

- slot-modeled types
- slot records
- slot enums
- slot maps
- slot options
- slot values and known semantic leaves
- atomic enum values in `LpValue`
- explicit storage metadata such as discriminators, defaults, and transient
  fields

The system may eventually be usable outside slots, but that is not a design
goal. Generality should be treated as accidental upside, not a requirement.

## Rationale

LightPlayer already has a domain modeling language. Slots describe the data
that can flow through the system, the shapes tools can inspect, and the values
that can be authored or synchronized. Serialization should be another projection
of that model.

The immediate production pressure is embedded size. Generated per-domain-model
Serde code can become a large part of firmware code size, and the ESP32-C6
target must keep enough flash and RAM available for the on-device GLSL JIT
compiler. Serialization is supporting infrastructure; it must not crowd out the
compiler, shader runtime, or resource buffers.

The secondary pressure is conceptual simplicity. In LightPlayer's domain, data
modeling should mean "define the slot shape." It should not also require a
parallel Serde language for firmware-authored domain loading.

Serde is still useful infrastructure. It may remain in `lpc-model`, `lpc-wire`,
tests, host tooling, and small protocol envelopes when measurement shows the
cost is flat. The design target is not "no Serde anywhere"; the target is "no
large duplicated per-domain-model serialization path on firmware."

## Serde Boundary

Use SlotCodec for:

- authored project/node definition loading on firmware
- slot-shaped data payloads
- `SlotData`/`SlotShape` paths when bloat measurements show Serde is too costly
- future binary or compact slot-native formats

Serde is acceptable for:

- small client/server message envelopes
- protocol shells whose fields are not full slot-authored domain trees
- tests and host-side tooling
- schema/debugging helpers when they do not affect firmware size materially

The current measured posture is documented in
`docs/reports/2026-05-17-slotcodec-bloat-check.md`: after switching authored
domain loading to SlotCodec, `lpc_model` and `lpc_wire` firmware text size
dropped while `serde_core` remained roughly flat around 22 KiB.

## Serde Influence

`SlotCodec` should copy Serde's durable ideas where they fit:

- separate format parsing/writing from type construction
- generate shape, access, and factory code from derive-time metadata
- make defaults, tags, field lists, and future transient projections explicit
- keep format frontends small and reusable
- provide friendly errors with path/span context where possible

It should not copy Serde's full generic data model. The supported type universe
is much smaller, and that is an advantage. A smaller model should let us use
fewer generic entry points, fewer generated branches, and more shared helpers.

## Runtime Shape

The architecture has three layers:

```text
JSON / TOML / future binary syntax
        |
        v
shape-agnostic syntax events
        |
        v
slot-aware reader/writer helpers
        |
        v
SlotShapeRegistry + SlotAccess / SlotMutAccess
```

Syntax sources do not know target slot shapes. They emit objects, properties,
arrays, scalars, nulls, and string chunks. JSON should prove the direct
streaming path because wire messages can be large. TOML may be tree-backed
initially because authored TOML is usually small and TOML's table model is
awkward to stream.

The slot-aware layer knows the target shape. It can either mutate a
caller-provided object or ask the registry to create a default object for a
shape id. The preferred generic load path is:

```rust
let mut object = registry.create_default(shape_id)?;
apply_reader_to_slot(object.data_mut(), shape, &registry, reader.value())?;
let project = object.downcast::<ProjectDef>()?;
```

Codegen should support that path by generating shape and access machinery, not
format-specific parse bodies. In other words, generated code should answer
questions like "what fields does this record have?", "which variant is active?",
and "how do I mutate this field?", while shared SlotCodec helpers handle JSON,
TOML, maps, options, values, discriminators, and error reporting.

`#[derive(Slotted)]` is the primary source of this machinery for structured
types. It covers named-field records, one-field tuple wrappers, and structured
enums. For enums, derived slot metadata uses Rust variant names as authored
discriminators by default and Rust-style `#[default]` to choose the default
variant. This keeps discriminator lists attached to the model instead of
duplicated in hand-written serializer code.

Slot enums can opt into alternate authored encodings. The default remains
tagged `kind = "Variant"` syntax; externally tagged enums use a single property
named after the active variant. See
[`enum-encoding.md`](./enum-encoding.md) for the supported encodings and derive
attributes.

That code is allowed to be opinionated. Unknown fields are errors until schema
versioning exists. Discriminators are explicit. Field casing should match the
slot/domain model unless a specific compact syntax is explicitly enabled.

## Default-And-Mutate Construction

Slot-modeled data should be default-constructible at the model layer. Required
fields are a logic-layer concern: a model can be loaded, synced, edited, and
serialized even when application logic later decides it is not renderable or
otherwise invalid.

Defaults are intentionally empty sentinel data, not domain validity. Empty
strings, empty maps, absent options, zero numeric ids, and zero scalar values
are all acceptable model-layer defaults. A default object should have the right
slot shape and be safe to mutate, but it does not need to be meaningful to the
engine. Recursive validation is the layer that should reject empty references,
missing artifacts, invalid ids, or incomplete runtime objects.

Enums that do not have an honest semantic default should use an explicit
`Unset` variant. This keeps the sentinel visible in Rust and serialized data
instead of pretending that a real domain variant, such as a resource family or
binding endpoint, was authored.

The preferred construction path is:

1. Construct `T::default()`.
2. Use generated mutable slot access as the Rust reflection bridge.
3. Apply parsed fields through generic slot mutation helpers.

This keeps generated deserializers small. Codegen should provide field and enum
variant access, not format-specific parsing bodies.

Enums use the same rule. Deserializing an enum is two-phase:

1. Read the explicit discriminator.
2. Switch to that variant with default payload.
3. Mutate the now-active variant payload from the remaining fields.

Static structured slot enums are stored through `EnumSlot<T>`. The raw Rust
enum `T` exposes revision-free variant data through `SlottedEnum` and
`SlottedEnumMut`; `EnumSlot<T>` owns the active-variant revision. This keeps the
same field pattern as other slotted record fields: records contain slot
containers such as `ValueSlot<T>`, `MapSlot<K, V>`, `OptionSlot<T>`, and
`EnumSlot<T>`, while the wrapped semantic/domain value stays focused on domain
data.

Runtime field mutation should not silently switch enum variants. Variant
switching is an explicit operation. A convenience helper may later switch a
variant and then apply payload data in one call, but it should still be built
from the same default-switch plus field-mutation steps.

Slot-level enums are not the only enum-like concept in the system. `LpValue`
also supports atomic enum values for semantic leaves whose whole choice changes
as one value. SlotCodec should use the slot shape to decide the boundary:

- `LpValue::Enum` is a leaf payload inside `ValueSlot<T>`.
- `SlotShape::Enum` is an addressable slot subtree with a variant revision and
  mutable payload fields.

SlotCodec should preserve readable authored syntax while keeping the in-memory
value compact. Names live in `LpType` and slot metadata; `LpValue` carries
payloads. For enum values, authored TOML/JSON can say `kind = "Value"`, but the
stored `LpValue::Enum` should use the variant index from `LpType::Enum`.

This distinction keeps small definition-time choices compact without turning
editable structured variants, such as fixture mapping configs, into opaque
values.

## Metadata Shape

The code generator may build a compact intermediate model before rendering
Rust:

```rust
struct SlotCodecModule {
    types: Vec<SlotCodecType>,
}

struct SlotCodecType {
    rust_type: &'static str,
    kind: &'static str,
    fields: Vec<SlotCodecField>,
    constructor: SlotCodecConstructor,
}
```

This model is build-time metadata, not a runtime value tree. It exists to keep
the generator simple and to make generated shape/access code uniform.

The long-term source of this metadata should be slot declarations and slot
attributes. Temporary explicit hook tables should be removed as the generic
shape/access path matures.

## Code Size Discipline

Binary size is a first-class acceptance criterion, not a cleanup task at the
end. The generator should prefer shared helper calls over large per-type
specialized bodies.

Guidelines:

- Generate field/variant access and small match loops, not full bespoke parsers
  for every type.
- Keep common leaf/map/array behavior in shared non-generic helpers where
  possible.
- Avoid adding type parameters to generated functions unless they buy real
  reuse.
- Prefer runtime helper dispatch over monomorphized helper families when the
  hot path is not performance critical.
- Keep specialized compact forms opt-in so they do not multiply every enum's
  generated surface.
- Track generated Rust size and firmware binary size before and after major
  SlotCodec changes.
- Re-run the firmware bloat check before removing useful Serde surfaces or
  before adding new serde-derived domain loading paths.

## Minimize The Monomorphs Pass

After the mockup is fully generated and before production adoption, run a
focused size pass:

1. Record baseline generated source size for the mockup codec.
2. Record host binary/test size where useful.
3. Record embedded firmware size for a representative build once production
   adoption begins.
4. Identify generic helper functions that monomorphize across many types or
   value types.
5. Convert high-fanout helpers to shared concrete helpers where that reduces
   binary size without making the API clumsy.
6. Re-run the same measurements and document the delta.

The goal is not to make generated Rust source tiny for its own sake. The goal is
to reduce final firmware size while keeping the generated code understandable
enough to debug.

## Default Object Factories

Generic deserialization needs a way to turn a shape id into a mutable slot
object. The registry owns that creation behavior:

```rust
registry.create_default(shape_id) -> Box<dyn SlotMutAccess>
```

Static shapes register factories that call their Rust `Default`
implementation. Dynamic shapes can opt into a factory that builds a
`DynamicSlotObject` from `SlotData`. Shapes that are not meaningful standalone
objects register an explicit unsupported factory.

This is also the opt-in boundary for generic loading. A reader may mutate a
caller-provided object, or it may ask the registry to create one. If creation is
unsupported, deserialization fails at that shape boundary instead of inventing a
partial object.

## Conceptual Boundaries

`SlotCodec` should support:

- default object construction from shape ids through the registry
- direct mutation of caller-provided slot objects from syntax streams
- direct JSON writing from typed objects
- TOML loading through the same semantic reader API
- `SlotData` as a reference/tooling path
- generated shape/access/factory machinery for slot-modeled types and slot
  enums
- explicit errors for unknown fields, invalid discriminators, and unsupported
  syntax

`SlotCodec` should not become:

- a second fully generic Serde
- a runtime reflection interpreter for arbitrary Rust data outside the slot
  model
- a mandatory `JSON -> SlotData -> object` pipeline
- a format parser that knows concrete domain types
- generated per-type format parsers unless a specific compact leaf syntax needs
  a small custom handler
- a reason to weaken the slot model with values that are skipped but not
  modeled

## Things To Revisit Before Production Adoption

- Whether `kind = "TextureDef"` is enough for all top-level discriminators, or
  whether some contexts need full slot shape ids such as `lp::TextureDef`.
- How wrapper enums such as future `NodeDef` variants should appear in slot
  metadata.
- Whether compact single-value enum syntax such as `{ ref = "..." }` and
  `{ value = 123 }` is worth supporting in generated code.
- How much path/span tracking is required for authored TOML errors.
- Whether any remaining Serde serialization of `SlotData`, `SlotShape`, or
  `LpValue` is large enough to justify targeted custom codecs.
