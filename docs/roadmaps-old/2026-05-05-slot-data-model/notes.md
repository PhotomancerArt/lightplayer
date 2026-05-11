# Slot Data Model Notes

## Scope Of Work

This roadmap captures and develops the next model layer after the
produced/consumed slot cleanup:

- Slots are the unit of versioning, binding, production, and sync.
- Slot records are grouping/namespacing structures whose children carry their
  own versions.
- Slot values are atomic leaf payloads: they may be internally structured, but
  they are produced and changed as a whole.
- Shape/schema metadata is needed so both Rust and the client can render and
  edit the slot tree.
- Resource references should become portable model values so generic slot sync
  can carry references without special "product" wire shapes.

The effort is too large for one implementation plan. It should be split into
milestones that first establish shared concepts in `lpc-model`, then apply them
to source/runtime/wire/client layers.

## Current Codebase Context

### Existing Slot Vocabulary

Recent runtime cleanup added a small slot identity layer under
`lp-core/lpc-model/src/slot/`:

- `slot_name.rs`: `SlotName` is currently an opaque string. It allows names
  like `config.width` and rejects only empty names and `#`.
- `slot_owner.rs`: `SlotOwner` abstracts a node-owned or bus-owned slot
  namespace.
- `slot_ref.rs`: `SlotRef { owner, slot }` identifies one slot and deliberately
  does not include direction.
- `value_ref.rs`: `ValueRef { slot, path }` combines a slot with `ValuePath`
  for nested reads/projection.

The module docs already say that a `ValuePath` navigates inside the value
exposed at a slot and is not part of slot identity. The new discussion sharpens
that into: a `ValuePath` is not a binding endpoint and is not a version
boundary.

### Existing Value Model

`lp-core/lpc-model/src/prop/model_value.rs` defines `ModelValue`:

```rust
pub enum ModelValue {
    I32(i32),
    U32(u32),
    F32(f32),
    Bool(bool),
    Vec2([f32; 2]),
    // ...
    Array(Vec<ModelValue>),
    Struct {
        name: Option<String>,
        fields: Vec<(String, ModelValue)>,
    },
}
```

`lp-core/lpc-model/src/prop/model_type.rs` mirrors this with `ModelType` and
`ModelStructMember`.

`ModelValue` does not currently include `ResourceRef`. Resource refs live in
`lp-core/lpc-model/src/resource.rs`:

- `RuntimeBufferId`
- `RenderProductId`
- `ResourceDomain`
- `ResourceRef { domain, id }`

The user strongly prefers putting `ResourceRef` directly into `ModelValue` so a
slot value can carry resource references as normal portable data.

### Runtime Product / Wire Resource Scaffolding

`lp-core/lpc-engine/src/runtime_product/runtime_product.rs` currently has:

```rust
pub enum RuntimeProduct {
    Value(LpsValueF32),
    Render(RenderProductId),
    Buffer(RuntimeBufferId),
}
```

This was useful for produced slots in the runtime cleanup, but it creates a
nomenclature question: if runtime has `RuntimeProduct`, what are model and wire
products? The emerging answer is that "product" should probably not become the
generic slot-data term. Engine-owned resources should be represented in portable
slot data as `ModelValue::Resource(ResourceRef)`, while resource bytes stay in a
separate payload endpoint.

`lp-core/lpc-wire/src/project/resource_sync.rs` is still M4.1-specific resource
sync scaffolding. It has separate request/specifier/payload types for runtime
buffers and render products, plus resource kind and metadata enums. This is a
good future target for generic slot/resource sync but probably too much for the
first slot-data model slice.

### Authored Node Definitions

Source node definitions currently mix graph wiring and config fields:

- `lp-core/lpc-source/src/node/fixture/fixture_def.rs`
  - `FixtureDef` has `output_loc` and `texture_loc` node references alongside
    config-ish fields such as `mapping`, `color_order`, `transform`,
    `brightness`, and `gamma_correction`.
- `lp-core/lpc-source/src/node/texture/texture_def.rs`
  - `TextureDef` has flat `width` and `height`.
- `lp-core/lpc-source/src/node/output/output_def.rs`
  - `OutputDef` is an enum with `GpioStrip { pin, options }`.
  - `OutputDriverOptionsConfig` contains several scalar options.
- `lp-core/lpc-source/src/node/shader/shader_def.rs`
  - `ShaderDef` has `glsl_path`, `texture_loc`, `render_order`, and `glsl_opts`.

The user noted that graph/node refs feel different from authoring values. A
future shape such as:

```rust
pub struct FixtureDef {
    pub output_ref: RelativeNodeRef,
    pub texture_ref: RelativeNodeRef,
    pub config: FixtureConfig,
}
```

would make that distinction explicit. `FixtureConfig` could then be a
Rust-authored slot record.

