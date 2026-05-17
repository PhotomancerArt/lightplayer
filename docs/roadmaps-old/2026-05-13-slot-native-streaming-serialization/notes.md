# Slot-Native Streaming Serialization Notes

Date: 2026-05-13

## Scope

Define a new serialization architecture where the slot system is the source of
truth for LightPlayer domain data, while TOML and JSON are syntax frontends.

The important reframing from the earlier slot reflection serialization roadmap:
streaming construction is not a distant optimization. It should shape the
design from the start, because embedded RAM pressure is one of the core
constraints.

This roadmap should cover:

- slot metadata needed for storage policy
- syntax-level reader/event APIs
- generated typed construction from slot-aware readers
- TOML authored storage using a tree-backed adapter first
- JSON wire storage using direct streaming where possible
- mockup-first validation before broad production adoption
- a vertical slice into real production code after the mockup proves the shape

Out of scope for the roadmap itself:

- implementing the full engine in one pass
- replacing every Serde derive immediately
- choosing a final binary protocol
- schema-version negotiation
- host/client compatibility migrations for old authored TOML

## User Notes To Preserve

- The slot system should become the source of truth for serialization.
- Persisted domain data should be represented by slot roots, slot records,
  slot enums, slot maps, slot options, or semantic slot leaves.
- Type-specific codec branches are a design smell. `BindingDef` custom TOML
  handling demonstrated this and should not be repeated.
- The mockup should be used as the first serious lab, because moving directly
  into the real domain is too much blast radius while the architecture is still
  forming.
- The mockup should mirror the real model shape: node defs, invocations,
  values, maps, options, enums, and bindings.
- TOML-authored files are usually small enough to parse into `toml::Value`
  first.
- JSON messages may become large enough that `JSON bytes -> SlotData -> typed
  object` is too much memory pressure on embedded.
- On target, RAM pressure is real: a running LightPlayer instance has been
  observed around `mem 137k free / 174k used`. A single 64x64 RGBA unorm16
  image is about 32 KiB, so triple-buffering can matter.
- The low-level stream source should not know target slot shape. It only knows
  syntax: objects, arrays, props, strings, numbers, booleans, nulls.
- Generated code can use a higher-level reader API:

```rust
Self {
    brightness: reader.prop("brightness")?.f32()?,
    mapping: reader.prop("mapping")?.slot_root("Mapping")?,
}
```

- Binary/resource payloads can use a length-prefixed base64 tuple such as:

```json
[8192, "base64data"]
```

This lets the destination buffer be allocated once and filled directly.

## Current Codebase State

### Slot Model

Relevant files:

- `lp-core/lpc-model/src/slot/slot_shape.rs`
- `lp-core/lpc-model/src/slot/slot_access.rs`
- `lp-core/lpc-model/src/slot/slot_data.rs`
- `lp-core/lpc-model/src/slot/slot_shape_registry.rs`
- `lp-core/lpc-slot-macros/src`

The slot model already has:

- `SlotShape::{Ref, Unit, Value, Record, Map, Enum, Option}`
- static root shape ids via `StaticSlotShape`
- borrowed data traversal via `SlotAccess` and `SlotDataAccess`
- field-level access via `FieldSlot`
- record, map, enum, and option access traits
- semantic value leaves via `SlotValueShape` and `SlotValueAccess`
- `SlotRecord` derive support for static records and views

Missing or incomplete for storage:

- default policy metadata
- transient/authored-storage policy metadata
- enum storage style metadata
- generated construction/hydration from a reader
- a formal separation between syntax reader and slot/typed construction

### Existing Authored TOML Codec

Relevant file:

- `lp-core/lpc-wire/src/slot/authored_toml.rs`

The current codec can decode `toml::Value` into owned `SlotData` and encode
owned or borrowed slot data back to `toml::Value`.

It currently hardcodes:

- records as tables
- maps as tables
- enums with `kind = "<variant>"`
- missing `Unit`, `Map`, and `Option` fields as structural defaults
- skipped `Option::None` during encode
- unknown field rejection, except caller-provided ignored fields

This is good enough as a proof, but not the final construction story because
typed hydration currently happens outside the generic codec.

### Production Adoption Slice

Relevant file:

- `lp-core/lpc-engine/src/engine/project_loader.rs`

The current production experiment routes `kind = "texture"` child artifacts
through native slot TOML. The loader parses TOML, decodes `TextureDef` slot
data, then hand-hydrates `TextureDef` from `SlotData`.

That proves the slot codec can load real data, but the handwritten hydration is
the next scalability problem.

### Binding Model Correction

Relevant files:

- `lp-core/lpc-model/src/binding/binding_def.rs`
- `lp-core/lpc-model/src/binding/binding_defs.rs`
- `lp-core/lpc-model/src/binding/binding_endpoint.rs`

`BindingDef` has been corrected into a slot root/record instead of custom
codec behavior. `BindingEndpoint` is currently a semantic string leaf, and the
roadmap should preserve the direction that binding literals eventually become
a real slot enum instead of hidden `LpValue::Struct` conventions.

### Node Definitions

Relevant file:

- `lp-core/lpc-model/src/nodes/node_def.rs`

