# Notes

## Scope Of Work

Milestone 2 should harden the M1 slot data model around two newly clarified
problems:

- Container structure changes need versions, not just value leaves.
- Rust-authored structs should be able to act as records directly, while
  dynamic authored data still needs owned dynamic record/map/enum/option forms.

M1 already implemented the raw vocabulary:

- `SlotShape::{Value, Record, Map, Enum, Option}`
- `SlotData::{Value, Record, Map, Enum, Option}`
- `SlotRegistry`
- `SlotTree`
- recursive validation

So M2 is not primarily about adding those variants. It is about making their
semantics right enough for future source/runtime/wire work.

In scope:

- Decide and implement the versioning boundary for container structure:
  - map key-set changes,
  - enum variant changes,
  - option presence changes,
  - possibly dynamic record field-set changes.
- Introduce access traits that allow static Rust structs and dynamic owned data
  to share one conceptual record/container interface.
- Keep `SlotRecord` as the dynamic owned record implementation.
- Add static Rust-authored examples where a normal Rust struct acts as a slot
  record without first converting into `SlotRecord { fields: Vec<SlotData> }`.
- Add dynamic examples that approximate shader params or artifact-authored data.
- Update validation/traversal where required by the trait split.
- Capture clear rustdocs for "static", "dynamic", "record", "map structure",
  and "version boundary".

Out of scope:

- Proc-macro derives.
- Applying slot data to real nodes.
- Replacing source defs/configs.
- Wire sync.
- Artifact mutation.
- Arrays/tuples unless an implementation detail absolutely requires a mention.

## Current Codebase Context

### M1 Slot Data

`lp-core/lpc-model/src/slot/slot_data.rs` currently defines owned data:

```rust
pub enum SlotData {
    Value(Versioned<ModelValue>),
    Record(SlotRecord),
    Map(SlotMap),
    Enum(SlotEnum),
    Option(SlotOption),
}

pub struct SlotRecord {
    pub fields: Vec<SlotData>,
}

pub struct SlotMap {
    pub entries: BTreeMap<SlotMapKey, SlotData>,
}

pub struct SlotEnum {
    pub variant: SlotName,
    pub data: Box<SlotData>,
}

pub enum SlotOption {
    None,
    Some(Box<SlotData>),
}
```

This is a good dynamic owned representation. It is not enough for static
Rust-authored records because a struct such as `FixtureConfig` should not need
to allocate or mirror itself into `Vec<SlotData>` just to be inspected as a
record.

### M1 Slot Tree Validation

`lp-core/lpc-model/src/slot/slot_tree.rs` validates owned `SlotData` against
registered `SlotShape` trees. Traversal takes a `SlotRegistry` because record
data is indexed and field names live in the shape.

This validation is currently concrete over `SlotData`. M2 may need to either:

- keep concrete validation for dynamic owned data and add trait-based access
  separately, or
- move validation/traversal to trait-based access so static and dynamic data
  share the same path.

### Versioned Leaves

`lp-core/lpc-model/src/versioned.rs` provides:

```rust
pub struct Versioned<T> {
    value: T,
    version: FrameId,
}
```

M1 uses `SlotData::Value(Versioned<ModelValue>)`, making leaf values the only
version boundary. That is incomplete for containers whose structure can change.

Examples:

- `state.touches` as a map needs a version for the key set. If a touch is
  removed, the client needs to know the keys changed so it can prune the stale
  entry.
- `mapping` as an enum needs a version for the active variant. If the variant
  switches, the old variant data should be discarded by clients.
- `maybe_texture` as an option needs a version for presence. If it changes from
  `Some` to `None`, the client needs an update even though there is no child
  value to version.
- A dynamic record may need a field-set version if fields can be added or
  removed at runtime. Static Rust records likely do not, because their fields
  are defined by the registered shape.

### Shape Vocabulary

`lp-core/lpc-model/src/slot/slot_shape.rs` currently uses owned recursive
shapes:

```rust
pub enum SlotShape {
    Value { meta: SlotMeta, ty: ModelType },
    Record { meta: SlotMeta, fields: Vec<SlotFieldShape> },
    Map { meta: SlotMeta, key: SlotMapKeyShape, value: Box<SlotShape> },
    Enum { meta: SlotMeta, variants: Vec<SlotVariantShape> },
    Option { meta: SlotMeta, some: Box<SlotShape> },
}
```

