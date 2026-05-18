# Notes

## Scope

Plan the remaining M4 cleanup for slot codec generation:

- remove the hand-authored mockup codec schema from `lpc-slot-codegen`
- stop relying on mockup `from_codec` constructors as generated-code adapters
- generate mockup readers/writers from discovered `#[derive(SlotRecord)]`
  fields and explicit enum metadata
- keep codec behavior primitive/value focused, with shared helpers where useful
- validate through `lpc-model`, `lpc-wire`, and `lpc-slot-mockup`

Out of scope for this plan:

- production loader adoption
- removing serde from production crates
- extracting `lpc-slot` or `lpc-domain`
- general Serde-like customization

## Current State

The slot model cleanup is in a better place:

- `ValueSlot<T>` is the normal revision-tracked leaf container.
- semantic values implement `SlotValue + ToLpValue + FromLpValue`
- most former `FooSlot` wrappers are aliases like `pub type RatioSlot =
  ValueSlot<Ratio>`
- `#[derive(SlotRecord)]` requires public fields and no longer supports
  `#[slot(skip)]`
- `kind`/discriminator fields are not slot data
- generated views are emitted by default for slot records

The main remaining codec rough edge is in
`lp-core/lpc-slot-codegen/src/lib.rs`:

- `mockup_source_codec_module()` contains a hand-authored shadow schema
- `SlotCodecType` and `SlotCodecField` manually list mockup records and fields
- generated readers decode into plain local variables, then call mockup
  `from_codec` constructors
- generated writers mostly use hand-written accessor snippets

The mockup domain still exposes codec-only constructors:

- `source/output_def.rs`
  - `OutputDef::from_codec`
  - `OutputDriverOptionsConfig::from_codec`
- `source/texture_def.rs`
  - `TextureDef::from_codec`
- `source/fixture_def.rs`
  - `FixtureDef::from_codec`
- `source/shader_def.rs`
  - `ShaderDef::from_codec`
- `source/mapping.rs`
  - `MappingConfig::square_from_codec`

These constructors mostly do mechanical wrapping:

- `T -> ValueSlot<T>`
- `Option<T> -> OptionSlot<_>`
- `BTreeMap<K, V> -> MapSlot<K, V>`
- fill default or skipped fields such as `bindings` and `sampling`
- stamp revisions through normal slot constructors

That is generated-code responsibility, not domain-model responsibility.

## Relevant Files

- `lp-core/lpc-slot-codegen/src/lib.rs`
  - discovery functions:
    - `discover_static_registered_shapes`
    - `discover_static_slot_views`
    - `slot_view_fields`
    - `infer_type_path`
  - current codec generator:
    - `generate_mockup_slot_codec`
    - `mockup_source_codec_module`
    - `render_slot_codec_type_reader`
    - `render_slot_codec_type_writer`
    - `MOCKUP_SLOT_CODEC_REAL_HELPERS`
- `lp-core/lpc-slot-mockup/build.rs`
  - calls `generate_mockup_slot_codec`
- `lp-core/lpc-slot-mockup/src/source/*.rs`
  - mockup source records, enums, and codec-only constructors
- `lp-core/lpc-model/src/slot/value_slot.rs`
  - `ValueSlot`, `OptionSlot`, `MapSlot`
- `lp-core/lpc-model/src/slot/slot_access.rs`
  - `FieldSlot`, `SlotRecordAccess`, `SlotEnumAccess`
- `lp-core/lpc-model/src/slot_codec/*`
  - JSON/TOML syntax readers and writers used by generated codecs
- `lp-core/lpc-wire/src/slot/authored_toml.rs`
  - shape-driven TOML conversion; currently still in wire

## User Notes

- The point is for authoring types with slots to make everything else work.
- Hidden hand-coded tables defeat the purpose of the slot system.
- The system should stay simpler than Serde.
- Codegen should handle mechanical wrapping instead of domain `from_codec`
  helpers.
- Custom code is allowed when it is primitive-focused, explicit, and easy to
  find.
- Be mindful of generated-code verbosity and binary size; prefer shared helpers
  where that keeps generated code smaller.
- For now, focus on `lpc-model`, `lpc-wire`, and `lpc-slot-mockup`.

## Open Questions

### Q1. How should enum metadata be discovered?

Context: `MappingConfig` and `PathSpec` are enums with manual
`SlotEnumShape`, `SlotEnumAccess`, `SlotRecordAccess`, and `FieldSlot`
implementations. They are not `#[derive(SlotRecord)]`, so a pure record
discovery pass cannot know their variants and fields.

Suggested answer: keep enum codec generation explicit for this milestone.
Introduce a small, discoverable metadata table only for enums/discriminated
wrappers if needed, but do not let record field lists live there. The record
construction cleanup is the priority.

### Q2. Should generated readers decode directly into slot fields or plain
semantic values?

Context: current readers decode plain values such as `Dim2u` and `bool`, then
call `from_codec` to wrap them. That is why `from_codec` exists.

Suggested answer: generated readers should initialize and assign the actual
field type where possible:

```rust
let mut render_size = defaults.render_size.clone();
...
"render_size" => render_size = Dim2uSlot::new(read_dim2u(prop.value())?),
```

For nested records, generate nested record readers that return the record type.

### Q3. How should skipped/defaulted fields be represented now that
`#[slot(skip)]` is gone?

Context: fields like `bindings` and `sampling` exist in slot data but are not
always in the current authored/wire codec surface.

Suggested answer: this is codec policy, not slot shape policy. For the mockup,
allow a narrow generated-code policy that initializes omitted fields from
`Default` or field-specific default expressions. This policy must be visible in
the generator, not hidden in domain `from_codec` constructors.

### Q4. Should M4 move TOML helpers from `lpc-wire` to `lpc-model`?

Context: earlier design direction preferred codec code in `lpc-model`, but the
shape-driven authored TOML converter currently lives in `lpc-wire`.

Suggested answer: do not move it in this plan unless it blocks generated codec
cleanup. Keep M4 focused on eliminating shadow schemas and `from_codec`
adapters.

### Q5. Do we remove all `from_codec` functions at the end?

Context: some constructors may be useful for tests or domain ergonomics, but
the name `from_codec` exposes codec scaffolding in the domain.

Suggested answer: yes, remove codec-only constructors. Keep or rename only
human/domain constructors that are genuinely useful outside generated codecs.
