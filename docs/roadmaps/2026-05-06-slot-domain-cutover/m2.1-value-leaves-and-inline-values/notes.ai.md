# M2.1 Notes: Value Leaves And Inline Values

## Why This Exists

M2 made source defs slot roots, but `examples/basic/fixture.toml` exposed a
missing part of the model: not every structured thing should become slot
structure.

`ring_lamp_counts` became:

```toml
[mapping.paths.0.ring_lamp_counts]
0 = 1
1 = 8
2 = 12
```

That is technically consistent with `MapSlot<u32, ValueSlot<u32>>`, but it is
the wrong domain shape. Ring lamp counts are one logical value. Individual
indexes do not have stable authoring identity, should not have independent slot
versions, and should not be separately addressable through slot paths.

The better representation is an opaque value leaf:

```toml
ring_lamp_counts = [1, 8, 12, 16, 24, 32, 40, 48, 60]
```

with a value object that converts to/from `LpValue::Array`.

## Current Naming State

Recent interactive refactoring has moved the portable value vocabulary toward:

- `LpValue` instead of `ModelValue`
- `LpType` instead of `ModelType`
- `value/` instead of `prop/` for the portable value modules
- `ValueSlot<T>` remains the generic versioned Rust storage wrapper
- `SlotValue` is the typed value trait at a slot leaf
- `SlotShapeId` is the one shape identity type, including semantic value shapes
- `SlotValueShape` remains the value-boundary shape vocabulary

This direction is good. Some names may still evolve, but M2.1 intentionally
removed the separate leaf/root id vocabulary.

## Three Tree Model

We have three justified trees, and the code should keep their flavors distinct:

- Node tree:
  Node is the unit of ownership, execution, and dataflow.
- Slot tree:
  Slot is the unit of data wiring, versioning, sync, watching, and mutation.
- Value tree:
  Value is one logical payload. It may have structure, but it is set, versioned,
  and synced as a whole by the containing slot.

This helps clarify the rule:

If the UI/server/client needs to address or diff pieces independently, model it
as slot structure. If it is edited and synced as one thing, model it as an
`LpValue` with a leaf/value shape.

## Concepts To Normalize

### Slot Data

`SlotData` is the generic, versioned, addressable data tree:

- `Record`
- `Map`
- `Enum`
- `Option`
- `Value(Versioned<LpValue>)`
- `Unit`

Slot containers carry structural versions such as field/key/variant/presence
versions. `SlotData::Value` carries one value version for the whole payload.

### Value Data

`LpValue` is the portable value payload for `SlotData::Value`. It is the
on-wire/on-client representation of a solid value.

This is not meant to be an unlimited language. It should be a small, durable
value transport vocabulary:

- scalars
- fixed vectors/matrices
- strings
- resources
- structs
- fixed arrays
- variable lists, likely via a new `LpType::List(Box<LpType>)`

`LpValue::Array(Vec<LpValue>)` can probably continue to be the data variant for
both fixed arrays and variable lists. `LpType` decides validation semantics.

### Value Shape / Leaf Shape

`SlotValueShape` currently holds:

- shape id
- `LpType`
- metadata
- editor hints

This is close to the missing concept. It describes how to validate and render a
solid value leaf.

Open naming question:

- keep `SlotValueShape` to make clear it belongs to the slot system
- rename to `ValueShape` for simpler value-tree terminology
- rename to `SlotLeafShape` if we commit to leaf language

Current feeling: `Leaf` language is useful because it differentiates value
leaves from slot containers and node tree concepts.

M2.1 decision: keep `SlotValueShape` and `SlotValue`, but do not keep
`SlotLeaf` / `SlotLeafId` / `LpValueRootId`.

## Registry Ownership

Current direction: avoid a separate value-shape registry unless forced.

The shape registry can own all reusable shape contracts:

- slot root shapes such as `source.fixture`
- reusable container shapes such as fixture mapping enums
- reusable leaf/value shapes such as `affine2d`, `dim2u`, `ring_lamp_counts`

The semantic distinction is not "slot registry vs value registry." It is:

- container slot shape: nested slot addressability
- leaf/value shape: solid `LpValue` boundary

This points toward keeping one `SlotShapeRegistry` and making leaf/value shapes
first-class inside it.

## Immediate Pressure Cases

### Ring Lamp Counts

Current:

```rust
ring_lamp_counts: MapSlot<u32, ValueSlot<u32>>
```

Likely target:

```rust
pub struct RingLampCounts(Vec<u32>);

ring_lamp_counts: ValueSlot<RingLampCounts>
```

`RingLampCounts` should convert to/from `LpValue::Array(Vec<LpValue::U32>)` and
provide a leaf/value shape whose type is a variable list of `u32`.

### Dim2u And Affine2d

These are already behaving like value objects. They are useful evidence that a
value can have internal structure while still being one slot leaf.

Question: should these continue to expose `SlotValueShape`, or should the
naming move toward a clearer leaf/value-object trait?

### Resources

`LpValue::Resource(ResourceRef)` fits the same model: the value payload carries
a lightweight reference, while the resource sync system manages large texture or
buffer payloads separately.

This reinforces the three-part split:

- resources: large external payloads and handles
- slot data: versioned/addressable sync tree
- value data: opaque portable values inside value slots

## Naming Audit

Names that feel directionally good:

- `LpValue`
- `LpType`
- `SlotData`
- `SlotShape`
- `SlotPath`
- `ValuePath`
- `ResourceRef`

Names that need more thought:

- `SlotValueShape`
- `ValueSlot<T>`
- `SlotValueAccess`
- `ToLpValue`
- `FromLpValue`

Possible concern: `ValueSlot<T>` and `SlotValueShape` sound extremely similar
but mean different things:

- `ValueSlot<T>` is runtime/authored storage with a version.
- `SlotValueShape` is metadata/type/editor shape for an opaque value leaf.

Possible alternatives:

- `LeafSlot<T>` for the versioned storage wrapper
- `SlotLeafShape` for the shape
- `LeafShape` if the module context is already `slot`
- `SlotLeafValueAccess` for access to a value leaf

No decision yet.

## Likely Implementation Slice

1. Finish the `LpValue` / `LpType` move and clean up stale docs that still say
   `ModelValue` / `ModelType`.
2. Add `LpType::List(Box<LpType>)` and validation for `LpValue::Array` against
   list type.
3. Add a ring-lamp-counts leaf/value object.
4. Convert `PathSpec::RingArray.ring_lamp_counts` to the value object.
5. Keep this proof in the mockup before changing real `lpc-source` and
   `examples/basic`.
6. Update mockup sync evidence to assert `ring_lamp_counts` as one opaque
   value leaf.
7. Revisit naming after seeing the concrete code.

## Questions To Keep Live

- Should `SlotValueShape` be renamed, or is it good because value shapes are
  explicitly part of the slot system?
- Should `ValueSlot<T>` become `LeafSlot<T>` to reduce collision with
  `SlotValueShape`?
- Which value objects should be promoted now versus left as source-local leaves?
- Should value leaf shapes become registered roots in `SlotShapeRegistry`, or
  is inline `SlotShape::Value { shape: SlotValueShape }` still enough for now?

## M2.1 Result

- `LpType::Array` is fixed-size and `LpType::List` is variable-length.
- `LpValue::Array` is the payload form for both.
- `RingLampCounts` now proves a value object can sync as one slot leaf while
  still carrying inspectable list structure.
- Real fixture source/example conversion is intentionally deferred until the
  mockup shape has been reviewed.
