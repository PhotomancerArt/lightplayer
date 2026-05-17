# Slot-Native Streaming Serialization Roadmap

## Motivation

LightPlayer already has a domain modeling language: slot roots, records, maps,
enums, options, semantic values, shape ids, and registries. Serialization should
grow from that model instead of requiring the same domain to be expressed in
Serde's model as well.

The practical motivator is embedded size and memory. Generated Serde code is a
large part of firmware code size, and the embedded target cannot afford a
design that always requires `JSON bytes -> SlotData -> typed object` for large
messages or resources. The on-device GLSL JIT is the core product, so
serialization must not crowd out compiler code or memory.

The direction is to make slots the source of truth, while treating TOML and
JSON as syntax frontends.

For the durable slot design docs, see `docs/design/slots/overview.md` and
`docs/design/slots/serialization.md`.

## Architecture

The roadmap centers on a syntax stream plus a slot-aware reader/writer layer:

```text
TOML / JSON / future binary syntax
        |
        v
shape-agnostic syntax events
        |
        v
slot-aware reader/writer + SlotShapeRegistry
        |
        v
generated typed construction / borrowed slot access / SlotData reference path
```

The source stream knows only syntax:

```text
start_object
prop(key)
end_object
start_array
end_array
string_chunk(...)
number(...)
bool(...)
null
```

Generated type code knows the target shape:

```rust
Self {
    brightness: reader.prop("brightness")?.f32()?,
    mapping: reader.prop("mapping")?.slot_root("Mapping")?,
}
```

`SlotData` remains useful as a reference representation, test oracle, host
tooling shape, and sync-state structure. It should not be the only production
construction path.

TOML can parse into `toml::Value` first and adapt that tree into the same reader
semantics, because authored TOML is usually small and TOML is awkward to stream
faithfully. JSON should prove a direct parser-to-reader path because wire
messages may be larger and may carry resources.

Writers should mirror readers. A slot-native output stream should replace or
absorb the existing ad hoc JSON stream so borrowed slot data, generated typed
writers, and object round trips share one storage model.

## Design Rules

- Persisted domain concepts should be slot roots, slot records, slot enums,
  slot maps, slot options, or semantic slot leaves.
- SlotCodec is allowed to be opinionated and slot-only. General-purpose Rust
  serialization is explicitly out of scope.
- Type-specific codec branches are bugs waiting to happen.
- Codec logic can know slot shapes and storage metadata; it should not know
  `BindingDef`, `OutputDef`, or other concrete domain names.
- Unknown fields are errors until schema versioning is formalized.
- Default values come from Rust `Default` / generated default instances, not
  duplicated portable shape blobs.
- Authored storage uses a universal elision rule first: omit `None` and empty
  maps, but do not elide scalar defaults yet.
- `#[slot(skip)]` should disappear from persisted domain modeling. Use explicit
  concepts such as `#[slot(transient)]` or wrapper/envelope fields.
- Normal tagged enums use explicit discriminators with PascalCase variant
  names.
- Compact single-value enum storage may be explicitly enabled for cases such
  as `{ ref = "..." }` and `{ value = 123 }`.
- Generated code size and embedded binary size should be measured before and
  after major adoption steps.

## Alternatives Considered

### SlotData As The Main Decode Target

This is simple and generic, and it is already partially implemented for TOML.
It remains valuable for tests and tooling. It is not sufficient as the embedded
runtime story because large JSON/resource messages can force avoidable
temporary allocations.

### Serde As The Long-Term Domain Codec

Serde is convenient and mature, but generated per-type code is exactly the cost
we are trying to reduce. It also keeps LightPlayer's domain model split across
two languages: slots and Serde attributes.

### Shape-Aware Syntax Parsers

A parser that directly knows the target `SlotShape` can be very efficient, but
it couples syntax parsing to domain policy too early. The preferred split is a
syntax-only event source with a slot-aware reader layered above it.

### Production-First Migration

Jumping directly into production loading and messages would create too much
coupled churn. The mockup is the right proving ground because it can mirror the
real domain while staying small enough to reshape.

## Risks

- The reader/writer abstraction may become too clever before one real slice
  validates it.
- Generated construction may duplicate too much logic from the `SlotData`
  decoder unless both paths share reader semantics.
- TOML's table model may pressure the reader API in different ways than JSON.
- Enum wrapper modeling, especially `NodeDef` and future inline node defs, may
  need more than one iteration.
- Removing Serde from core crates may uncover host tooling or schema-generation
  dependencies that need their own replacement story.

## Scope Estimate

This is a multi-milestone architecture change. The first two milestones should
stay experimental and mockup-heavy. Production adoption should begin only after
manual and generated reader/writer tests prove the shape.