This is fine for registered dynamic shapes and for serializable shape snapshots.
The old `lpmini2024` design suggests static shape access may also want traits
or static structs to avoid dynamic allocation and boilerplate for Rust-authored
types.

## Prior `lpmini2024` Context

Relevant old files:

- `/Users/yona/dev/photomancer/lpmini2024/crates/lp-data/DESIGN.md`
- `/Users/yona/dev/photomancer/lpmini2024/crates/lp-data/src/kind/record/record_shape.rs`
- `/Users/yona/dev/photomancer/lpmini2024/crates/lp-data/src/kind/record/record_value.rs`
- `/Users/yona/dev/photomancer/lpmini2024/crates/lp-data/src/kind/record/record_static.rs`
- `/Users/yona/dev/photomancer/lpmini2024/crates/lp-data/src/kind/record/record_value_dyn.rs`
- `/Users/yona/dev/photomancer/lpmini2024/crates/lp-data/src/kind/enum_struct/enum_struct_value.rs`
- `/Users/yona/dev/photomancer/lpmini2024/crates/lp-data/src/kind/option/option_value.rs`
- `/Users/yona/dev/photomancer/lpmini2024/crates/lp-data-derive/src/record_value.rs`

Useful old ideas:

- Static shapes use static data and trait access.
- Dynamic shapes own allocated vectors/strings.
- Rust structs implement record value traits directly.
- Dynamic record values own dynamic fields.
- Field lookup is by index first, with name lookup supplied by shape.

Things not to copy blindly:

- The old `LpValueBox` and many per-kind traits are heavier than M2 probably
  needs.
- Some old code uses leaked boxes / unsafe shape lifetime tricks. M2 should
  keep the new model simpler and safer unless a real need appears.

## User Notes

- M2 is real after all because map keys and enum/option structure need version
  tracking.
- "FixtureConfig is a SlotRecord" is the mental model. A Rust struct should be
  able to act as a record, not merely convert into a dynamic record.
- `SlotRecord { fields: Vec<SlotData> }` is right for a dynamic record, but not
  for static Rust-authored records.
- This likely implies traits and static/dynamic versions.
- Avoid `View` as a model-layer name. `View` currently reads as client-side
  `lpc-view` vocabulary on the far side of `lpc-wire`.
- Container structural versions should be explicit container fields, not
  `Versioned<T>` wrappers around whole containers.
- The name/concept `SlotTree` is suspect. In the user's vision, artifacts,
  node defs, runtime nodes, node state structs, node output structs, and dynamic
  shader params are the things that expose slot access. There may not be a
  standalone runtime object called a slot tree.
- A node's produced namespace may itself be slot-accessible. Node internals
  should be able to expose selected state/output/config through access traits.
- Shader definitions are more meta than runtime shader params. `ShaderDef`
  owns authored param definitions such as name/type/shape/default/metadata.
  Runtime shader nodes own dynamic param values materialized from those defs.
- `SlotRegistry` should probably be named more specifically as a slot-shape
  registry. It also likely needs versions for shape ids and shape contents if it
  is synced to clients.
- There may eventually be a generic versioned registry abstraction because the
  codebase now has several id-keyed stores/caches: render products, runtime
  buffers/resources, and slot shapes.
- Top-level-only shape ids may be wrong. Generic traversal may require every
  shape node in the shape tree to have an id, not just the root registered
  shape. This is especially relevant if clients store shape nodes centrally and
  traverse data by shape id.
- `SlotAccess` should definitely know its `shape_id`.
- The real M2 deliverable should be a test data graph that exercises the
  concepts without forcing unfinished real nodes to adopt them.
- A temporary/std-friendly mock crate is acceptable and likely useful. Candidate
  name: `lpc-slot-mockup`.
- The mock crate can have `src/{model,server,client}` and act as a design lab:
  mock defs/configs/state/nodes, rust-authored structs, dynamic shader params,
  registry sync, and client diff application.
- The mock crate is throw-away or temporary-ish. Its purpose is to make the
  model cheap to develop before integrating with constrained `no_std` crates and
  real runtime nodes.

## Open Questions

### Q1: Which containers carry structural versions?

Context: Leaf values already carry `Versioned<ModelValue>`, but clients also
need to learn about removed map keys, active enum variant changes, and option
presence changes.

Suggested answer:

- `SlotMap` carries `keys_version: FrameId`.
- `SlotEnum` carries `variant_version: FrameId`.
- `SlotOption` carries `presence_version: FrameId`.
- Dynamic `SlotRecord` carries `fields_version: FrameId` only if dynamic record
  field sets can change.
