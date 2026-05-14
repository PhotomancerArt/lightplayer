# M3 Notes: Slot Storage Metadata

Date: 2026-05-13

## Scope

Design the slot-system metadata needed to remove storage policy that is
currently encoded only in Serde attributes or ad hoc loader/codec behavior.

The immediate goal is not to convert every root. The immediate goal is to make
the next production adoption target, probably `OutputDef`, possible without
copying Serde policy into handwritten code.

In scope:

- Audit source/domain serialization policy currently expressed with Serde.
- Decide how that policy belongs in slot shapes, slot field metadata, slot
  value metadata, or artifact envelope metadata.
- Plan derive/macro changes so root and nested slot objects can declare storage
  policy locally.
- Keep the generic TOML codec generic over slot metadata. It should not learn
  type-specific names such as `BindingDef`.
- Preserve the principle that persisted domain data is represented by slot
  roots, slot records/objects, maps, enums, options, or semantic slot leaves.

Out of scope for this milestone:

- Replacing all production node TOML loading.
- Replacing JSON protocol parsing beyond existing direct slot JSON coverage.
- Choosing a different TOML parser.
- Removing Serde derives wholesale.
- Firmware size measurement. That should happen after more than one real root
  uses the native path.

## User Notes To Preserve

- The slot system should become the source of truth for serialization.
- If the codec needs type-specific code for persisted domain data, that is a
  design smell. The `BindingDef` special case demonstrated this and should not
  be repeated.
- Persisted top-level artifacts should be slot roots. Nested persisted concepts
  should be slot objects or semantic leaves, not hidden `LpValue::Struct`
  conventions.
- The user wants to respond directly in this file before implementation plan
  details are finalized.

## Current State

### Existing Native Codec

`lp-core/lpc-wire/src/slot/authored_toml.rs` contains the current generic
authored TOML slot codec.

It currently knows how to walk:

- `SlotShape::Ref`
- `SlotShape::Unit`
- `SlotShape::Value`
- `SlotShape::Record`
- `SlotShape::Map`
- `SlotShape::Enum`
- `SlotShape::Option`

Current hardcoded storage conventions:

- Records are TOML tables.
- Maps are TOML tables.
- Enums use `kind = "<variant>"`.
- Missing `Unit`, `Map`, and `Option` fields default to unit/empty/none.
- Missing other fields are errors.
- Unknown record fields are rejected unless explicitly ignored by the caller.
- `Option::None` is skipped during encode.

The codec no longer contains `BindingDef`-specific code. That is good.

### Production Slice

`lp-core/lpc-engine/src/engine/project_loader.rs` routes `kind = "texture"`
child artifacts through the native slot TOML codec.

The current typed hydration for `TextureDef` is handwritten in
`ProjectLoader`. It handles:

- `TextureDef.size`
- `TextureDef.bindings`
- nested `BindingDef`
- nested `BindingEndpoint`

This is acceptable for the proof, but it should not scale to all roots.

### Binding Model Correction

`BindingDef` used to be a value leaf represented as an `LpValue::Struct` with
`direction` and `endpoint`. That forced authored TOML to special-case the real
format:

```toml
[bindings.input]
source = "bus#visual.out"
```

Current direction after correction:

- `BindingEndpoint` is a semantic string slot leaf.
- `BindingDef` is a static slot root/record with:
  - `source: OptionSlot<ValueSlot<BindingEndpoint>>`
  - `target: OptionSlot<ValueSlot<BindingEndpoint>>`
- `BindingDefs` maps slot names to `BindingDef` via `SlotShape::Ref`.

This establishes the desired pattern: domain shape belongs in `lpc-model`, not
in the serialization codec.

## Audit: Serde-Only Or Serde-First Serialization Policy

### 1. Field Defaults

Current examples:

- `ShaderDef.bindings`: `#[serde(default)]`
- `ShaderDef.glsl_opts`: `#[serde(default)]`
- `ShaderDef.param_defs`: `#[serde(default)]`
- `TextureDef.bindings`: `#[serde(default)]`
- `OutputDef.bindings`: `#[serde(default)]`
- `OutputDef.options`: `#[serde(default)]`
- `OutputDriverOptionsConfig` fields use `#[serde(default = "...")]`
- `FixtureDef.render_size`: `#[serde(default = "default_render_size")]`
- `FixtureDef.bindings`: `#[serde(default)]`
- `FixtureDef.sampling`: `#[serde(default)]`
- `FixtureDef.brightness`: `#[serde(default = "default_brightness")]`
- `FixtureDef.gamma_correction`: `#[serde(default = "default_gamma_correction")]`
- `GlslOpts` fields use `#[serde(default)]`
- `ShaderParamDef.min`: `#[serde(default)]`
- `BindingDef.source` and `BindingDef.target`: `#[serde(default)]`

Current slot situation:

- `SlotShape` has no default metadata.
- The TOML codec hardcodes defaults only for `Unit`, `Map`, and `Option`.
- Semantic defaults such as "fixture render size is 16x16" or "output LUT is
  enabled" live in Rust constructors/default functions and Serde attributes.

Proposed slot direction:

- Add default policy to field metadata, probably on `SlotFieldShape`, but do
  not store complex default values directly in `SlotShape`.
- Treat Rust `Default` as the preferred source of default values.
- Generated static slot support should be able to construct a default instance
  and extract default data by path through normal slot reflection.
- Conceptually:

```rust
trait StaticSlotDefaults {
    fn default_root_data() -> Option<SlotData>;
    fn default_data_at(path: &SlotPath, registry: &SlotShapeRegistry) -> Option<SlotData>;
}
```

- When a field is missing, the decoder/hydrator checks field metadata to see
  whether defaulting is allowed. If it is, it asks the generated default
  provider for the missing path.
- Defaults are path-relative:
  - a missing field in `OutputDef` asks an `OutputDef` default provider
  - a missing field inside present `[options]` asks an
    `OutputDriverOptionsConfig` default provider
- Simple structural defaults remain useful:
  - `Unit`
  - `EmptyMap`
  - `None`
- Attribute syntax should express policy, and may support simple literals as
  sugar, but should not become a second object-construction language:

```rust
#[slot(default)]
#[slot(default = "default_render_size")]
#[slot(default_value = 0)]
#[slot(default_some = true)]
```

Suggested answer:

- Add `SlotDefaultPolicy` metadata now, with "use generated default provider"
  as the main non-structural path.
- Use Rust `Default` wherever possible.
- Let generated slot/default support construct default instances and read
  missing fields through slot paths.
- Keep simple literal default attributes as optional convenience, not the core
  mechanism.

User response:

> Agreement: defaults should generally come from Rust `Default` / default
> instances, with generated slot support dynamically reading a default by path.
> The generated slot shape/support can have access to `T::default()`, while the
> shape itself remains schema data. This feels like the right direction.

### 2. Emit Policy: Skip Empty / None / Default

Current examples:

- `skip_serializing_if = "BindingDefs::is_empty"`
- `skip_serializing_if = "MapSlot::is_empty"`
- `skip_serializing_if = "OptionSlot::is_none"`
- default-valued output option fields are not currently skipped, but this is a
  likely future pressure point.

Current slot situation:

- The native TOML encoder skips `Option::None`.
- It does not know "skip empty map".
- It does not know "skip default".
- Empty `[bindings]` tables can appear from native encode even when Serde would
  omit them.

Proposed slot direction:

- Prefer a universal authored-storage emit rule for now instead of field-level
  metadata.
- Proposed global rules:
  - omit `Option::None`
  - omit empty maps
  - omit structurally empty records only when the record itself has no fields
  - do not omit scalar defaults yet
  - do not omit records merely because all of their child fields encoded as
    empty
  - do not omit enum payloads unless their actual payload encodes as empty/unit
- Add field-level emit metadata only if a real exception appears.

Suggested answer:

- Implement global authored TOML elision for structural empties.
- Keep `SlotEmitPolicy` out of the schema for M3.
- Revisit field-level emit metadata only when concrete model pressure needs it.

User response:

> Agreement: use a universal rule for now. Omit empty maps and none options
> globally. Avoid one-off field metadata unless this proves too one-size-fits
> all. Do not add scalar/default elision yet.

