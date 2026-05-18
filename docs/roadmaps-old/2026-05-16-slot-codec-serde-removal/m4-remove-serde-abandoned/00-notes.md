# M4 Remove Serde From `lpc-model` Notes

> Superseded: these notes record the attempted wholesale Serde removal plan.
> M4 changed after firmware bloat measurement. Keep Serde available for
> protocol/tooling surfaces, and only replace specific Serde paths when
> measurements justify it.

## Scope

Remove Serde as a direct dependency and behavioral surface from `lpc-model`.
This is the final cleanup milestone after message reads/writes and authored
definition reads/writes have moved to SlotCodec-owned paths.

In scope:

- Remove `serde` derives, manual impls, attributes, imports, and serde-only
  tests from `lpc-model`.
- Remove `serde_json` dev-dependency usage inside `lpc-model`.
- Remove the `toml` crate's `serde` feature from `lpc-model` if no longer
  required.
- Replace remaining model-local serde tests with SlotCodec tests, direct parser
  tests, or delete them when they only prove old behavior.
- Replace slot infrastructure snapshot serde where needed with explicit
  snapshot codecs.
- Remove stale serde-era helper APIs and docs once the slot-native API owns the
  behavior.
- Validate downstream crates that depend on `lpc-model` without default
  features.

Out of scope:

- Removing Serde from `lpc-wire`, `lpc-source`, `lpc-shared`, apps, or tools
  unless `lpc-model` compile boundaries require small adjustments.
- Redesigning authored TOML syntax.
- Schema versioning and compatibility policy.
- Project-builder authored writing. That was split out and is now routed
  through `NodeDef::write_toml`.
- Broad `NodeDef` API reshaping beyond final naming polish.

## Current State

`lpc-model` still has direct dependencies:

- `serde = { workspace = true, features = ["derive"] }`
- `serde_json` as a dev-dependency
- `toml` with the `serde` feature enabled
- optional `schemars` behind `schema-gen`

Slot-native paths that already exist:

- `SlotShapeRegistry::read_slot_toml`
- `SlotShapeRegistry::write_slot_toml`
- `SlotShapeRegistry::read_slot_json`
- `SlotShapeRegistry::write_slot_json`
- `NodeDef::read_toml`
- `NodeDef::write_toml`
- `NodeArtifact(pub EnumSlot<NodeDef>)`
- Slot value codecs in `slot_codec::slot_value_codec`
- Dynamic slot readers/writers for records, maps, options, enums, values, and
  semantic leaves.

Recent cleanup already done:

- `NodeDef` derives `Slotted`.
- `NodeArtifact` owns the authored root boundary.
- `ProjectBuilder` writes complete `NodeDef` artifacts through
  `NodeDef::write_toml`.
- Project loader reads authored TOML through `NodeDef::read_toml`.
- Structured enum discriminators use Rust variant names such as `Project`,
  `Texture`, `PathPoints`, and `RingArray`.

## Remaining Serde Surface

The audit found Serde usage in several broad categories.

### Domain Records And Enums

Examples:

- `nodes/project/project_def.rs`
- `nodes/texture/texture_def.rs`
- `nodes/shader/shader_def.rs`
- `nodes/output/output_def.rs`
- `nodes/fixture/fixture_def.rs`
- `nodes/fixture/mapping.rs`
- `nodes/fixture/sampling.rs`
- `nodes/texture/format.rs`
- `nodes/shader/glsl_opts.rs`
- `nodes/shader/shader_param_def.rs`
- `binding/binding_def.rs`
- `binding/binding_defs.rs`

These should generally drop serde derives/attributes and rely on `Slotted`,
`SlotValue`, and SlotCodec tests instead.

### Slot Containers And Infrastructure

Examples:

- `slot/value_slot.rs`
- `slot/enum_slot.rs`
- `slot/slot_data.rs`
- `slot/slot_shape.rs`
- `slot/slot_shape_registry.rs`
- `slot/slot_meta.rs`
- `slot/slot_name.rs`
- `slot/slot_path.rs`
- `slot/slot_ref.rs`
- `slot/slot_owner.rs`
- `slot/slot_value.rs`
- `slot/value_ref.rs`

This is likely the trickiest part. Some serde derives support snapshots and
tests rather than authored model loading. These need explicit SlotCodec or
snapshot codec replacements, or deletion if the behavior is no longer needed.

### Semantic Leaves And References

Examples:

- `slots/artifact_path.rs`
- `slots/source_path.rs`
- `slots/dim2u.rs`
- `slots/positive_f32.rs`
- `slots/ratio.rs`
- `slots/xy.rs`
- `slots/render_order.rs`
- `slots/affine2d.rs`
- `slots/color_order.rs`
- `binding/bus_slot_ref.rs`
- `binding/node_slot_ref.rs`
- `binding/binding_endpoint.rs`
- `node/relative_node_ref.rs`
- `resource/resource_ref.rs`
- `product/product_ref.rs`
- `products/visual/visual_product.rs`
- `products/control/control_product.rs`

Most should keep their domain parsing/formatting and `SlotValue` conversions,
but drop serde impls. Tests should shift from serde JSON/TOML round trips to
`SlotValue`/SlotCodec round trips.

### General Model Utility Types

Examples:

- `sync/revision.rs`
- `sync/with_revision.rs`
- `node/node_id.rs`
- `node/node_name.rs`
- `node/node_prop_spec.rs`
- `node/kind.rs`
- `node/node_invocation.rs`
- `node/tree_path.rs`
- `value/lp_value.rs`
- `value/lp_type.rs`
- `value/constraint.rs`
- `value/legacy_kind.rs`
- `value/value_path.rs`
- `project/config.rs`
- `server/server_config.rs`
- `artifact/artifact_loc.rs`

Some of these are not slot-authored node definitions but still live in
`lpc-model`. Each needs a decision: convert to slot/value codecs, move serde
support out of `lpc-model`, or delete serde-only behavior if unused.

### Schema Generation

`schema-gen` currently enables `schemars`, and many model types derive
`schemars::JsonSchema` via `cfg_attr(feature = "schema-gen", ...)`.

Downstream crates expose schema-gen features:

- `lpc-source/schema-gen`
- `lpc-wire/schema-gen`
- `lpv-model/schema-gen`

If `lpc-model` stops deriving serde and removes direct serde, schema generation
may need to move out of `lpc-model`, be replaced by slot-shape schema export, or
be dropped from this crate.

## Open Questions

### Q1. What happens to `schema-gen` in `lpc-model`?

Context: the goal is no direct serde dependency in `lpc-model`. `schemars`
derives are not the same as serde derives, but the current schema-gen surface
tracks the old serde model and appears to duplicate slot shape metadata.

Suggested answer: remove `schema-gen` from `lpc-model` in M4. If host tooling
still needs schemas, replace it later with a slot-shape-derived schema exporter
outside the embedded model crate.

User answer: yes. Remove it completely. If schemas are needed later, generate
them from slot shapes.

### Q2. Should `SlotData` / `SlotShapeRegistrySnapshot` keep serde through a
feature or get explicit codecs now?

Context: shape snapshots are still useful across wire/tooling boundaries, but
keeping serde in `lpc-model` for snapshots violates the M4 deliverable.

Suggested answer: add explicit SlotCodec/native snapshot read/write helpers for
the snapshot forms that are still needed. Do not keep serde behind a feature in
`lpc-model`.

User answer: use explicit custom codecs. Prefer the same reader/writer
interfaces as SlotCodec uses, but do not model slot metadata itself as slotted
domain data.

Implementation direction: metadata codecs should use the shared syntax layer
(`SyntaxEventSource`, `SlotReader`, `SlotWrite`, writer helpers) for JSON/TOML
plumbing, but should not require a `SlotShapeRegistry` to interpret
`SlotShape`, `SlotData`, or registry snapshots.

### Q3. How aggressive should deletion be for serde-only tests?

Context: many tests currently prove `serde_json::to_string`,
`serde_json::from_str`, or `toml::from_str` behavior for types that now have
SlotCodec or `SlotValue` equivalents.

Suggested answer: replace tests when they prove behavior we still care about
through SlotCodec; delete tests when they only prove legacy serde syntax.

### Q4. Do we remove all serde-derived authored compatibility in one pass?

Context: authored syntax has already changed to slot variant names in recent
work. The project is in heavy development and the user has explicitly said not
to preserve serde backwards compatibility.

Suggested answer: yes. Do not retain lowercase/serde compatibility shims in
`lpc-model`.

## User Notes

- Binary size is a leading motivation; generated serde code is the problem.
- The slot system should be the source of truth.
- We are in heavy development; prefer bold cleanup over backwards-compatible
  shims.
- Project writing should be separate from final cleanup. It has already moved
  to `NodeDef::write_toml`.
- Keep the final M4 cleanup focused on removing serde, stale helper APIs,
  old annotations, and docs that describe the transitional state.
