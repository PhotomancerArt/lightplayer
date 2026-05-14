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

### Slot Roots

A slot root is a top-level persisted or synchronized domain object. Project
definitions, output definitions, texture definitions, fixture definitions, and
shader definitions are examples.

Persisted domain objects should generally be slot roots. If something is loaded
from disk or sent as a meaningful wire object, it should have a slot identity
rather than living as custom serialization glue outside the slot model.

### Slot Records

A slot record is a structured object with named fields. It usually maps to a
Rust struct.

Records are the common shape for authored configuration and runtime state:

- field names
- field types
- defaults
- optional values
- maps
- transient fields

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

- root ids and discriminators
- field names
- field types
- default behavior
- transient fields
- compact storage policies
- specialized semantic leaf handling

Metadata should describe the domain shape, not duplicate business logic.

## Design Rules

- Persisted domain concepts should be slot roots, slot records, slot enums,
  slot maps, slot options, or semantic slot leaves.
- Unknown serialized fields are errors until schema versioning exists.
- Defaults should come from Rust defaults or generated default instances rather
  than duplicated portable blobs.
- Prefer explicit discriminators over inferred or untagged formats.
- Avoid `slot(skip)` for persisted fields. If a value exists at runtime but is
  not persisted, model that honestly as transient or through a wrapper.
- Keep casing close to the domain model. Type and variant names are PascalCase;
  compact field-like enum forms may opt into lower-case keys such as `ref` and
  `value`.

## Relationship To Serialization

Slots are the schema language. `SlotCodec` is the serialization projection of
that schema.

See `serialization.md` for the SlotCodec architecture, code size constraints,
and migration plan away from Serde in no-std core paths.
