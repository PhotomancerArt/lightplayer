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
  constructors, and private fields.
- Avoid duplicating the domain in unrelated metadata languages.

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

In Rust, a normal slot record is written as `#[derive(SlotRecord)]` with the
derive macro in scope. No type-level `#[slot]` marker is required.

Records are the common shape for authored configuration and runtime state:

- field names
- field types
- defaults
- optional values
- maps
- transient fields

Every static slot record should also get a generated `*View`. Views are the
compiled path-access projection over the shape, so they are part of the core
slot surface rather than an opt-in root feature.

### Slot Enums

A slot enum is a closed set of variants. It usually maps to a Rust enum.

Normal serialized enums should use explicit discriminators. Variant names should
remain close to the Rust/domain names unless a specific compact syntax is
enabled for that enum.

### Slot Values

Slot values are the leaf values that can be authored, displayed, transported, or
bound:

- numbers
- booleans
- strings
- colors
- vectors and matrices
- resource references
- `LpValue`-like dynamic values

Semantic leaves should stay semantic. A color, path, resource reference, or
binding endpoint should not be flattened into unrelated ad hoc strings unless
that string syntax is itself part of the slot value's design.

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
- transient fields
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