### Prior LightPlayer Attempt

The previous attempt at `/Users/yona/dev/photomancer/lpmini2024/crates/lp-data`
has useful precedent:

- `LpShape`: metadata/shape trait.
- Static and dynamic shape forms.
- Rust structs implement value/record traits directly.
- A shape registry was considered for dynamic shapes.
- Record shape and record value concepts were split.

Useful files:

- `/Users/yona/dev/photomancer/lpmini2024/crates/lp-data/DESIGN.md`
- `/Users/yona/dev/photomancer/lpmini2024/crates/lp-data/src/kind/record/record_shape.rs`
- `/Users/yona/dev/photomancer/lpmini2024/crates/lp-data/src/kind/record/record_value.rs`
- `/Users/yona/dev/photomancer/lpmini2024/crates/lp-data-derive/src/record_value.rs`

The current system should probably borrow the architecture, not copy the exact
implementation wholesale.

## User Notes And Decisions So Far

- Slots are the right unit of versioning and a good granularity for UI sync and
  binding.
- A slot value is produced at once and completely.
- It should not make semantic sense for one part of a slot value to change on
  its own.
- If part of a thing changes independently, it should be a child slot under a
  record.
- Slot records are grouping/namespacing structures. They are not themselves the
  atomic version boundary.
- Example:

```text
<fixture>.state                  Slot record
<fixture>.state.lamp_shapes      Slot value
<fixture>.state.lamp_colors      Slot value
<texture>.config                 Slot record
<texture>.config.size            Slot value containing { width, height }
```

- `width` and `height` should probably become one `size` value. This gives
  cleaner change detection and a better UI granularity.
- The model needs shape metadata for labels, descriptions, and other UI
  rendering hints.
- Rust-authored and dynamic versions are both needed:
  - Rust-authored config/state types for nodes such as fixture/output/texture.
  - Dynamic shader params from artifacts.
- Static and dynamic shapes and values are both first-class requirements. A key
  goal is easy authoring in Rust, but shader params and artifact-authored data
  may not be known at compile time.
- Resource refs should be folded into `ModelValue`.
- The `ModelValue` name is not loved; `Value` or `LpValue` may eventually be
  better. Renaming can wait.
- The first implementation should develop strong concepts before trying to
  replace all existing source/runtime/wire machinery.

## Working Vocabulary

Tentative terms:

```text
SlotOwner      identity/authority: who owns a slot namespace
SlotTree       rooted data/access object for an owner's slots
SlotRef        where: owner + slot path
SlotPath       path through a slot tree
SlotShape      schema/metadata/UI shape
SlotData       one node in the current data tree
SlotRecord     record/grouping instance containing child slots
SlotRegistry   shape storage/lookup
ValuePath      traversal inside a leaf ModelValue for projection/inspection only
```

Current tension:

- `SlotRef` currently uses a flat `SlotName`; the new model likely needs a
  `SlotPath`.
- `SlotValue` is not currently needed as a separate enum. Leaf values can use
  `Versioned<ModelValue>` until `ModelValue` is renamed or the leaf payload
  needs extra semantics.
- `SlotData` is a good instance name because `Slot` alone is overloaded between
  "location", "schema", and "current data".
- `SlotData` is preferred over `SyncData` because sync is only one consumer of
  the model. The same data is authored, mutated, observed, bound, and synced.
- `SlotTree` and `SlotOwner` are related but distinct: `SlotOwner` identifies
  whose namespace this is, while `SlotTree` is the data/access surface.

## Candidate Model Sketch

This is a vocabulary sketch, not yet a finalized design:

```rust
pub struct SlotPath {
    pub segments: Vec<SlotName>,
}

pub struct SlotRef {
    pub owner: SlotOwner,
    pub path: SlotPath,
}

pub struct SlotTree {
    pub root: SlotData,
}

pub enum SlotShape {
    Record {
        meta: SlotMeta,
        fields: Vec<SlotFieldShape>,
    },
    Value {
        meta: SlotMeta,
        value_shape: ModelType,
    },
}

pub struct SlotFieldShape {
    pub name: SlotName,
    pub shape: SlotShapeRef,
}

pub struct SlotData {
    pub shape: SlotShapeRef,
    pub kind: SlotDataKind,
}

pub enum SlotDataKind {
    Record(SlotRecord),
    Value(Versioned<ModelValue>),
}

pub struct SlotRecord {
    pub fields: BTreeMap<SlotName, SlotData>,
}
```

Important invariant:

```text
SlotData references SlotShape. SlotData does not define its own structure.
```

## Open Questions

### Q1: Should this live in `lpc-model` or a new crate?

Context: Slot identity, `ModelValue`, `ModelType`, `Versioned`, and
`ResourceRef` already live in `lpc-model`. The client should understand the same
slot tree and slot shapes.