### 3. Loader / Envelope Metadata

Current examples:

- Top-level `kind = "project" | "shader" | "texture" | "output" | "fixture"`.
- `ProjectDef.kind` is `#[slot(skip)]`.
- Native TOML decode currently ignores top-level `kind` only when caller passes
  `ignored_fields = &["kind"]`.

Current slot situation:

- Slot roots do not describe their artifact discriminator.
- `NodeDef` is the loader envelope and delegates slot access to the concrete
  root.
- The codec has an "ignored fields" escape hatch, but no formal envelope model.

Proposed slot direction:

- The current `NodeDef` wrapper is a real Rust-native data shape:

```rust
pub enum NodeDef {
    Project(ProjectDef),
    Texture(TextureDef),
    Shader(ShaderDef),
    Output(OutputDef),
    Fixture(FixtureDef),
}
```

- Today it is not a slot root. It delegates `SlotAccess` to the wrapped
  concrete root.
- Consider modeling this directly in the slot system as a thin one-level enum
  wrapper whose variants reference concrete payload roots:

```text
NodeDef = Enum {
  project -> Ref(ProjectDef::SHAPE_ID)
  texture -> Ref(TextureDef::SHAPE_ID)
  shader  -> Ref(ShaderDef::SHAPE_ID)
  output  -> Ref(OutputDef::SHAPE_ID)
  fixture -> Ref(FixtureDef::SHAPE_ID)
}
```

- Authored TOML can still use the familiar inline payload shape:

```toml
kind = "texture"

[size]
width = 16
height = 16
```

- The discriminator selects the enum variant, then the payload decodes through
  the referenced concrete root shape.
- This lets `NodeDef` remain the contextual wrapper/allowlist while preserving
  concrete root identity.
- This is also the direction that could eventually support inline-or-reference
  child authoring:

```rust
enum NodeSpec {
    Ref(NodeInvocation),
    Texture(TextureDef),
    Shader(ShaderDef),
    Output(OutputDef),
    Fixture(FixtureDef),
}
```

- In that future shape, `ProjectDef.nodes` could become
  `MapSlot<String, NodeSpec>` and authored children could be either:

```toml
[nodes.shader]
artifact = "./shader.toml"
```

or:

```toml
[nodes.shader]
kind = "shader"
glsl_path = "shader.glsl"
```

- For M3, keep to one wrapper level. Do not design nested wrapper enums or a
  broad inline node migration yet.

Suggested answer:

- Prefer one-level slot enum wrapper support with referenced payload roots.
- Use `NodeDef` as the motivating production shape.
- Keep concrete node defs as canonical slot roots.
- Defer `NodeSpec` / inline-or-reference child nodes as future-compatible
  context, not M3 scope.

User response:

> Direction: model Rust wrapper enums in the slot system, but keep it to one
> level for now. `NodeDef` as a one-level enum wrapper with referenced concrete
> payload roots feels right. A future `NodeSpec` could represent either an
> artifact reference or an inline node definition, but wrapping/nesting these
> enums may get tricky and should stay out of this milestone.

### 4. Skipped Fields

Current examples:

- `#[slot(skip)] pub kind: String` on `ProjectDef`.
- `#[slot(skip)] pub sampling: FixtureSamplingConfig` on `FixtureDef`.
- `variant_revision` fields inside enum variants are `#[serde(skip, default)]`.

Current slot situation:

- `#[slot(skip)]` only means "not in slot shape/data".
- It does not explain storage semantics:
  - loader metadata
  - runtime-only default
  - revision bookkeeping
  - intentionally unsupported authored field

Proposed slot direction:

- Treat `#[slot(skip)]` as a design smell for persisted domain models.
- If a field exists in the Rust domain object but not in the slot model, the
  model becomes ambiguous:
  - where does the value come from during native deserialization?
  - is it loader metadata?
  - is it runtime-only?
  - should it be wire-visible?
  - should it be authored/persisted?
- Prefer one of these outcomes:
  - remove the field from the concrete root and represent it in a wrapper or
    context instead
  - make it normal persisted slot data
  - make it transient slot data
  - keep it as internal implementation detail outside the slot-shaped domain
    object