`NodeDef` is currently a Rust enum wrapper around concrete node defs and still
uses Serde TOML parsing to dispatch on `kind`.

The emerging design is that concrete node defs remain canonical slot roots,
while `NodeDef` becomes a thin slot enum/wrapper when a loader needs a closed
set of expected node definitions.

### OutputDef As Next Real Root

Relevant file:

- `lp-core/lpc-model/src/nodes/output/output_def.rs`

`OutputDef` is the likely next production root because it exercises:

- required scalar fields
- `BindingDefs`
- `OptionSlot<OutputDriverOptionsConfig>`
- defaulted nested fields

It is more interesting than `TextureDef` but smaller than shader or fixture
loading.

## Architecture Direction

The new architecture should separate three layers:

```text
TOML / JSON / future binary syntax
        |
        v
syntax reader or syntax event source
        |
        v
slot-aware reader helpers
        |
        v
generated typed construction / SlotData construction / borrowed writers
```

Low-level syntax events should be target-shape agnostic:

```text
start_object
prop(key)
end_object
start_array
end_array
string_chunk(bytes)
number(...)
bool(...)
null
```

Generated code knows the target type and target slot shape. It can ask a
higher-level reader for field, map, enum, option, leaf, root, default, and
diagnostic behavior.

`SlotData` remains important, but should not be the only production path:

- useful for tests and reference behavior
- useful for host tooling
- useful for sync state where owned dynamic data is appropriate
- too expensive as a required intermediary for large embedded JSON messages

## Storage Metadata Decisions Already Discussed

### Defaults

- Model authored-storage default policy in slot metadata.
- Use Rust `Default` / default instances as the source of actual default values.
- Generated support should be able to construct `T::default()` and read a
  default value by path.
- Do not duplicate full default values into portable `SlotShape` metadata.

### Emit Policy

- Use a universal authored-storage emit policy first.
- Omit `Option::None`.
- Omit empty maps.
- Do not elide scalar/default values yet.
- Add field-level emit policy only after a real exception appears.

### Loader / Discriminators

- Concrete definitions remain slot roots.
- `NodeDef` should become a thin one-level slot enum/wrapper for contextual
  loading and expected-type allowlists.
- Normal tagged enum discriminators should use PascalCase Rust variant names.
- Top-level artifact `kind` should eventually be wrapper/envelope metadata, not
  a skipped field inside a concrete def.

### Transient Fields

- Avoid `#[slot(skip)]` for persisted domain models.
- Prefer `#[slot(transient)]` for values that are slot-visible and wire-visible
  but omitted from authored disk storage.

### Unknown Fields

- Unknown/unexpected data is an error until schema versioning is formalized.
- Wire shape registry updates need special care: new shape data should be
  applied/validated before messages that depend on it.

### Compact Single-Value Enums

Likely support an explicitly enabled enum storage style for cases like:

```toml
source = { ref = "bus#visual.out" }
source = { value = 123 }
```

This should be general enum metadata, not a `BindingEndpoint` special case.
For this compact form, lowercase/domain-facing keys are acceptable because
they read like inline table fields. Normal tagged enum discriminators remain
PascalCase.

## Open Questions

### Q1. Should the roadmap name the generated construction API as a first-class milestone?

Context:

- The current `TextureDef` slice proves generic `toml::Value -> SlotData`.
- The next scalability problem is typed construction.
- The embedded RAM concern means a direct reader-to-object path should shape
  the architecture, not be treated as a late optimization.

Suggested answer:

- Yes. Make generated construction from a slot-aware reader a first-class
  milestone.
- Keep `SlotData` construction as reference behavior and for tests.
- TOML can adapt a value tree to the reader first; JSON should target direct
  streaming earlier.

User response:

> Yes.

### Q2. How far should the first streaming JSON slice go?

Context:

- We do not need to solve every project message immediately.
- The risk is overbuilding a large parser abstraction before one real message
  forces the shape.

Suggested answer:

- Start with a small mock JSON message that includes records, maps, options,
  enums, chunked strings, and the `[len, base64]` binary convention.
- Do not make the first slice load production project messages.
- Use the mockup to prove peak-buffer behavior and reader semantics.

User response:

> Yes.

### Q3. Should the TOML path share the same reader API even if it starts from `toml::Value`?

Context:

- TOML table semantics make true streaming harder.
- Authored TOML files are expected to be small.
- Sharing construction semantics matters more than avoiding a `toml::Value`
  allocation in the TOML path.

Suggested answer:

- Yes. TOML should expose the same higher-level reader API, even if its source
  is a value tree.
- This keeps generated construction code format-agnostic.

User response:

> Yes.

## User Milestone Shape

The roadmap should roughly break down as:

1. Build parser/generator foundation:
   - event stream
   - JSON parser
   - TOML converter
   - slot reader wrapping the event stream and `SlotShapeRegistry`
   - output stream concept, likely replacing the earlier JSON stream
   - manual reader/writer functions
   - inverse writing path for objects
   - round-trip tests validating the model
2. Build codegen and apply it to the mockup project. Prove it works and change
   the architecture as needed.
3. Reshape the real domain to fit the validated shape.
4. Use the custom serializer for loading and messages.
5. Remove Serde from the `no_std` core parts of the project completely.
