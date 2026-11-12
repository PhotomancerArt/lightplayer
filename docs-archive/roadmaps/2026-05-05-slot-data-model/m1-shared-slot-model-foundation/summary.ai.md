# Summary

M1 added the shared slot model foundation in `lpc-model`.

Implemented:

- `ModelValue::Resource(ResourceRef)` and `ModelType::Resource`.
- `SlotPath`, with dotted path parsing over `SlotName` segments.
- `SlotRef { owner, path }`, keeping value projection separate in `ValueRef`.
- `SlotMeta`, `SlotShape`, `SlotShapeId`, `SlotMapKeyShape`, and `SlotRegistry`.
- `SlotData`, `SlotRecord`, `SlotMap`, `SlotMapKey`, `SlotEnum`,
  `SlotOption`, and `SlotTree`.
- Recursive validation of `SlotTree` data against registered `SlotShape` trees.
- `serde`/schema support for the new public model types and `Versioned<T>`.

Important decisions captured in code:

- Record data is indexed. Field names and ordering live in `SlotShape`, not in
  every `SlotRecord` snapshot.
- `SlotTree::get` takes a `SlotRegistry` because indexed record traversal needs
  the registered shape to map names to field positions.
- Maps use constrained key domains: string, `i32`, and `u32`.
- `SlotData::Value(Versioned<ModelValue>)` is the version boundary for leaf
  values.

Validation run:

```bash
cargo fmt
cargo fmt --check
cargo test -p lpc-model
cargo test -p lpc-source
cargo test -p lpc-wire
cargo test -p lpc-view
cargo check -p lpc-model --features schema-gen
```
