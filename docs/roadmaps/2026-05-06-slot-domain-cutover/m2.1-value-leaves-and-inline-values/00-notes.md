# M2.1 Notes: Value Leaves And Inline Values

## Scope Of Work

M2.1 should make the slot/value boundary explicit and prove it in the mockup.

The concrete pressure case is `ring_lamp_counts`. M2 represented it as slot
structure:

```rust
ring_lamp_counts: MapSlot<u32, ValueSlot<u32>>
```

That made the TOML verbose and gave every index its own slot identity/version.
The domain shape is different: ring counts are one logical value, with internal
value structure. M2.1 should make this natural:

```rust
ring_lamp_counts: ValueSlot<RingLampCounts>
```

and the authored TOML should return to:

```toml
ring_lamp_counts = [1, 8, 12, 16, 24, 32, 40, 48, 60]
```

The broader goal is to clarify the three-tree model:

- Node tree: ownership, execution, dataflow.
- Slot tree: addressability, wiring, versioning, sync, watching, mutation.
- Value tree: one solid logical payload that may have internal structure.

## Current Codebase State

There is active interactive refactoring in the worktree.

Important current direction already visible in code:

- `ModelValue` has mostly become `LpValue`.
- `ModelType` has mostly become `LpType`.
- `lp-core/lpc-model/src/prop/{model_value,model_type,value_path}.rs` has moved
  to `lp-core/lpc-model/src/value/{lp_value,lp_type,value_path}.rs`.
- Legacy quantity/proposition concepts have moved under
  `lp-core/lpc-model/src/value/{legacy_kind,constraint}.rs`.
- `lp-core/lpc-model/src/slot/slot_value.rs` exists and now contains the
  former slot-leaf/value conversion concepts.
- `SlotTree` has been removed locally.
- `SlotShapeRegistry` naming is moving from `*_tree` APIs toward `*_root` APIs.
- `LpValueRoot` has been renamed locally to `SlotValue`.
- `SlotValueShape` is the current name again; it fits the surrounding slot
  vocabulary better than `ValueRootShape`.
- `SlotEditorHint` has been renamed locally to `ValueEditorHint`.

Important current rough edges:

- `slot_value.rs` still has a few naming questions:
  - `SlotValue`
  - `LpValueRootId`
  - `SlotValueShape`
  - `ValueEditorHint`
  - `ValueRootError`
- Several stale comments still say `ModelValue` / `ModelType`.
- Some call sites still use `register_tree_with_version`; the public naming is
  partway through the root rename.
- `LpType` currently has fixed-size `Array(Box<LpType>, usize)` but no
  variable-length list type.
- `ring_lamp_counts` is still a `MapSlot<u32, ValueSlot<u32>>` in both
  `lpc-source` and fixture mapping engine code.
- The mockup has not yet pressured an opaque list-like value root.

Relevant files:

- `lp-core/lpc-model/src/value/lp_value.rs`
- `lp-core/lpc-model/src/value/lp_type.rs`
- `lp-core/lpc-model/src/value/mod.rs`
- `lp-core/lpc-model/src/slot/lp_value_root.rs`
- `lp-core/lpc-model/src/slot/value_slot.rs`
- `lp-core/lpc-model/src/slot/slot_shape.rs`
- `lp-core/lpc-model/src/slot/slot_shape_builder.rs`
- `lp-core/lpc-model/src/slot/slot_shape_registry.rs`
- `lp-core/lpc-source/src/node/fixture/mapping.rs`
- `lp-core/lpc-engine/src/legacy/nodes/fixture/mapping/points.rs`
- `lp-core/lpc-engine/src/nodes/core/fixture_node.rs`
- `lp-core/lpc-wire/tests/source_slot_sync.rs`
- `lp-core/lpc-slot-mockup/src/source/mapping.rs`
- `lp-core/lpc-slot-mockup/src/engine/shader_node.rs`
- `examples/basic/fixture.toml`

## Core Model

The slot/value boundary should read as:

```text
SlotData::Value(Versioned<LpValue>)
```

That is the correct join:

- `SlotData` owns versioning and addressability.
- `Versioned<LpValue>` says the whole value payload changes as one unit.
- `LpValue` owns portable structural data for wire/client/source transfer.

`LpType` should remain the storage/validation grammar. It should not own UI
metadata:

```text
LpType = can this LpValue be validated as this shape?
```

`SlotValueShape` should own semantic/editor/meta:

```text
value shape = what does this value mean and how can generic tooling render it?
```

That shape is the root of the value tree at a slot leaf. This is why
`SlotValue` feels like the right trait for now: this concept lives in the slot
system, but marks the end of the slot tree and the beginning of a value tree.

## Mockup Goal

The mockup should prove the rule, not just compile.

Add a source-domain value root that is opaque to the slot tree but internally
structured as an `LpValue`. This milestone should stay in the mockup until the
shape feels right; applying it to real `lpc-source` fixture mapping should be a
follow-up after review. A good mockup slice:

- Add `RingLampCounts` or equivalent list-like authored value in the mockup.
- Use it in a source mapping/path shape.
- Serialize to concise TOML inline array.
- Sync to a client as one `SlotData::Value`.
- Show in tree-walk output that the slot tree stops at `ring_lamp_counts`.
- Show in value rendering/debug output that the contained `LpValue::Array`
  can still be inspected/rendered generically via its value shape.
- Mutate the whole value and confirm one slot patch/version update.

## Naming Notes

Current names we probably keep:

- `LpValue`
- `LpType`
- `ValuePath`
- `SlotData`
- `SlotShape`
- `SlotPath`
- `SlotShapeRegistry`
- `ValueSlot<T>`

