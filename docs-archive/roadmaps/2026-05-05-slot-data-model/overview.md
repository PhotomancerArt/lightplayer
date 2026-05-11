# Slot Data Model Roadmap

## Motivation And Rationale

LightPlayer needs one coherent model for authored config, dynamic params,
runtime state, bindings, UI editing, and sync. The current system still carries
too many parallel ideas: source defs/configs, runtime products, wire resource
payloads, legacy state objects, and shader-compatible values. That makes it hard
to answer basic questions such as "what changed?", "what can be edited?", "what
can be bound?", and "how should the client render this?"

The core insight for this roadmap is that slots are the domain boundary for
versioned structured data. A slot tree gives each node, bus, or project a
structured namespace where each meaningful edit/production boundary can carry
its own version.

This model should support:

- Rust-authored config and state shapes.
- Dynamic artifact-authored data, especially shader params.
- Derive-assisted Rust authoring after the manual model is validated.
- UI-renderable metadata.
- Stable sync granularity.
- Rich config structures such as enums, maps, and options.
- Resource references as ordinary portable values.
- A clear boundary between rich authoring data and shader ABI values.

## Architecture And Design

```text
SlotOwner
  identifies who owns a slot namespace

SlotTree
  rooted data/access object for an owner's slots

SlotRef
  owner + SlotPath

SlotPath
  path through a SlotTree

SlotData
  one node in the current data tree

SlotShape
  schema/metadata for SlotData

SlotRegistry
  owns registered SlotShape trees by SlotShapeId
```

The initial shape vocabulary should be rich enough for real node config without
trying to be a general programming language:

```rust
enum SlotData {
    Value(Versioned<ModelValue>),
    Record(SlotRecord),
    Map(SlotMap),
    Enum(SlotEnum),
    Option(SlotOption),
}
```

Matching `SlotShape` variants describe the schema and metadata for those
structures. Arrays and tuples are intentionally omitted at first. Dynamic
collections should use maps with stable ids so UI editing and sync are not tied
to fragile array indexes.

Shape ownership should be explicit from the beginning:

```text
SlotRegistry
  SlotShapeId -> complete SlotShape tree
```

A registered shape owns one complete tree. Internal references to shared/global
shape fragments are out of scope until duplication or identity needs prove real.

`ModelValue` remains the portable leaf payload for now, but it should gain
`ResourceRef` so resource handles can travel through generic slot data. A future
rename to `Value` / `ValueShape` or `LpValue` / `LpType` can wait until the
model is stable.

## Alternatives Considered

### Keep SlotShape Tiny

One option was:

```rust
enum SlotShape {
    Record { fields: ... },
    Value { value_shape: ValueShape, meta: ... },
}
```

This keeps version boundaries simple but introduces another value-shape layer
besides `ModelValue`/`ModelType`. It also pressures large editable structures
such as fixture mappings into atomic values.

### Use Arrays For Dynamic Collections

Arrays are familiar, but index identity is painful for UI sync. Inserting or
reordering an item can make unrelated entries appear changed. This roadmap
prefers maps with stable ids for dynamic collections.

### Name The Model SyncData

Sync is only one consumer. The same structure is authored, mutated, observed,
bound, and synced. `SlotData` better names the domain concept.

### Treat Def And Config As Wholly Separate

Node refs feel different from scalar config, but server-side mutation may need
to edit graph references through the same model. This roadmap avoids hardening a
semantic wall between def and config too early.

## Risks

- The model may get too abstract before it is exercised against real nodes.
- Static and dynamic shapes may need different storage strategies, which can
  complicate the registry.
- Enums and maps introduce lifecycle questions for variant switches and key
  removal.
- Folding `ResourceRef` into `ModelValue` will touch wire/view/resource code.
- The derive macro could obscure model mistakes if it is introduced before a
  manual slice validates the shape.
- Applying the model across all existing nodes may reveal config structures that
  do not fit the first vocabulary cleanly.
- Shader ABI conversion must remain explicit; rich slot data must not be
  confused with values a shader can consume directly.

## Scope Estimate

This is a multi-milestone domain-model effort. It should be executed through
separate implementation plans under this roadmap, not as one large patch.