Suggested answer: Put the core model in `lpc-model`. Add a proc-macro crate
later only if/when derive support is needed.

Status: likely yes, but confirm before writing design.

### Q2: What should the first milestone include?

Context: The full vision touches `lpc-model`, source node definitions, runtime
produced access, resolver/bindings, wire sync, and client projection. Applying
all of that in one plan would be too much.

Suggested answer: First milestone establishes `lpc-model` concepts and tests:

- `SlotPath`
- `SlotShape`
- `SlotData`
- `SlotRecord`
- `SlotRegistry`
- `ModelValue::Resource(ResourceRef)`

Then use one or two small examples/tests to prove the semantics, without
rewiring all nodes yet.

Status: likely yes, but confirm before writing the overview.

### Q3: What is the exact split between `SlotShape`, `SlotData`, `SlotRecord`,
and `SlotValue`?

Context: `SlotShape` is schema/metadata. `SlotData` is instance data.
`SlotRecord` is the record instance. `SlotValue` may be an alias for
`ModelValue`.

Suggested answer:

- `SlotShape`: shape/schema metadata.
- `SlotData`: instance enum-like wrapper referencing shape.
- `SlotRecord`: child slot map for `SlotDataKind::Record`.
- `SlotValue`: do not introduce as a separate enum yet; use
  `Versioned<ModelValue>` and consider `type SlotValue = ModelValue` if the
  readability win is worth it.

Status: unresolved.

### Q4: Should `ResourceRef` go directly into `ModelValue` now?

Context: The user strongly prefers this. It would let generic slot data carry
resource references without runtime/wire product-specific shapes. It also helps
collapse the meaning of `RuntimeProduct::{Render, Buffer}` toward portable
values.

Suggested answer: Yes, add `ModelValue::Resource(ResourceRef)` and likely
`ModelType::Resource` or a resource-typed shape variant. Keep byte payload
fetching separate.

Status: likely yes, but confirm details.

### Q5: How should shape identity and the registry work?

Context: The previous `lp-data` design needed shape storage because data
instances should not carry full recursive shape metadata. Static Rust-authored
shapes and dynamic artifact-authored shapes both matter.

Suggested answer: Introduce:

```rust
pub struct SlotShapeId(String);
pub struct SlotRegistry { ... }
pub enum SlotShapeRef { Inline(Box<SlotShape>)? or Id(SlotShapeId)? }
```

But the first cut may be simpler if `SlotData` stores `SlotShapeId` and tests
use a registry lookup. Avoid inline shapes in `SlotData` unless a concrete use
case demands it.

User answer:

- Use ids and a registry from the beginning.
- Do not store partial shapes in the registry.
- A registered `SlotShape` should be one owned shape tree.
- Shape ownership and lifecycle should be explicit.
- References to common/global shapes may be useful later, but they are out of
  scope for the initial model.

Status: resolved.

### Q6: Should `SlotShape::Record` fields point to shapes by id, by inline
shape, or by direct owned shape?

Context: Shape reuse and dynamic shader params suggest ids. Simplicity suggests
owned recursive shapes.

Suggested answer: Use direct owned child shapes inside one registered shape
tree. Do not create a graph of shape ids inside one `SlotShape` yet:

```rust
pub struct SlotFieldShape {
    pub name: SlotName,
    pub shape: SlotShape,
}
```

This keeps the registry lifecycle simple: a `SlotShapeId` owns one complete
shape tree. Common/global shape references can be revisited later if duplication
or identity needs prove real.

Status: likely resolved, but confirm during overview.

### Q7: What should the top-level field metadata be called and contain?

Context: UI needs labels/descriptions, maybe units, controls, grouping, order,
constraints, and presentation hints. Existing `Constraint`/`Kind` in
`lpc-model/src/prop/` may overlap with this.

Suggested answer: Start with a small `SlotMeta`:

```rust
pub struct SlotMeta {
    pub label: Option<String>,
    pub description: Option<String>,
}
```

Add richer presentation hints later.

Status: unresolved.

### Q8: Should a record itself have a version?

Context: The user model says records group child slots; child slots carry
versions. But a record shape/data may need a version for structural changes
(field add/remove), separate from value changes.

Suggested answer: Do not give `SlotRecord` a value version. Shape/structure
changes should be tracked by shape registry versions or tree/config versions.
Only `SlotDataKind::Value` has a `Versioned<ModelValue>`.

Status: unresolved.

### Q9: Should `SlotPath` support only named field segments at first?

Context: Slot namespace records likely only need field/name segments. Arrays and
enums can be inside `ModelValue`, but are not independently versioned slot
segments.