- Static Rust records do not carry a record field-set version because their
  fields are fixed by shape.

This keeps versions at structure/change boundaries without versioning every
nested scalar.

Answer: Accepted in principle. Container structural versions are part of M2.

### Q1A: Are shape ids root-only or assigned to every shape node?

Context: M1 registered a complete `SlotShape` tree under a single
`SlotShapeId`. Generic traversal/sync/client rendering may need stable identity
for every child shape as well, not just the top-level root shape.

Answer:

- Every shape node should have a `SlotShapeId`.
- Shapes are still logically owned trees for now: one parent owns each child
  shape.
- The registry must support unregistering a shape tree, removing its owned
  descendants too.
- Shared/non-owning shape references are future work. They will likely be useful
  for common/global shapes, but they are out of scope for M2.

### Q1B: Does a shape node store its own id?

Context: If every shape node has an id, the registry can either store
`SlotShapeId -> SlotShapeNode` or duplicate identity inside the shape node.

Answer:

- Use `SlotShapeId -> SlotShapeNode`.
- The id is the registry key, not a field inside the node.
- Child relationships store child shape ids.

### Q2: Is `FrameId` enough, or should structural versions use `Versioned<T>`?

Context: A map key set is not exactly a normal value; the keys are already
available in `entries`. An enum's active variant and an option's presence are
small values that could be wrapped.

Suggested answer:

- Use explicit `FrameId` fields with semantic names:
  - `keys_changed_frame`,
  - `variant_changed_frame`,
  - `presence_changed_frame`,
  - possibly `fields_changed_frame`.
- Do not wrap the whole map/enum/option in `Versioned<T>` because the container
  also contains independently versioned children.

Answer: Accepted. Use explicit semantic `FrameId` fields.

### Q3: What trait layer should M2 introduce?

Context: M1's `SlotData` is an owned enum. Static Rust structs need to expose
record-like access without allocating dynamic `SlotData`.

Suggested answer:

Introduce trait access around data access, probably starting narrow:

```rust
pub trait SlotRecordAccess {
    fn field_count(&self) -> usize;
    fn field(&self, index: usize) -> Option<SlotDataAccess<'_>>;
}

pub enum SlotDataAccess<'a> {
    Value(&'a Versioned<ModelValue>),
    Record(&'a dyn SlotRecordAccess),
    Map(&'a dyn SlotMapAccess),
    Enum(&'a dyn SlotEnumAccess),
    Option(&'a dyn SlotOptionAccess),
}
```

The dynamic owned types implement these traits. Static examples can implement
them by hand for a small test struct.

Naming note: avoid `View` because that reads as client-side `lpc-view`.
`Access` is the current candidate, but the exact trait names are still open.

Answer: Use `Access` for M2 trait naming.

### Q4: Should `SlotData` remain the dynamic owned data type?

Context: If traits become the primary access model, the enum name could become
ambiguous.

Suggested answer:

Keep `SlotData` as the owned dynamic representation. Add rustdocs saying it is
the owned/dynamic slot data tree. Trait views are the abstraction over both
owned dynamic data and static Rust-authored values.

Refinement: avoid saying "tree" too strongly here. `SlotData` is the owned
dynamic representation for one slot-access node and its descendants.

### Q5: Should shape access also become trait-based in M2?

Context: Static data traits need shape information. M1 has owned `SlotShape`,
which works for a registry snapshot but may allocate for static Rust-authored
types.

Suggested answer:

Start with data access traits and keep `SlotShape` owned. Static examples can
register an owned `SlotShape` describing the Rust struct. A richer static shape
trait can wait until it removes real boilerplate, likely near the derive
milestone.

Follow-up: if `SlotAccess` needs to expose shape directly, M2 may need a small
shape accessor abstraction, but it should avoid reproducing all of `lp-data`'s
static/dynamic shape hierarchy too early.

### Q6: Should M2 include mutation helpers?

Context: Structural versions only matter if mutation APIs update them reliably.
But full server mutation is out of scope.

Suggested answer:

Add minimal methods on dynamic owned containers:

- map insert/remove mark `keys_changed_frame`;
- enum switch marks `variant_changed_frame`;
- option set_some/set_none marks `presence_changed_frame`;
- dynamic record add/remove can be deferred unless dynamic record field changes
  are in M2 scope.

