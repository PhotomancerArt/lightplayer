# Slots Overview

Slots are LightPlayer's domain modeling system. They describe the authored,
runtime, and synchronized data that flows through the engine in a form that can
be reflected, edited, serialized, and connected.

The core idea is that LightPlayer should have one primary data language for its
own domain. Rust structs and enums hold the values, but slot metadata describes
how those values participate in the system.

## Goals

- Make the data model inspectable without hand-written per-type reflection.
- Let authored TOML, wire JSON, runtime views, and tooling share one domain
  shape.
- Keep embedded code size and RAM pressure under control.
- Preserve Rust-native modeling where it is useful: records, enums, defaults,
  constructors, and type-safe semantic leaves.
- Avoid duplicating the domain in unrelated metadata languages.

## Why "Slot"?

The word "slot" is intentional. The system is not only a document model, a
schema model, or a dynamic object model. It is about addressable places where
domain values can be inserted, edited, connected, synchronized, and observed.

That matters for LightPlayer because the same model is used across authored
project definitions, runtime state, wire messages, and dataflow bindings. A
slot is a place in the machine: it has a shape, it may hold a value or
container, it can be reached by a path, and tools can reason about what belongs
there.

Names like "document," "dynamic object," or "domain object" describe narrower
views of the same machinery. "Slot" is shorter, distinct in the codebase, and
keeps the dynamic/runtime nature of the system visible.

## Main Concepts

### Registered Shapes

`SlotShape` is the schema node for the slot system. Any slot-modeled Rust type
can have a stable `SlotShapeId` and be registered in a `SlotShapeRegistry`.

The registry is a catalog of shapes. It does not own runtime objects and does
not decide what is top-level in the app.

### Path Roots

A registered shape can be used as the root of a `SlotPath` traversal. In this
context, root means "start of this path," not "top-level synced object."

### Runtime Slot Objects

`SlotAccess` is the runtime trait for an object that exposes slot data for a
shape id. Engine, storage, and wire code can choose which objects are
addressable in a given context.

### Slot Records

A slot record is a structured object with named fields. It usually maps to a
Rust struct.

In Rust, a normal slot record is written as `#[derive(Slotted)]` with the
derive macro in scope. `Slotted` is the author-facing derive for structured
slot objects; internal record traits still use record terminology where they
describe the concrete shape. No type-level `#[slot]` marker is required.

Records are the common shape for authored configuration and runtime state:

- field names
- field types
- defaults
- optional values
- maps
- future transient projections

Every static slot record should also get a generated `*View`. Views are the
compiled path-access projection over the shape, so they are part of the core
slot surface rather than an opt-in root feature.

### Slot Wrappers

A slot wrapper is a single-field tuple struct derived with `Slotted`, such as
`struct Artifact(EnumSlot<NodeDef>);`. The wrapper has its own static shape id
and can be used as an ownership or loading boundary, but it exposes the wrapped
slot shape directly. Paths start at the wrapped shape, so a wrapper does not
introduce a synthetic `.0` field.

Wrappers are registered as static shapes. They do not currently generate their
own `*View`; view generation remains tied to named-field records until wrapper
view delegation has a real use case.

### Slot Enums

A slot enum is a closed set of variants. It usually maps to a Rust enum.

Normal serialized enums should use explicit discriminators. `#[derive(Slotted)]`
uses the Rust variant name as the slot discriminator by default, such as
`PathPoints`, `RingArray`, or `Texture`. `#[slot(name = "...")]` is an escape
hatch, not the normal style.

Use slot enums when the variant payload is slot-structured data: callers should
be able to address, mutate, sync, or validate fields inside the active variant
through slot paths. Variant selection is then part of the slot tree, with its
own revision boundary and a default payload for each variant.

Static structured enums should be stored through `EnumSlot<T>`. The raw Rust
enum implements `SlottedEnum` / `SlottedEnumMut` and exposes the active variant
data; `EnumSlot<T>` owns the active-variant revision. This keeps variant
selection first class without pretending a plain Rust enum field can carry its
own slot revision.

For structured enums, `Slotted` supports unit variants, one-field tuple wrapper
variants, and named-field record variants. Multi-field tuple variants should be
written as named variants so slot paths have field names. Enums with multiple
variants use Rust-style `#[default]` on the neutral/default variant; if no real
domain variant is an honest default, add an explicit sentinel such as `Unset`.

### Slot Values

Slot values are the leaf values that can be authored, displayed, transported, or
bound:

- numbers
- booleans
- strings
- colors
- vectors and matrices
- resource references
- atomic enum values
- `LpValue`-like dynamic values

Semantic leaves should stay semantic. A color, path, resource reference, or
binding endpoint should not be flattened into unrelated ad hoc strings unless
that string syntax is itself part of the slot value's design.

Use `LpValue::Enum` for atomic enum-like values inside a `ValueSlot`: the whole
choice changes as one leaf, and its payload is value-language data rather than
an addressable slot subtree. Authored syntax can use variant names, but the
in-memory value should store the active variant as an index into `LpType::Enum`.

See `values.md` for the detailed value model, including primitive values,
semantic leaves, dynamic `LpValue`, defaults, transient values, and compact
ref/value endpoint syntax.

### Slot Metadata

Slot metadata is the information needed to interpret a Rust domain type as a
LightPlayer data shape:

- shape ids and discriminators
- field names
- field types
- default behavior
- future transient storage/wire projections
- compact storage policies
- specialized semantic leaf handling

Metadata should describe the domain shape, not duplicate business logic.

## Design Rules

- Persisted domain concepts should be slot-modeled as records, enums,
  slot maps, slot options, or semantic slot leaves.
- Unknown serialized fields are errors until schema versioning exists.
- Defaults should come from Rust defaults or generated default instances rather
  than duplicated portable blobs.
- Prefer explicit discriminators over inferred or untagged formats.
- Derived slot records do not support `slot(skip)`. If a field is in a
  `SlotRecord`, it participates in the slot model; runtime-only state should use
  a wrapper or a future explicit transient projection.
- Keep casing close to the domain model. Type and variant names are PascalCase;
  compact field-like enum forms may opt into lower-case keys such as `ref` and
  `value`.

## Relationship To Serialization

Slots are the schema language. `SlotCodec` is the serialization projection of
that schema.

See `serialization.md` for the SlotCodec architecture, code size constraints,
and migration plan away from Serde in no-std core paths.