Suggested answer: Yes. `SlotPath` is a non-empty list of `SlotName`, not
`ValuePath`. If array-like dynamic slot collections are needed later, add a
separate segment type then.

Status: unresolved.

### Q10: How much of the old `lp-data` trait/derive model should be in scope?

Context: The old system had `LpShape`, `RecordShape`, `LpValue`, and derive
support. The current roadmap needs durable model concepts first.

Suggested answer: Out of scope for early milestones. Capture derive support in
`future.md`. The first slice can use plain structs and manual tests.

Status: unresolved.

## Potential Future Work

Items already captured in `future.md` or likely to become later milestones:

- `lpc-model-derive` proc macro for Rust-authored `SlotRecord` / slot value
  codec implementations.
- Generic wire slot sync replacing `lpc-wire/src/project/resource_sync.rs`
  product-specific request/payload shapes.
- Rename `ModelValue` / `ModelType` to `Value` / `ValueShape` or `LpValue` /
  `LpType`.
- Refactor source node defs to separate graph wiring from `*Config` slot
  records.
- Dynamic shader parameter shapes from artifacts.

## Slot Shape Vocabulary Exploration

The roadmap should spend time defining the range of value/slot shapes the system
should support, because this is the authoring-language layer that source files,
runtime nodes, wire sync, and UI editing will share.

User's candidate vocabulary for a modern data system:

- `Value`: atomic scalar or resource/reference-ish payload.
- `Array`: indexed, variable length, all elements have the same shape.
- `Tuple`: indexed, known size, each element has its own shape.
- `Record`: known keys, each field has its own shape.
- `Map`: key/value pairs where all keys share one shape and all values share one
  shape.
- `Enum`: discriminated union of known variants.
- `Option`: `Some<T>` or `None`.

Important context:

- Shader values cannot support the entire authoring vocabulary, and that is
  good. A richer slot/value model creates a clear boundary between authoring
  data and shader ABI data.
- Config values likely need enums soon. Example:

```rust
enum FixtureMapping {
    Points(...),
    Shapes(...),
}
```

- The UI and wire layer must be able to represent these values, not just Rust.
- This rich value vocabulary is one of the missing pieces behind the current
  config/state modeling pain.

### Open Tension: Rich Shapes In SlotShape Or A Separate ValueShape?

One possible model keeps `SlotShape` tiny:

```rust
enum SlotShape {
    Record { fields: ... },
    Value { value_shape: ValueShape, meta: ... },
}
```

The concern with this model is that it introduces another "value shape" layer
besides `ModelValue`/`ModelType`, while `ModelValue` is currently roughly
shader-compatible and not rich enough for natural Rust config/state authoring.

User pressure-test example:

```rust
enum FixtureMapping {
    ShapeMapping { shapes, resolution },
    MatrixMapping { size, routing },
}
```

A real fixture may contain around 190 shapes. The UI may edit one shape at a
time by moving, scaling, or drawing. If `FixtureMapping` is one atomic slot
value, a single shape edit may require sending the whole mapping object. That
may be acceptable for some computed data, but it is worth deciding deliberately.

Alternative model: let the rich authoring vocabulary live directly in
`SlotShape`, so versioned sync can choose natural boundaries inside rich config
structures without introducing a second `ValueShape` layer.

Status: unresolved. This should be a primary roadmap design question before
milestones are finalized.

### Current Suggested Shape Set

The emerging smaller-but-not-flat model:

```rust
enum SlotData {
    Value(Versioned<ModelValue>),
    Record(SlotRecord),
    Map(SlotMap),
    Enum(SlotEnum),
    Option(SlotOption),
}
```

Matching `SlotShape` variants should likely exist for these same structures.

Arrays are intentionally excluded for now. Index-based sync creates identity and
reordering problems in UI systems; stable ids make updates, diffing, and
selection much simpler. If ordered collections become necessary, revisit with a
stable-id collection model rather than a plain index-addressed array.

Tuples are also excluded for now unless a concrete config/state use case needs
them.

Maps become the preferred collection shape:

- Key shape should likely be constrained to stable scalar-ish ids at first.
- Values can be full `SlotData` subtrees.
- UI ordering can be a separate field if needed rather than implicit array
  index identity.

Status: current suggested direction.

### Static And Dynamic Shapes And Values

The roadmap must support both static and dynamic authoring:

- Static Rust-authored shapes/values for node config and state types.
- Dynamic artifact-authored shapes/values, especially shader params.

The old `lp-data` crate separated static and dynamic concrete shapes behind
common traits. That pattern is relevant here because it gives zero-ish-cost
Rust authoring while still allowing dynamic artifact-defined data.

The goal is that natural Rust types can become slot data without manual mirrors
at every call site. For example, a future `FixtureMapping` enum or
`TextureConfig` struct should expose shape and data through shared slot traits
or generated implementations.