- Add `#[slot(transient)]` for fields that are slot-visible and wire-visible
  but omitted from authored/disk storage.
- This creates two clear axes:
  - slot visibility: in the slot model or not
  - storage surface: disk/authored vs wire/runtime sync
- Possible attribute semantics:

```rust
#[slot(transient)]
```

- `#[slot(transient)]` still contributes to shape/data access for wire/runtime
  sync, but authored storage encode/decode omits it.
- Current cases:
  - `ProjectDef.kind`: should move out of the concrete root and into the
    `NodeDef` wrapper/discriminator direction
  - `FixtureDef.sampling`: should be decided explicitly; either persisted
    slot data or transient slot data if it is a live/runtime-only control
  - `variant_revision`: should remain enum/container machinery, not a user
    slot field

Suggested answer:

- Avoid adding skip reason taxonomy.
- Deprecate or strongly discourage `#[slot(skip)]` on persisted domain model
  fields.
- Add `#[slot(transient)]` as the one real storage-surface distinction currently
  motivated by the model.
- Use wrappers/context/internal machinery for things that truly are not slot
  data.

User response:

> Direction: `#[slot(skip)]` feels like a hole in the model and should be
> avoided for persisted domain data. `#[slot(transient)]` is the honest concept:
> slot-visible and wire-visible, but not written to disk/authored storage.
> Current skipped fields should be re-evaluated rather than classified with a
> broad skip taxonomy.

### 5. Enum Discriminator Metadata

Current examples:

- `MappingConfig`: `#[serde(tag = "kind", rename_all = "snake_case")]`
- `PathSpec`: `#[serde(tag = "kind", rename_all = "snake_case")]`
- `FixtureSamplingConfig`: `#[serde(tag = "kind", rename_all = "snake_case")]`
- Native TOML codec assumes `kind`.

Current slot situation:

- `SlotShape::Enum` has variants with names.
- No enum storage metadata records discriminator field name.
- The codec hardcodes `kind`.

Proposed slot direction:

- Add enum storage metadata, probably inside enum `SlotMeta`:

```rust
pub struct SlotEnumStorageMeta {
    pub tag_field: SlotName, // usually "kind"
}
```

- Variant names should be explicit and should default to Rust enum variant
  names, not Serde-style case conversion.
- Canonical authored form should become:

```toml
[mapping]
kind = "PathPoints"
```

not:

```toml
[mapping]
kind = "path_points"
```

- This applies to slot enum variants such as `MappingConfig::PathPoints`,
  `PathSpec::RingArray`, and future wrapper enums such as `NodeDef::Texture`.
- Semantic leaf values are a separate concern. A semantic leaf such as
  `ColorOrder` may still choose compact domain strings such as `"rgb"` if that
  is the leaf's canonical value. The no-rename rule is specifically about slot
  enum variant discriminators.

Suggested answer:

- Add `tag_field`, defaulting to `"kind"` for now.
- Use Rust PascalCase variant names as canonical authored discriminator values.
- Remove `rename_all` thinking from slot enum storage.
- Do not add lowercase/snake_case aliases for current authored TOML. This code
  is in heavy development; update files and tests forward.

User response:

> Agreement: enum discriminators should use Rust variant names. Avoid
> `rename_all = "snake_case"` because it makes authored values less
> search-friendly and reads like variable names. Do not preserve compatibility
> aliases for old lowercase/snake_case authored names; upgrade everything.

### 6. Rename / Case / Alias Rules

Current examples:

- `#[serde(rename_all = "snake_case")]` on many enums.
- `#[serde(rename_all = "lowercase")]` on GLSL mode enums.
- `#[serde(rename_all = "UPPERCASE")]` on `TextureFormat`.
- `#[serde(rename = "node")]` in older source props.

Current slot situation:

- `SlotFieldShape.name` stores the field name used by slot traversal.
- `SlotVariantShape.name` stores enum variant names.
- Semantic leaves such as `AddSubMode` already encode their authored values in
  `ToLpValue` / `FromLpValue` and dropdown options.

Proposed slot direction:

- Prefer explicit authored names in slot metadata over case-conversion rules.
- For slot enum variants, section 5 answers the policy:
  - use Rust PascalCase variant names
  - no `rename_all`
  - no aliases for old names during current development
- For record fields, keep `SlotFieldShape.name` as the authored field name.
  Rust field names are already snake_case and align with current TOML field
  names.
- For semantic leaves, the leaf owns its canonical string representation.
  Examples:
  - `ColorOrder` may choose `"rgb"`
  - `RingOrder` may choose `"inner_first"` or could be revisited separately
  - shader mode leaves may choose `"wrapping"` / `"reciprocal"`

Suggested answer:

- Do not add broad `rename_all` machinery yet.
- Do not add aliases for current migration work.
- Revisit only if an external/stable file format compatibility need appears.

User response:

> Section 5 mostly answers this. Do not add case-conversion machinery. Use
> explicit slot names. Upgrade current authored files/tests rather than carrying
> aliases.

### 7. Transparent Wrappers

Current examples:

- `BindingDefs`: `#[serde(transparent)]`
- `Revision`: `#[serde(transparent)]`
- `ChannelName`, runtime ids, and several newtypes use transparent-like
  serialization.

Current slot situation:

- For source slot data, transparent wrapper behavior can usually be expressed
  by implementing `FieldSlot` for the wrapper.
- `BindingDefs` now exposes a map shape directly.

Proposed slot direction:

- Support transparent slot wrappers when the wrapped concept is atomic in the
  slot model.
- A Rust wrapper can be transparent when its whole authored/storage meaning is
  representable as a single conceptual slot value or container:
  - scalar leaf
  - semantic string leaf
  - structured `LpValue`
  - map wrapper
  - option wrapper
  - constrained semantic leaf
- The shape exposed by `FieldSlot` should be the conceptual shape, not
  necessarily the Rust wrapper's syntactic fields.
- Examples:
  - `BindingEndpoint` -> transparent semantic string leaf
  - `BindingDefs` -> transparent map wrapper
  - `Dim2uSlot` -> transparent value slot around `Dim2u`
  - `Dim2u` -> atomic structured `LpValue`
  - `BindingDef` -> not transparent, because `source` vs `target` is authored
    structure
  - `OutputDriverOptionsConfig` -> record
  - `NodeDef` -> enum wrapper
- If a transparent wrapper later needs independent fields, partial updates, or
  authored substructure, it should graduate to a record or enum.

Suggested answer:

- Keep transparent wrappers as a first-class slot modeling pattern.
- Use transparency where the concept can be expressed as one `LpValue` or one
  container.
- Do not use transparency to hide real authored structure.

User response:

> Agreement: things that can be expressed as an `LpValue` should generally be
> transparent where possible. The key test is whether the concept is atomic in
> the slot model. Atomic concepts can be transparent leaves/wrappers; structured
> concepts should be records/enums.

### 8. Unknown Field Policy

Current examples:

- `BindingDef`: `#[serde(deny_unknown_fields)]`
- `Constraint` types: `#[serde(deny_unknown_fields)]`
- Native TOML codec rejects unknown record fields globally.

Current slot situation:

- Unknown fields are always rejected by the native TOML codec except
  caller-provided ignored fields.
- There is no per-root or per-record policy.

Proposed slot direction:

- Keep strict unknown handling until schema/version compatibility is formalized.
- Authored storage:
  - unknown TOML fields are errors
  - unknown enum variants are errors
  - map keys are data, not schema, so arbitrary map keys remain valid if they
    parse as the map key type
- Wire/project messages need extra care because slot data may depend on shape
  updates:
  - apply or validate shape registry updates before applying slot data that
    references them
  - reject slot data whose `SlotShapeId` is unknown
  - reject unknown fields in typed protocol envelopes until message schema
    versioning exists
  - future old-client/new-server or new-client/old-server compatibility should
    use explicit schema versioning/capability negotiation, not ad hoc unknown
    field ignores

Suggested answer:

- Default to `Reject` for authored source roots.
- Do not add `Preserve` until there is a migration/round-trip requirement.
- Replace `ignored_fields` with envelope-owned fields rather than general
  record ignores.

User response:

