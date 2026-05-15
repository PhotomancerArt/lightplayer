# Slot Factory Defaults Notes

## Scope

Add the missing default-construction layer for slot shapes.

The target API shape is:

```rust
registry.create_default(shape_id) -> Result<Box<dyn SlotMutAccess>, SlotFactoryError>
```

This gives generic code a real slot object, not an inert data blob. Static
shapes create typed Rust defaults. Dynamic shapes create a dynamic slot object
that still implements `SlotAccess` and `SlotMutAccess`.

In scope:

- Add a registry-owned factory concept for constructing default slot objects.
- Add a `DynamicSlotObject` wrapper around `{ shape_id, SlotData }`.
- Add a dynamic factory that recursively builds `SlotData` from `SlotShape`
  when a shape is explicitly registered as dynamically creatable.
- Make static shape registration/codegen install factories that call
  `T::default()`.
- Add generic insertion/creation operations needed by map, option, and enum
  deserialization.
- Prove the shape with mockup tests.

Out of scope:

- Full replacement of generated codec deserialization.
- Full TOML writer implementation.
- Runtime client insert/remove wire operations.
- Downcasting `Box<dyn SlotMutAccess>` back to a concrete type.
- Binary size measurement. Keep the design size-conscious, but measure in a
  later pass.

## Current State

### Dynamic "Any Slot Object"

`SlotAccess` is the dynamic object trait:

```rust
pub trait SlotAccess {
    fn shape_id(&self) -> SlotShapeId;
    fn data(&self) -> SlotDataAccess<'_>;
}
```

`SlotMutAccess` is the mutable counterpart:

```rust
pub trait SlotMutAccess: SlotAccess {
    fn data_mut(&mut self) -> SlotDataMutAccess<'_>;
}
```

Static Rust types can implement both. Dynamic containers such as `SlotRecord`,
`SlotMapDyn`, `SlotEnum`, and `SlotOptionDyn` expose lower-level access traits,
but a plain `SlotData` does not have a root `shape_id`. Dynamic root objects
need a wrapper:

```rust
DynamicSlotObject {
    shape_id: SlotShapeId,
    data: SlotData,
}
```

### Registry

`lp-core/lpc-model/src/slot/slot_shape_registry.rs` currently stores:

```rust
pub struct SlotShapeRegistry {
    pub ids_revision: Revision,
    shapes: BTreeMap<SlotShapeId, SlotShapeEntry>,
}

pub struct SlotShapeEntry {
    pub changed_at: Revision,
    pub name: Option<String>,
    pub shape: SlotShape,
}
```

The registry is serializable and snapshot-friendly. Factory hooks are executable
runtime behavior, not wire data. That creates an important implementation
constraint: factories must not become serialized shape metadata.

### Shape Registration

Static shape registration is generated in `lpc-slot-codegen`:

```rust
pub fn register_all_static_slot_shapes(registry: &mut SlotShapeRegistry) -> Result<(), ...>
```

The generated dispatch currently calls:

```rust
<T as StaticSlotShape>::ensure_registered(registry)
```

This registers shape metadata and referenced shapes, but not a default object
factory.

Dynamic runtime shapes use:

```rust
registry.register_shape(id, shape)
registry.replace_shape(id, shape)
```

Example: mockup shader runtime param shapes are registered/replaced as dynamic
artifact-instance shapes.

### Defaults

Typed values already generally have Rust defaults:

- `ValueSlot<T>: Default where T: Default`
- `MapSlot<K, V>: Default`
- `OptionSlot<T>: Default`
- many semantic leaves now implement `Default`

Enums recently gained explicit `SlotEnumDefaultVariant` for active variant
switching by default payload.

What is missing is a registry-level way to create "the default object for this
shape id" after the concrete Rust type has been erased.

## User Direction

- `SlotShapeEntry` should have something like a trait/function factory hook.
- Registering a shape should include the function or trait that knows how to
  create the shape.
- The registry should learn:

```rust
create_default(shape_id) -> Box<dyn SlotMutAccess>
```

- Static shapes should delegate to `::default()`.
- Dynamic shapes should build themselves.
- The public API should return a slot object, not raw `SlotData`.
- Dynamic objects are still `SlotMutAccess`; if needed they should be wrapped as
  a root object with a shape id.
- Factories should not be optional. A registered root shape should have explicit
  creation behavior: typed factory, dynamic factory, or an explicit
  uncreatable/unsupported factory.
- Do not assume every shape can be dynamically created. Some shape metadata may
  describe an interior/dynamic record where standalone construction is not a
  useful operation, such as shader parameter record shapes.
- Factory choice is also the opt-in point for generic mutation/deserialization.
  If a deserialize operation needs to create a shape whose factory is
  unsupported, that is an error. The reader should stop there rather than
  inventing a partial object.

## Open Questions

### Should factories live directly in `SlotShapeEntry`?

Suggested answer: yes conceptually, but keep the executable hook runtime-only.

`SlotShapeEntry` is currently serialized in snapshots. A factory function or
trait object cannot be wire data. The implementation can either:

- add a skipped runtime-only factory field to `SlotShapeEntry`, with custom
  `Debug`/`PartialEq` behavior that ignores the factory, or
- keep a parallel `factories: BTreeMap<SlotShapeId, SlotFactory>` inside
  `SlotShapeRegistry`.

The first matches the user's mental model. The second keeps serde/derive churn
smaller and makes it obvious factories are not shape metadata. The plan should
prefer the lowest-friction implementation while preserving the public model:
entries have associated creation behavior, snapshots do not.

### Are factories optional?

Answer: no.

A registered root shape should have explicit creation behavior. That does not
mean every shape must successfully produce an object through the generic dynamic
builder. It means the registry should not have "missing factory" as normal
state. The factory can be:

- a static typed default factory,
- a dynamic object factory,
- or an explicit unsupported factory with a clear error.

This keeps accidental dynamic construction out of the model. Dynamic default
construction is useful, but it is a chosen factory policy, not an implicit
fallback for all shapes.

The same factory is the opt-in for generic deserialization. A reader can only
deserialize into a shape if `registry.create_default(shape_id)` succeeds or if
the caller supplied an existing mutable object. If a document references a
non-creatable shape, deserialization should return a clear non-creatable shape
error.

### What happens after applying a remote shape snapshot?

Suggested answer: restored snapshot entries should use explicit snapshot policy.

A snapshot carries shape metadata, not local Rust type identity. After
`apply_snapshot`, the registry still needs complete factory state. The first
implementation should install a consistent snapshot factory policy for every
entry. Since factory choice is the opt-in boundary, restored snapshot entries
should start as explicitly unsupported unless local code later registers a real
static or dynamic factory.

### Does `create_default` need a revision parameter?

Suggested answer: no public revision parameter at first.

Static defaults already use `current_revision()` through their slot containers.
Dynamic defaults should also stamp with `current_revision()`. If tests need
deterministic versions, they can call `set_current_revision` first. A later
low-level helper can accept an explicit revision if needed.

### How should dynamic enum defaults pick a variant?

Suggested answer: first variant for generic dynamic construction.

Static typed enums should use their own `Default` implementation. Dynamic enum
data has only shape metadata, so choosing the first declared variant is simple,
deterministic, and matches many schema systems. Empty enum shapes should return
a clear factory error.

### How does this relate to map insertion?

The factory solves the missing-value half of the map problem. Mutation should
remain conservative by default:

- `set_slot_value` should continue to require existing map keys.
- deserialization/creation paths can opt into `ensure/default` behavior.
- future runtime wire APIs should use explicit insert operations rather than
  silently creating keys during value set.