Do not design the message API or artifact mutation yet.

Concern: read-only traits might make later mutation harder. A possible M2
compromise is to define a read access trait family plus reserved mutable trait
names/APIs (`SlotRecordAccessMut`, etc.) for owned dynamic containers, without
using them for server mutation yet.

### Q7: How much dynamic shader-param work belongs in M2?

Context: The roadmap calls for dynamic authored examples approximating shader
params. The real shader-param integration belongs later.

Suggested answer:

M2 should include tests that build a dynamic `SlotShape` and dynamic `SlotData`
for a shader-param-like record/map by hand. Do not wire it into `ShaderDef` or
the resolver yet.

## Likely Plan Shape

Possible phases:

1. Add structural versions to dynamic owned containers.
2. Revisit/demote `SlotTree`; center the model on `SlotAccess`.
3. Add access traits and implement them for owned dynamic data.
4. Add static Rust-authored examples implementing the record access trait.
5. Add dynamic shader-param-like examples and validation/traversal tests.
6. Cleanup, docs, validation, summary.

## Fresh Design Tension

`SlotTree` may be the wrong abstraction. Better candidates:

- `SlotAccess`: trait/interface for one accessible slot-data node.
- `SlotData`: owned dynamic implementation of that interface.
- `SlotShapeRegistry`: versioned store of registered shape trees.
- No separate `SlotTree` object unless an owned dynamic snapshot needs a root
  wrapper for serialization or tests.

Potential root model:

```rust
pub trait SlotAccess {
    fn shape(&self) -> SlotShapeAccess<'_>; // exact shape handle TBD
    fn data(&self) -> SlotDataAccess<'_>;
}

pub enum SlotDataAccess<'a> {
    Value(&'a Versioned<ModelValue>),
    Record(&'a dyn SlotRecordAccess),
    Map(&'a dyn SlotMapAccess),
    Enum(&'a dyn SlotEnumAccess),
    Option(&'a dyn SlotOptionAccess),
}
```

Examples in the user's model:

- `ProjectDef` / `NodeDef` exposes authored artifact data through `SlotAccess`.
- `FixtureConfig` is a `SlotRecordAccess` implementation.
- `FixtureState` is a `SlotRecordAccess` implementation.
- `ShaderDef` owns shader param definitions and can produce/register the param
  shape.
- `ShaderParamsDyn` is an owned dynamic `SlotRecord`/`SlotData` implementation
  on the runtime node, materialized from the authored param definitions.
- A runtime `Node` can implement `SlotAccess` to expose the produced slots it
  chooses to expose.

Open design question: should `SlotAccess` expose the shape directly, a
`SlotShapeId`, or both?

Answer so far: `SlotAccess` definitely exposes `shape_id`. Whether it also
exposes a direct shape reference is still open.

## Revised M2 Deliverable

Create a standalone mock/prototype crate, likely `lpc-slot-mockup`, to prove the
slot model outside the real runtime.

The crate should include:

```text
src/
  model/
    slot access traits
    dynamic slot data
    shape registry
    diff/snapshot types
  server/
    mock server-side graph and mutation helpers
  client/
    mock client registry/data mirror and diff application
```

The mock graph should include:

- Rust-authored defs/configs with normal fields.
- Rust-authored enums, such as mapping variants like `Circle` and `Square`.
- Runtime state structs.
- `ShaderDef` with param definitions: authored metadata such as param name,
  type/shape, label/description/default, not live runtime values.
- Runtime shader nodes with dynamic param values materialized from `ShaderDef`
  param definitions.
- Nested records/maps/enums/options enough to exercise structural versions.

Acceptance tests should show:

- Static Rust-authored data can be referenced through `SlotAccess`/record access
  without converting to dynamic `SlotRecord`.
- Dynamic values can be walked generically using shape ids and access traits.
- The tree can be printed/debug-walked from generic access.
- A mock server can sync the full registry/data snapshot to a mock client.
- A deep server-side mutation produces a diff.
- The client applies the diff and prunes removed map keys / stale enum or option
  state correctly.
- Shape registry changes can be synced or are explicitly stubbed with notes if
  deferred.

This crate should not replace real `lpc-model` yet. It is a pressure-test for
the model, and its useful pieces can be migrated into `lpc-model` after the
shape stabilizes.
3. Add static Rust-authored examples implementing the record view trait.
4. Add dynamic shader-param-like examples and validation/traversal tests.
5. Cleanup, docs, validation, summary.