> Agreement: unknown/unexpected data should be an error until schema versioning
> is formalized. Be especially careful for project/wire messages because they
> may include new shapes that affect the rest of serialization. Shape updates
> must be known/applied before dependent slot data is accepted.

### 9. Untagged / Peer-Key Inference

Current examples:

- `Constraint` uses `#[serde(untagged)]`.
- Some older `lpc-source::prop` types infer shape from peer keys.

Current slot situation:

- `SlotShape::Enum` is explicitly tagged.
- No untagged enum support exists in the native codec.

Proposed slot direction:

- Keep core source node roots explicitly tagged.
- Avoid adding untagged enum support to the embedded native codec unless a
  migrated root truly needs it.
- Treat peer-key inference as a specialized legacy/parser concern, not a
  generic slot serialization feature.

Suggested answer:

- Defer. If old prop models need compatibility, handle them with a migration
  layer into explicit slot-shaped data.
- Generic slot enum storage should stay explicit and tagged.

User response:

> Agreement: avoid untagged/peer-key inference. Prefer explicit tagged data now.

### 10. Custom Compact Leaf Syntax

Current examples:

- `BindingEndpoint` compact strings.
- `ArtifactPathSlot`, `SourcePathSlot`, `RelativeNodeRefSlot`, `ResourceRefSlot`.
- String-backed enums like `ColorOrder`, `RingOrder`, `AddSubMode`, `MulMode`,
  `DivMode`.

Current slot situation:

- Semantic leaves use `SlotValueShape`, `ToLpValue`, and `FromLpValue`.
- The native TOML codec decodes scalar strings into `LpValue::String`.
- Typed validation happens later during hydration or typed construction.

Proposed slot direction:

- Keep compact leaf syntax at the `SlotValueShape`/semantic leaf layer.
- Consider adding a static leaf parser/formatter id only if diagnostics or
  generated hydration need it.
- This is largely covered by the transparent/atomic wrapper policy in section
  7:
  - atomic concepts become semantic slot leaves
  - semantic leaves own their compact `LpValue` representation
  - generic codecs see only `LpType` / `LpValue`
  - type-specific validation happens through `FromLpValue` / generated
    hydration

Suggested answer:

- Do not make the generic codec know semantic leaves.
- Add enough metadata for tools to know "this is a path", "this is a node ref",
  etc.; much of this already exists in `ValueEditorHint`.
- Defer additional parser/formatter ids until diagnostics need them.

User response:

> Covered by section 7. Keep compact syntax in semantic leaf types, not in
> codecs. Additional diagnostic metadata can wait.

### 11. Literal Binding Endpoints

Current examples:

- Serde supports `{ literal = ... }` for `BindingEndpoint::Literal`.
- Native slot model now treats `BindingEndpoint` as a compact string leaf.

Current slot situation:

- Literal endpoints are not represented by the current `BindingEndpoint` slot
  leaf storage.
- `BindingDef::validate` already rejects literal targets but allows literal
  sources.

Proposed slot direction:

Options:

1. Keep `BindingEndpoint` as compact ref-only leaf for native storage and move
   literal source bindings elsewhere.
2. Make `BindingEndpoint` a slot enum over the real storage distinction:
   - `Ref(BindingRef)`
   - `Value(LpValue)`
3. Split binding source and target endpoint types:
   - source endpoint can be literal/ref
   - target endpoint can only be ref

Current direction:

- Prefer option 2.
- Do not split the enum into `Bus` and `Node`; that duplicates information that
  belongs to the ref string language.
- Introduce a semantic ref leaf such as `BindingRef` that can parse the current
  string format:

```text
bus#visual.out
..shader#output
```

- The string format is likely to be revisited later, possibly toward something
  like:

```text
bus:visual.out
node:..shader:output
```

- The important point is that bus-vs-node can be inferred from the string by
  `BindingRef`, so the slot enum only needs to distinguish `Ref` from `Value`.

Potential authored forms:

Regular tagged enum form:

```toml
[bindings.input.source]
kind = "Ref"
value = "bus#visual.out"
```

Compact single-value enum form:

```toml
source = { ref = "bus#visual.out" }
source = { value = 123 }
```

