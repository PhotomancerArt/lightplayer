# Slot Reflection Serialization Roadmap

## Motivation

The embedded firmware is now paying a large code-size cost for broad
Serde-generated TOML and JSON conversion. The size evidence in `notes.md`
points at typed deserialization for model/source types as a major contributor:
`lpc_model`, `serde_core`, `toml`, `toml_parser`, `serde_json`, and
`ser_write_json` together represent a meaningful chunk of firmware `.text`.

This hurts because the compiler is the product. Serialization should not crowd
out the on-device GLSL JIT.

The second problem is conceptual. LightPlayer already has a domain modeling
language: slots, values, shapes, map keys, enum variants, option presence,
metadata, revisions, and root registries. Keeping that model while also
expressing the same domain in Serde's data model creates duplicate sources of
truth.

The proposal is to grow the existing slot reflection system into the canonical
serialization engine for authored data and selected wire data, but not by
cutting directly through the production source/engine model first. The first
work should happen in the slot mockup, which already exists as a small replica
of the real domain. Production code should receive only reusable codec pieces
until the mockup proves the model.

Third-party syntax parsers are still welcome. The important shift is that
format syntax parsing and domain conversion become separate layers, and the
domain conversion is driven by `SlotShape`/`SlotDataAccess` rather than by
per-Rust-type `Serialize`/`Deserialize` derives.

## Current Evidence

Relevant existing pieces:

- `lpc-model::slot` has `SlotShape`, `SlotData`, `SlotDataAccess`,
  `StaticSlotAccess`, `SlotRecordShape`, `SlotShapeRegistry`, `ValueSlot`,
  `MapSlot`, enum/option access traits, and semantic leaf metadata.
- `lpc-slot-macros::SlotRecord` already generates shape and access code for
  Rust-authored records.
- `lpc-wire::slot::slot_data_json` already writes borrowed slot data as JSON by
  walking shape plus `SlotDataAccess`, avoiding an owned `SlotData` tree.
- `lpc-wire::slot::slot_shape_registry_json` already writes registry snapshots
  directly.
- Recent project-read streaming work already moved toward direct writers to
  avoid building large semantic response objects before writing JSON.
- The slot-domain roadmap already made the domain model the intended source of
  truth for shape, metadata, versioning, sync, and future mutation.

The missing piece is a native slot codec story that covers both directions:
authored disk storage, wire storage, direct/streaming writers, and the limited
places where Serde remains acceptable. The slot mockup should become the lab
where those choices are made visible.

## Core Idea

Split serialization into three layers:

```text
TOML / JSON / future binary syntax
        |
        v
small syntax adapter or event stream
        |
        v
slot reflection codec
        |
        v
SlotData / typed SlotAccess value / mutation target
```

The syntax layer only knows about format tokens and basic scalar values. The
slot reflection codec knows the LightPlayer domain:

- record field names and order
- map key types
- enum variant names and payload shapes
- option presence
- semantic scalar leaves
- defaults and skip rules
- useful error paths

Serde can remain for host-only tooling, tests, external compatibility, and
small non-hot paths. The embedded path should stop depending on broad typed
Serde monomorphization for slot-heavy data.

## Proposal A: Mockup-First Slot Codec

Use `lpc-slot-mockup` as the compatibility and semantics harness.

The mockup should first be refreshed to match the current real model shape:

- `ProjectDef` has loader metadata (`kind`), optional project `name`, and
  stable-key `nodes`.
- `NodeInvocation` is artifact-only.
- `ShaderDef` has `glsl_path`, `render_order`, `bindings`, `glsl_opts`, and
  `param_defs`.
- `TextureDef` has semantic `size` plus `bindings`.
- `OutputDef` has `pin`, `bindings`, and optional `options`.
- `FixtureDef` has `render_size`, `bindings`, skipped loader/runtime-ish
  `sampling`, enum `mapping`, `color_order`, `transform`, optional
  `brightness`, and optional `gamma_correction`.
- Fixture mapping uses `MappingConfig::PathPoints { paths,
  sample_diameter }`, `PathSpec::RingArray`, and keyed
  `ring_lamp_counts: MapSlot<u32, ValueSlot<u32>>`.

Then build the real codec APIs in production crates, but test them through a
mock disk-storage engine for TOML and a mock wire-storage engine for JSON. The
mockup should end with TOML/JSON that looks very similar to the current real
model. Any intentional deviations become explicit notes for possible real-model
cleanup.

This is the preferred path because it keeps the production domain stable while
we discover the serialization rules.

## Proposal B: Slot-Driven Authored Decode First

Start with project/source TOML loading.

Use a third-party TOML parser to produce either `toml::Value`, `toml::Table`,
or a thinner custom event/value abstraction. Then convert the parsed syntax
into slot data by walking `SlotShape`.

The first version can still use `toml` for parsing. The experiment is not
"replace TOML." It is "replace typed TOML deserialization." If that wins, then
we can evaluate smaller parsers later.

This attacks the measured hot area while preserving the authored file format,
but it should happen after the mockup establishes the rules. Jumping straight
into `lpc-source`/`lpc-model` risks turning codec design into domain migration.

