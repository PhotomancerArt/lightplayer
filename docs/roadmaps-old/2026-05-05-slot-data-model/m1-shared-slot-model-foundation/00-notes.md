# M1 Notes: Shared Slot Model Foundation

## Scope Of Work

This plan implements Milestone 1 of the slot data model roadmap:

- Add the foundational slot data model to `lpc-model`.
- Keep the work model-only; do not migrate real node defs, runtime nodes, wire
  sync, or client projection.
- Establish names, rustdocs, exports, and tests that later milestones can build
  on.

In scope:

- `SlotPath`
- `SlotRef` using `SlotPath`
- `SlotTree`
- `SlotData`
- `SlotRecord`
- foundational `SlotMap`, `SlotEnum`, and `SlotOption` data structures
- `SlotShape`
- `SlotShapeId`
- `SlotRegistry`
- `SlotMeta`
- `ModelValue::Resource(ResourceRef)`
- matching `ModelType` support
- basic shape/data validation

Out of scope:

- Applying slot data to real node defs.
- Runtime resolver/binding rewrites.
- Generic wire sync.
- Derive macros.
- Server-side artifact mutation.
- Dynamic shader param generation.
- Full static/dynamic authoring ergonomics; Milestone 2 owns that.

## Current Codebase State

`lp-core/lpc-model/src/slot/` currently contains:

- `slot_name.rs`: opaque `SlotName` string wrapper.
- `slot_owner.rs`: `SlotOwner::{Node, Bus}`.
- `slot_ref.rs`: `SlotRef { owner: SlotOwner, slot: SlotName }`.
- `value_ref.rs`: `ValueRef { slot: SlotRef, path: ValuePath }`.

The current `SlotRef` is flat. M1 should move it to:

```rust
pub struct SlotRef {
    pub owner: SlotOwner,
    pub path: SlotPath,
}
```

`lp-core/lpc-model/src/prop/model_value.rs` currently has shader-ish portable
values plus `Array` and `Struct`. It does not have `ResourceRef`.

`lp-core/lpc-model/src/prop/model_type.rs` mirrors `ModelValue`. It does not
have a resource type.

`lp-core/lpc-model/src/resource.rs` already defines `ResourceRef`,
`ResourceDomain`, `RuntimeBufferId`, and `RenderProductId`.

`lp-core/lpc-model/src/versioned.rs` defines `Versioned<T>`, re-exported
through `prop`. `SlotData::Value` should use `Versioned<ModelValue>`.

`lpc-model` is `no_std` with `alloc`. Use `alloc` collections or existing
workspace dependencies; do not introduce `std` requirements.

## Roadmap Decisions Applied Here

- Core model types live in `lpc-model`.
- Use `SlotData`, not `SyncData`.
- Keep `SlotOwner` and `SlotTree` distinct:
  - `SlotOwner`: identity/authority.
  - `SlotTree`: rooted data/access object.
- Use `SlotShapeId` and `SlotRegistry` from the beginning.
- A registered `SlotShapeId` owns one complete `SlotShape` tree.
- Do not use internal shape-id references inside a registered shape tree.
- Start with the shape/data vocabulary `Value`, `Record`, `Map`, `Enum`,
  `Option`.
- Do not add arrays or tuples to slot data in M1.
- Add `ResourceRef` as a `ModelValue` variant.
- Keep `ModelValue` / `ModelType` names for now.
- Rich slot data is not automatically shader-compatible.

## Suggested Design

Module layout:

```text
lp-core/lpc-model/src/slot/
  mod.rs
  slot_name.rs
  slot_owner.rs
  slot_path.rs
  slot_ref.rs
  value_ref.rs
  slot_meta.rs
  slot_shape.rs
  slot_registry.rs
  slot_data.rs
  slot_tree.rs
```

Key model sketch:

```rust
pub struct SlotPath {
    segments: Vec<SlotName>,
}

pub struct SlotRef {
    pub owner: SlotOwner,
    pub path: SlotPath,
}

pub struct SlotTree {
    pub shape: SlotShapeId,
    pub root: SlotData,
}

pub enum SlotShape {
    Value { meta: SlotMeta, ty: ModelType },
    Record { meta: SlotMeta, fields: Vec<SlotFieldShape> },
    Map { meta: SlotMeta, key: SlotMapKeyShape, value: Box<SlotShape> },
    Enum { meta: SlotMeta, variants: Vec<SlotVariantShape> },
    Option { meta: SlotMeta, some: Box<SlotShape> },
}

pub enum SlotData {
    Value(Versioned<ModelValue>),
    Record(SlotRecord),
    Map(SlotMap),
    Enum(SlotEnum),
    Option(SlotOption),
}
```

Exact field names can be adjusted during implementation if the resulting Rust
is clearer. The semantic constraints are more important than the exact spelling.

## Open Questions

### Q1: Should M1 define all initial shape variants or only `Record`/`Value`?

Context: The roadmap says the initial vocabulary is `Value`, `Record`, `Map`,
`Enum`, and `Option`. Milestone 2 owns deeper static/dynamic authoring.

Suggested answer: M1 should define all five variants and enough validation/tests
to prove their basic shape/data matching. M2 can then focus on ergonomic
authoring and richer examples.

Status: answered by plan direction.

### Q2: Should `SlotShape` use `Box<SlotShape>` or `Vec`-owned child shapes?

Context: A registered shape owns one complete tree. Internal `SlotShapeId`
references are out of scope.

Suggested answer: Use direct owned child shapes. Use `Box<SlotShape>` for single
child shapes (`Map.value`, `Option.some`) and `Vec` for field/variant lists.

Status: answered by plan direction.

### Q3: What collection type should maps use?

Context: Map keys are stable identity, not arbitrary payload data. `ModelValue`
is too broad because floats and composites have awkward key semantics.
`lpc-model` is `no_std` with `alloc`; deterministic ordering helps tests and
wire snapshots.

Suggested answer: Use a constrained `SlotMapKey`, not `ModelValue`. Start with
string and integer keys:

```rust
pub enum SlotMapKey {
    String(String),
    I32(i32),
    U32(u32),
}

pub enum SlotMapKeyShape {
    String,
    I32,
    U32,
}
```

Use `BTreeMap<SlotMapKey, SlotData>` if viable under `no_std + alloc`;
otherwise use a deterministic sorted vector.

Status: resolved.

### Q4: Should `SlotPath` allow empty paths?

Context: A `SlotTree` has a root, but external references usually point to a
specific slot under that root.

Suggested answer: `SlotPath` should allow a root/empty path for traversal APIs,
but parsing from user/authored strings should reject empty unless explicitly
using a `SlotPath::root()` constructor. This avoids ambiguous empty text while
keeping traversal ergonomic.

Status: answered by plan direction.

### Q5: How much shape/data validation belongs in M1?

Context: Full validation for dynamic authoring can grow large.

Suggested answer: M1 should include basic recursive validation:

- value model type matches value form,
- record field count matches shape field count and indexed fields match the
  corresponding shape field,
- map keys match SlotMapKeyShape and values match value shape,
- enum variant exists and payload matches variant shape,
- option some matches shape, none is accepted.

M2 can add richer static/dynamic construction APIs and validation ergonomics.

Status: answered by plan direction.

## Notes For Implementation

- Keep tests at the bottom of Rust files.
- Use rustdocs to describe semantic meaning, not plan history.
- Avoid introducing `std` into `lpc-model` core paths.
- Keep `ModelValue` rename out of scope.
- Do not change shader ABI conversion in M1.
- Do not update source/runtime/wire APIs except for compile fallout from
  `ModelValue::Resource` or `SlotRef` shape changes.