- The compact form is attractive because it is much nicer TOML and remains
  explicit: the variant key is present.
- This is an intentional exception to the PascalCase discriminator rule for
  normal tagged enums. In normal tagged form, `kind = "PathPoints"` reads like a
  type/variant value. In compact external-tag form, `{ ref = "..." }` reads like
  a field key.
- Do not implement this as broad `rename_all`. Require explicit external tag
  names when a compact enum style wants lowercase/domain-facing keys.
- This should not be a `BindingEndpoint` special case. If supported, it should
  be a general enum storage style, explicitly enabled in slot enum metadata.
- Possible enum storage metadata:

```rust
pub enum SlotEnumStorageStyle {
    Tagged { tag_field: SlotName },
    SingleValueExternalTag,
}
```

- Rules for `SingleValueExternalTag`:
  - only enabled explicitly
  - table/object contains exactly one key
  - key is the explicit external tag name for the variant
  - value is the variant payload
  - only valid for variants whose payload is unit or a single value/field
  - this is not untagged inference because the variant name is explicit

Suggested answer:

- Probably support compact single-value enum storage as a general enum storage
  style, not a type-specific codec branch.
- `BindingEndpoint` should become a `Ref`/`Value` slot enum when we model
  literal bindings honestly.
- Keep target validation rejecting `Value`.
- The exact ref string format is a later problem.

User response:

> Direction: model `BindingEndpoint` as `Ref`/`Value`, not `Bus`/`Node`/
> `Literal`. Bus-vs-node belongs to the reference string language and can be
> inferred by a semantic `BindingRef` leaf. Compact TOML such as
> `{ ref = "..." }` and `{ value = 123 }` is worth supporting if feasible as an
> explicitly enabled general enum storage style for single-value variants. Use
> lowercase/domain-facing external tag names in this compact form because they
> read like inline table fields. Keep normal tagged enum discriminators
> PascalCase. The ref string format itself will likely be revisited later.

## Open Questions

### Q1. Should slot metadata model exact Serde defaults, or only authored storage defaults?

Context:

- Serde defaults currently serve both "load omitted authored field" and
  "construct a convenient Rust value".
- Slot storage needs only the authored load/write behavior.

Suggested direction:

- Model authored storage defaults in slot metadata.
- Keep Rust constructor defaults separate unless they are intentionally part of
  authored storage.
- Use Rust `Default` as the actual default-value source when the authored
  storage policy says a field may be omitted.

User response:

> Model the authored storage policy in slot metadata, but use generated access
> to Rust `Default` instances for actual default values. Do not duplicate full
> default values into portable slot shape metadata.

### Q2. Should `#[slot(skip)]` become a family of skip reasons now?

Context:

- The current skip is ambiguous.
- `kind`, `sampling`, and enum revision fields all have different semantics.

Suggested direction:

- No. Avoid a skip-reason taxonomy for now.
- Treat `#[slot(skip)]` as a design smell in persisted domain models.
- Add `#[slot(transient)]` for the concrete need: slot-visible/wire-visible
  data that is omitted from disk/authored storage.

User response:

> Do not add skip reasons now. `#[slot(skip)]` feels like it breaks the model
> because it creates Rust fields with unclear serialization/source semantics.
> The real motivated concept is `#[slot(transient)]`: in the slot model and on
> the wire, but never written to authored disk storage. Current skipped fields
> should be re-evaluated.

### Q3. Should the next real root be `OutputDef`?

Context:

- `TextureDef` proved maps and nested binding records.
- `OutputDef` exercises `OptionSlot<OutputDriverOptionsConfig>` and nested
  defaulted fields.
- It avoids the full complexity of shader params and fixture mapping.

Suggested direction:

- Yes. Use `OutputDef` as the next adoption target after metadata exists.

User response:

> Yes. Use `OutputDef` as the next real root after the metadata design is in
> place.

### Q4. How much compatibility with old authored TOML should native storage preserve?

Context:

- Some example texture TOML files still use old top-level `width`/`height`.
- Native texture loading currently supports the current `size` shape only.
- Preserving old shapes inside the generic codec would add complexity.

Suggested direction:

- Keep compatibility as explicit migrations or root-specific pre-decode
  adapters, not generic slot codec behavior.
- During current heavy development, prefer upgrading authored files/tests
  forward rather than carrying compatibility aliases or legacy shapes.

User response:

> Do not preserve old authored TOML compatibility in the generic slot codec.
> Upgrade files/tests forward. If compatibility becomes necessary later, handle
> it as explicit migration or root-specific adaptation.

### Q5. Should generated hydration be part of this milestone?

Context:

- Handwritten `TextureDef` hydration in `ProjectLoader` is already showing the
  scaling problem.
- The default metadata work is tightly connected to hydration.
- There are two different serialization surfaces:
  - TOML for disk/authored storage
  - JSON for messages/wire
- TOML-authored artifacts are expected to be relatively small.
- JSON messages can become much larger, especially when moving resource data.
- RAM pressure is real on the target. A running LightPlayer instance has been
  observed around `mem 137k free / 174k used`. A single 64x64 RGBA unorm16
  native image is roughly 32 KiB. Triple-buffering JSON bytes, `SlotData`, and
  the final object can consume roughly 96 KiB before considering shader
  compilation or other runtime allocations.

Suggested direction:

- Avoid making `SlotData` the only construction path.
- Consider a syntax-level construction event stream that generated slot code can
  build typed objects from. The stream source should not need to know the target
  slot shape. It only knows the syntax it is parsing.
- TOML path can parse into `toml::Value` first, then expose that value tree
  through the same reader/event abstraction. This is acceptable because authored
  TOML is small and TOML has awkward table/dotted-key back-writing semantics.
- JSON path should be able to parse directly into construction events without
  materializing both JSON and `SlotData`, because JSON is cleaner to stream and
  may carry larger messages.
- `SlotData` remains useful as:
  - a reference/test representation
  - wire sync state where appropriate
  - client-side or host-side tooling where memory is less constrained
- The generated construction path should share semantics with the `SlotData`
  path so the project does not grow two serialization languages.
- A possible low-level syntax event shape:

```text
start_object
prop(key)
end_object
start_array
end_array
string_chunk(...)
number(...)
bool(...)
null
```

- A higher-level generated-reader API can sit on top of those events and expose
  slot/domain operations:

```rust
Self {
    brightness: reader.prop("brightness")?.f32()?,
    mapping: reader.prop("mapping")?.slot_root("Mapping")?,
}
```

- The generated code knows the target shape and asks for props, maps, enum
  tags, values, defaults, etc. The syntax source only feeds objects, arrays,
  props, strings, numbers, and similar syntax events.
- Strings should be chunked, probably around 1 KiB, so large payloads do not
  require one contiguous temporary string unless the target asks for one.
- Binary/resource data may use a conventional syntax such as an inline
  length-prefixed base64 tuple:

```json
[8192, "base64data"]
```

This lets the reader allocate the destination buffer up front and decode into
it without partial-string weirdness.
- A separate binary transfer path may still be useful later, but the event
  reader should not force triple buffering for base64 payloads.

User response:

> Concern: `SlotData` as an intermediary may be too expensive on embedded,
> especially for large JSON/resource messages. TOML artifacts are usually small,
> so TOML -> `toml::Value` -> construction stream is acceptable. JSON should be
> designed for direct streaming into generated builders. The stream should be
> syntax-level, not slot-shape-level: `start_object`, `prop(key)`,
> `start_array`, chunked strings, numbers, bools, null, etc. A higher-level
> reader used by generated code can provide slot/domain semantics such as
> `reader.prop("brightness")?.f32()?` or `reader.prop("mapping")?.slot_root(...)`.
> Binary data can use a length-prefixed base64 tuple like `[8192, "base64data"]`
> so the destination buffer can be allocated once and filled directly.
> `SlotData` should remain useful for tests/reference/tooling, but should not be
> the only production construction path.

## Proposed Next Step After User Review

After these responses are reviewed, produce:

1. `00-design.md` with the chosen storage metadata architecture.
2. Phase files covering:
   - slot metadata types
   - derive/macro attribute support
   - TOML codec behavior changes
   - `OutputDef` native loader adoption
   - cleanup/final validation