Names still under pressure:

- `SlotValueShape`
- `LpValueRootId`
- `ValueRootError`
- `SlotValueAccess`
- `ValueEditorHint`

Suggested direction:

- Keep `ValueSlot<T>`. It is the versioned slot storage wrapper.
- Keep `SlotValue` as the trait name. The concept lives in `slot/`, and the
  trait describes a value that can occupy a slot value boundary.
- Keep `SlotValueShape` for now. It aligns with `SlotShape::Value` and
  `ValueSlot<T>`, even though the names are close.
- Consider renaming `LpValueRootId` to match the shape name:
  - `LpValueShapeId` if shape is `LpValueShape`
  - `ValueRootId` if shape is `ValueRootShape`
- Keep `ValueEditorHint` for now. Editor hints belong to value root shape, not
  `LpType`.

## Open Questions

### Q1: Is `SlotValueShape` the long-term name?

Context:

`SlotShape::Value { shape: SlotValueShape }` makes the boundary explicit: a
slot shape has a value root shape.

Suggested answer:

User tried `ValueRootShape`, then renamed back to `SlotValueShape` because it
fits the other names better in the `slot/` module. Keep `SlotValueShape` through
M2.1.

### Q2: Should `SlotValue` own the shape?

Context:

The user pointed out that the slot value trait feels like it should know its
shape.
Current local code already has:

```rust
pub trait SlotValue: ToLpValue + FromLpValue {
    const LEAF_ID: LpValueRootId;
    fn value_shape() -> SlotValueShape;
}
```

Suggested answer:

Yes. `SlotValue` should provide the full value shape, not just `LpType`.
Metadata/editor hints belong on that value shape, not on `LpType`.

### Q2.5: Should `LpValueRootId` remain separate from `SlotShapeId`?

Context:

If value roots participate in the same shape-root/reference machinery, a
separate `LpValueRootId` duplicates identity with `SlotShapeId`.

Suggested answer:

No. Consolidate on `SlotShapeId`. `SlotValueShape` should use `SlotShapeId` for
its stable semantic identity, and `SlotValue` should expose that same id. This
keeps one registry and one shape identity type.

### Q3: Should `LpType::Array` stay fixed-size and `LpType::List` be added?

Context:

Shader-oriented arrays and authored value lists have different validation
semantics. `ring_lamp_counts` is variable length.

Suggested answer:

Unresolved. `List` is logically right for authored/value data, but shader
interoperability complicates the boundary because GLSL arrays are historically
fixed-size uniform-compatible storage.

Current options under discussion:

1. Introduce true dynamic list support in the shader/runtime path. This seems
   hard and likely requires native functions or a resource-like ABI.
2. Represent authored/runtime value data as a dynamic list in `LpValue`, with
   explicit conversion semantics to shader-compatible fixed arrays, max length,
   count fields, or sentinels.

Keep `LpValue::Array(Vec<LpValue>)` as the data representation for both fixed
arrays and lists. Validation decides whether length must match.

User answer:

Add `LpType::List(Box<LpType>)` for logical dynamic lists. Shader conversion can
be explicitly unsupported for now; the conversion bridge may use
`unimplemented!()` or an unsupported error depending on the surrounding API.

### Q4: Should value shapes be registered separately?

Context:

We discussed and pushed back on two registries. The current preferred model is
one shape registry whose registered roots can be slot roots, reusable container
shapes, or reusable value leaf shapes if needed.

Suggested answer:

Do not add a separate value registry in M2.1. Keep one `SlotShapeRegistry`.
`ValueRootShape` can participate in the same root/reference machinery because it
owns an `LpType` and represents the value tree root at the slot leaf boundary.

### Q5: What should this milestone prove?

Context:

`examples/basic/fixture.toml` is the canonical real example and currently
exposes the wrong verbosity, but the user wants M2.1 to stay mockup-first so the
shape can be reviewed before applying it to the real source model.

Suggested answer:

This milestone should prove the shape in `lpc-slot-mockup` only. The mockup
evidence should assert that:

- `mapping.path_points.paths[0].ring_array.ring_lamp_counts` exists as one
  `SlotData::Value`
- there is no slot path for `ring_lamp_counts[8]`
- the `LpValue` is an array/list of `u32`

If that feels good, a follow-up milestone can apply the same pattern to
`lpc-source`, `examples/basic`, engine fixture mapping code, and source sync
evidence.

## Cleanup Work

Expected cleanup for M2.1:

- Finish the `LpValue` / `LpType` move.
- Remove or update stale `ModelValue` / `ModelType` docs.
- Finish `SlotShapeRegistry` root API naming consistently, or intentionally
  leave compatibility shims with clear docs.
- Ensure removed `SlotTree` exports and docs are gone.
- Ensure generated code/tests compile after root naming changes.
- Update M2.1 notes or summary with final naming decisions.

## Validation Expectations

At minimum:

```bash
cargo fmt --check --package lpc-model --package lpc-source --package lpc-slot-mockup --package lpc-wire --package lpc-view --package lpc-engine
cargo test -p lpc-model --lib --tests
cargo test -p lpc-slot-mockup -- --nocapture
cargo test -p lpc-source --lib --tests
cargo test -p lpc-wire --test source_slot_sync -- --nocapture
cargo test -p lpc-engine --lib --tests
cargo check -p lpc-source --features schema-gen
cargo clippy -p lpc-model -p lpc-source -p lpc-slot-mockup -p lpc-wire -p lpc-view -p lpc-engine --all-targets -- -D warnings
git diff --check
```