## Proposal C: Domain Authored Value Boundary

Introduce a small `AuthoredValue` abstraction owned by LightPlayer:

```rust
enum AuthoredValue {
    Null,
    Bool(bool),
    Integer(i64),
    Float(f64),
    String(String),
    Array(Vec<AuthoredValue>),
    Table(Vec<(String, AuthoredValue)>),
}
```

TOML and JSON adapters both feed this shape, or feed equivalent visitor events.
The slot codec consumes `AuthoredValue` and produces domain data. This creates
one behavior path for authored TOML and JSON, with shared error handling.

This is cleaner architecturally, but it is more work than Proposal A. It is a
good second step once the first slice proves a size win.

## Proposal D: Direct Slot Event Codec

Avoid building an intermediate syntax tree by parsing TOML/JSON into a
`SlotDecodeVisitor` that is already walking the target `SlotShape`.

This is the most embedded-friendly long-term shape because it can reduce peak
RAM and code size, and it naturally supports streaming. It is also the hardest
to implement and debug. Treat it as a later optimization after the domain
semantics are proven against one tree-backed parser.

## Vertical Slice Experiment

The initial target should prove the premise with minimal blast radius.

Recommended slice:

1. Refresh `lpc-slot-mockup` so its source model mirrors the current real model
   shape closely enough to exercise the same concepts.
2. Add reusable codec code in the real codebase, likely a small
   `lpc-slot-codec`-style module/crate or a narrow `lpc-wire`/`lpc-model`
   module if extraction is premature.
3. Build mock TOML disk-storage tests and mock JSON wire-storage tests on top of
   that codec.
4. Decode TOML through a new slot reflection decoder:
   - records by field name
   - maps by stable key
   - options by presence
   - enums by `kind` or equivalent variant tag
   - scalar leaves by `SlotValueShape` / `LpType`
5. Write JSON with both owned and borrowed/direct paths where appropriate,
   proving that streaming/manual writers and serde-compatible output share the
   same native slot semantics.
6. Iterate until discriminators, defaults, skipped fields, option presence, map
   keys, and semantic leaves behave cleanly.
7. Document any mockup-vs-real-model deviations as candidates for real-model
   cleanup.
8. Only after the mockup is working, build firmware and compare `cargo bloat`
   before/after on one real adoption slice:
   - total `.text`
   - `lpc_model`
   - `serde_core`
   - `toml`
   - `toml_parser`
   - `serde_json`

Success is a working mock storage/wire engine that resembles real TOML/JSON
closely, plus a clear adoption plan for the real code. The first firmware size
win comes after that, when one real path switches over.

## Design Pressure Points

### Hydration

Decoding into `SlotData` is generic. Building a concrete Rust struct from
`SlotData` can become another source of per-type code if done casually.

For the slice, accept narrow hand-written hydration for one type. For the
longer-term engine, consider extending `SlotRecord` derive to generate compact
slot-domain hydration/set-field methods instead of Serde visitors.

### Defaults

Serde attributes currently encode defaults, skipped fields, aliases, and
flattening. The slot model needs its own smaller vocabulary for the pieces the
domain actually wants:

- field default value
- optional field absence
- ignored loader metadata such as `kind`
- unknown field policy
- compatibility aliases, only where truly needed

### Enums

Source TOML currently uses Serde tagging such as `kind = "path_points"`. Slot
enum shapes know variant names but not necessarily the authored tag convention.
The slice should standardize a slot-authored enum convention instead of
recreating all Serde enum modes.

### Error Reporting

A custom codec needs strong errors from the start:

- slot path / authored path
- expected shape
- actual value kind
- unknown field
- missing required field
- bad map key
- bad enum variant

Span-rich errors can wait until parser choice is settled, but path-rich errors
should be part of the first slice.

### Serde And Manual Writers

Borrowed slot JSON writing already exists. The native codec should define when
we use:

- Serde compatibility in host/tests or small non-hot typed surfaces.
- Owned native slot serialization when allocating a `SlotData` tree is fine.
- Borrowed/direct native slot writers when firmware must avoid building an
  owned tree.
- Streaming/event writers for large wire responses.

The mockup should exercise all of these modes, even if the first implementation
only has one or two concrete writers.

## Recommended Path

1. Keep the loose root `notes.md` as raw handoff only temporarily; this roadmap
   folder is the durable home.
2. Bring `lpc-slot-mockup` up to date with the current production model shape.
3. Implement reusable codec primitives in production code, but consume them from
   mockup tests first.
4. Build mock disk-storage TOML and mock wire-storage JSON tests.
5. Iterate on discriminators, defaults, skip policy, direct writers, and Serde
   interop until the mockup feels like the real model.
6. Write an adoption plan for production source/wire paths after review.

## Non-Goals

- Do not remove the on-device compiler.
- Do not replace TOML syntax before proving that typed Serde decode is the
  problem worth solving.
- Do not build a final binary protocol in the first slice.
- Do not rewrite project sync and source loading while designing the codec.
- Do not make slot reflection depend on `std`.
