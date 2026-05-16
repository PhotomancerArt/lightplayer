# Remove Serde From `lpc-model` Notes

## Scope

Goal: migrate `lpc-model` to use the slot codec as its serialization system and
remove serde from `lpc-model` itself.

The desired end state is:

- authored disk storage goes through slot registry + TOML syntax source/writer
- wire storage goes through slot registry + JSON syntax source/writer
- real model roots are slot-native and do not derive serde
- semantic leaf types serialize through slot value machinery, not serde impls
- `lpc-model/Cargo.toml` no longer depends on serde or serde_json

Out of scope for the first planning pass:

- removing serde from `lpc-wire` or other crates
- changing the on-wire project-read envelope outside the slot payloads
- schema versioning
- preserving backwards compatibility for old authored TOML

## Current State

`lpc-model` still depends on serde directly:

- `serde = { workspace = true, features = ["derive"] }`
- `toml = { version = "0.9", default-features = false, features = ["parse", "serde", "display"] }`
- dev `serde_json = { workspace = true }`

The slot codec path now exists and is the preferred direction:

- `SlotShapeRegistry::read_slot_json`
- `SlotShapeRegistry::read_slot_toml`
- `SlotShapeRegistry::write_slot_json`
- `SlotShapeRegistry::write_slot_json_value`
- `SlotShapeRegistry::write_slot_toml`
- `SlotShapeRegistry::write_slot_toml_data`
- dynamic default construction through `SlotFactory`
- dynamic mutation through `SlotMutAccess`

The mockup has proven this path for JSON/TOML read/write round trips.

## Serde Usage Clusters

### 1. Real Slot Records / Domain Roots

These are the main authored/runtime slot records that still derive serde:

- `binding/binding_def.rs`
- `binding/binding_defs.rs`
- `node/node_invocation.rs`
- `nodes/project/project_def.rs`
- `nodes/texture/texture_def.rs`
- `nodes/shader/shader_def.rs`
- `nodes/shader/shader_param_def.rs`
- `nodes/shader/glsl_opts.rs`
- `nodes/output/output_def.rs`
- `nodes/fixture/fixture_def.rs`

These should become plain slot records with `Default` and slot codec tests.
Existing serde attributes like `#[serde(default)]` and
`#[serde(skip_serializing_if = "...")]` should be replaced by slot defaults and
slot writer omit-empty policy.

### 2. Enum Wrappers / Discriminated Model Types

These need slot-native enum/discriminator handling:

- `nodes/node_def.rs`
- `nodes/fixture/mapping.rs`
- `nodes/fixture/sampling.rs`
- `nodes/fixture/fixture_def.rs` enum leaves
- `nodes/texture/format.rs`
- `nodes/shader/glsl_opts.rs` enum leaves
- product/resource discriminators

Current `NodeDef::from_toml_str` still probes `kind` with serde, then parses the
selected variant with serde. This should become slot-reader discriminator logic
that picks the concrete slot shape and delegates to the registry.

### 3. Slot Infrastructure Snapshot Types

These still derive serde:

- `slot/slot_data.rs`
- `slot/slot_shape.rs`
- `slot/slot_shape_registry.rs`
- `slot/slot_meta.rs`
- `slot/slot_value.rs`
- `sync/revision.rs`
- `sync/with_revision.rs`

These are not authored model roots, but they matter for complete serde removal.
They need slot-codec-native snapshot/debug serialization or a decision that
their current serde-facing tests are obsolete.

### 4. Semantic Leaf / ID / Path Serialization

Many small types still implement or derive serde as strings/compact structs:

- `artifact/artifact_loc.rs`
- `binding/bus_slot_ref.rs`
- `binding/node_slot_ref.rs`
- `binding/binding_endpoint.rs`
- `node/relative_node_ref.rs`
- `node/node_id.rs`
- `node/node_name.rs`
- `node/tree_path.rs`
- `slot/slot_name.rs`
- `slot/slot_path.rs`
- `slot/slot_ref.rs`
- `slot/slot_owner.rs`
- `slot/value_ref.rs`
- `resource/resource_ref.rs`
- `resource/resource_domain.rs`
- `product/product_ref.rs`
- `resources/buffer/runtime_buffer_id.rs`

Some already implement `SlotValue`, `ToLpValue`, and `FromLpValue`. Others are
plain syntax/support values. We need a crisp rule for where their custom
serialization lives.

Current sticky example:

- `BindingEndpoint` is a `SlotValue` with `LpType::String`, but its serde impl
  has richer authored syntax for literals: strings for refs and
  `{ literal = ... }` for literal values.
- `ResourceRef` is a `SlotValue` with `LpType::Resource`, and the slot codec
  currently has custom resource/product read/write logic in
  `slot_codec/slot_value_codec.rs` and `dynamic_slot_writer.rs`.

### 5. Non-Slot Value Metadata

These still derive/use serde:

- `value/lp_type.rs`
- `value/lp_value.rs`
- `value/constraint.rs`
- `value/legacy_kind.rs`
- `slot/slot_value.rs` editor hints and value shape metadata

Some of these are part of the slot schema language, not authored domain roots.
They probably need dedicated slot-codec schema/value codecs before serde can be
fully removed from `lpc-model`.

## Current Tests To Migrate

Representative serde tests still in `lpc-model`:

- `ProjectDef` TOML deserialize
- `OutputDef` TOML deserialize
- `NodeInvocation` JSON/TOML round trips
- `BindingDefs` TOML round trip
- `SlotData` JSON round trips
- `SlotShape` JSON round trips
- `ValueSlot`, `MapSlot`, `OptionSlot` serde tests
- `LpType`, `LpValue`, `Constraint`, and legacy kind serde tests
- string wrapper serde tests for path/ref/id types

These should move to slot codec tests or be deleted if they test only old serde
behavior.

## Notes From User

- The big goal is complete removal of serde from `lpc-model`, using slot codec
  only.
- Backwards compatibility is not important right now.
- Keep the model simple; complex/private-field models should use explicit
  escape hatches rather than making the derive system too clever.
- The sticky topic to discuss first is custom leaf serialization: where the
  custom logic for `ResourceRef`, `ProductRef`, `BindingEndpoint`, `Affine2d`,
  path refs, etc. should live.

## Open Questions

### Q1. What is the exact home for custom semantic leaf codecs?

Context: Today `SlotValue` provides shape metadata plus `ToLpValue` /
`FromLpValue`, but JSON/TOML syntax details for `LpType::Resource`,
`LpType::Product`, and some untyped `LpValue` cases live in
`slot_codec/slot_value_codec.rs` and `dynamic_slot_writer.rs`.

Suggested direction: keep `SlotValue` as the semantic contract and introduce a
small, explicit `SlotValueCodec` concept for syntax-level read/write when
`LpValue` conversion is not enough. Default implementation can go through
`ToLpValue` / `FromLpValue` and `LpType`; special leaves opt into custom syntax.

User answer:

- Yes, this makes sense.
- Plan the leaf codec layer first.
- For single-field refs, compact object syntax like `{ "runtime_buffer": 7 }`
  is attractive but not required immediately.
- `Affine2d` should probably serialize as an array in JSON for compactness.

Working decision:

- Add a first phase for semantic leaf codec design before migrating real model
  records.
- Treat compact special syntax as opt-in per leaf codec, not as a global slot
  record rule.
- Implement the leaf codec trait on the semantic value type, for example
  `ResourceRef`, `Affine2d`, and `BindingEndpoint`, not on `ValueSlot<T>`.
- `ValueSlot<T>` should delegate to `T` for read/write.

Follow-up realization:

- Most semantic leaf types already implement `SlotValue`; that already gives
  the slot system the core type/shape contract.
- The `BindingDef` issue is likely not "custom leaf codec first" so much as
  "make sure the semantic leaf is wrapped by the right revision-tracked slot
  container."
- `BindingEndpoint` already has an `Unset` variant, but a bare
  `BindingEndpoint` field cannot carry a real slot revision because it is just
  an enum value. It needs a wrapper.
- Adopted simpler model:

  ```rust
  #[derive(Clone, Debug, Default, PartialEq, SlotRecord)]
  pub struct BindingDef {
      pub source: ValueSlot<BindingEndpoint>,
      pub target: ValueSlot<BindingEndpoint>,
  }
  ```

- This keeps `BindingEndpoint` as the semantic leaf that belongs to the slot
  system, while `ValueSlot` owns revision tracking.
- `OptionSlot<ValueSlot<BindingEndpoint>>` was removed to avoid two empty
  states. `BindingEndpoint::Unset` is the single empty/default state.
- Current serde compatibility omits unset direction fields so authored TOML
  stays compact; the slot codec should preserve that as a general omit-empty
  policy during the switchover.

Approved value/slot enum boundary:

- `LpValue` is LightPlayer's atomic value language, not a perfect GLSL mirror.
  Shader compatibility matters, but `LpValue` also needs to model definition
  data because node definitions and artifacts are slotted too.
- Add `LpValue::Enum` for atomic enum values used inside `ValueSlot<T>`.
- Use `LpValue::Enum` when the whole choice changes as one leaf and callers do
  not need slot paths into the payload.
- Use `LpType::Any` sparingly for dynamic definition-time payloads such as
  `BindingEndpoint::Literal(LpValue)`.
- Use `SlotShape::Enum` when the active variant exposes addressable slot
  structure, has variant/payload revision behavior, and supports partial
  mutation/sync inside the active variant.
- This decision is documented in `docs/design/slots/overview.md`,
  `docs/design/slots/values.md`, and `docs/design/slots/serialization.md`.

### Q2. Should every serde-derived domain record lose serde in one pass?

Context: real model records are already `SlotRecord` and factories can create
defaults. The registry can read/write these records dynamically by shape.

Suggested direction: yes for the authored node/root records. Move their tests to
registry read/write and remove their serde derives/attributes together.

### Q3. Should `SlotData` / `SlotShape` lose serde in the same migration?

Context: they are infrastructure snapshot/schema types, not domain roots, but
they still keep the serde dependency alive.

Suggested direction: phase this after domain records and leaf codecs. First make
authored model data slot-native, then replace snapshot/schema serialization.

### Q4. How should `NodeDef::from_toml_str` work without serde?

Context: it currently uses serde to probe `kind` and then parse a concrete
variant. The slot reader already has discriminator support.

Suggested direction: parse TOML to `toml::Value`, use `TomlSyntaxSource` /
`SlotReader` to read `kind`, map to a concrete `SlotShapeId`, then call
`registry.read_slot_toml`. The returned boxed access object can be downcast to
the concrete type and wrapped in `NodeDef`.
